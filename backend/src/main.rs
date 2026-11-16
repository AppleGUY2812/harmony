use rocket::fairing::AdHoc;
use rocket::fs::FileServer;
use rocket::http::{ContentType, Header, Method, Status};
use rocket_cors::{AllowedOrigins, CorsOptions};
use rocket_db_pools::Database;
use utils::error_catchers::not_authorized;

#[macro_use]
extern crate rocket;

mod database;
mod models;
mod routes;
mod sfu;

mod controllers;
mod middleware;
mod utils;

#[get("/")]
async fn index() -> &'static str {
    "bingus"
}

#[launch]
async fn rocket() -> _ {
    let _ = dotenv::dotenv().ok();

    let cors = CorsOptions::default()
        .allowed_origins(AllowedOrigins::all())
        .allowed_methods(
            vec![
                Method::Get,
                Method::Post,
                Method::Put,
                Method::Patch,
                Method::Options,
            ]
            .into_iter()
            .map(From::from)
            .collect(),
        )
        .allow_credentials(true)
        .to_cors()
        .unwrap();

    let storage = utils::aws_s3_utils::Storage::from_env().await;
    let upload_dir = match &storage {
        utils::aws_s3_utils::Storage::Local(dir) => Some(dir.clone()),
        utils::aws_s3_utils::Storage::S3 { .. } => None,
    };

    let rocket = routes::build()
        .attach(database::HarmonyDb::init())
        .mount("/", rocket_cors::catch_all_options_routes())
        .attach(cors.clone())
        .manage(cors)
        .manage(std::sync::Arc::new(utils::hub::Hub::default()))
        .manage(storage)
        .manage(sfu::Sfu::from_env().await.expect("failed to start SFU"))
        .manage(utils::cloudfront::CloudFrontSigner::from_env())
        .register("/", catchers![not_authorized])
        .mount("/", routes![index]);

    // No S3: serve uploaded icons from disk. Keys are unique per upload, so cache forever.
    match upload_dir {
        Some(dir) => {
            std::fs::create_dir_all(&dir).expect("failed to create UPLOAD_DIR");
            rocket
                .mount("/images", FileServer::from(dir))
                .attach(AdHoc::on_response("image cache", |req, res| {
                    Box::pin(async move {
                        if res.status() == Status::Ok && req.uri().path().starts_with("/images/") {
                            res.set_header(Header::new(
                                "Cache-Control",
                                "public, max-age=31536000, immutable",
                            ));
                            // keys have no extension; every stored object is webp
                            res.set_header(ContentType::new("image", "webp"));
                        }
                    })
                }))
        }
        None => rocket,
    }
}
