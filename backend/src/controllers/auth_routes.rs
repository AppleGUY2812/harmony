use crate::database;
use crate::models;

use crate::models::ErrorResponse;
use crate::utils::{generate_token, json_error::json_error};
use rocket::http::{CookieJar, Status};
use rocket::response::Responder;
use rocket::serde::json::Json;
use rocket::State;
use rocket_db_pools::sqlx::PgConnection;
use rocket_db_pools::Connection;
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use validator::Validate;

use argon2::{
    password_hash::{
        rand_core::{OsRng, RngCore},
        PasswordHash, PasswordHasher, PasswordVerifier, SaltString,
    },
    Argon2,
};

// type Result<T, E = CustomError> = std::result::Result<T, E>;

// pub enum BadRequestError {
//     BadRequest(status::BadRequest<&'static str>),
// }

// #[derive(Responder)]
// pub enum InternalError {
//     #[response(status = 400, content_type = "json")]
//     ValidationFailed(String),
//     DatabaseError(rocket_db_pools::sqlx::Error),
// }

// impl From<ValidationErrors> for CustomError {
//     fn from(error: ValidationErrors) -> Self {
//         CustomError::ValidationFailed(error)
//     }
// }
// impl From<sqlx::Error> for CustomError {
//     fn from(error: rocket_db_pools::sqlx::Error) -> Self {
//         CustomError::DatabaseError(error)
//     }
// }

// impl<'r> Responder<'r, 'r> for CustomError {
//     fn respond_to(self, _: &Request) -> response::Result<'r> {
//         Err(Status::InternalServerError)
//     }
// }

// #[derive(Responder)]
// pub enum CustomResponse {
//     #[response(status = 200, content_type = "json")]
//     Approved(Json<i64>),
//     #[response(status = 400, content_type = "json")]
//     BadRequest(String),
//     #[response(status = 500, content_type = "json")]
//     InternalServerError(String),
// }

// pub enum CustomError {}

#[post("/signup", format = "json", data = "<user>")]
pub async fn signup<'a>(
    user: Json<models::UserSignupInput<'_>>,
    mut db: Connection<database::HarmonyDb>,
    jar: &'a CookieJar<'_>,
) -> impl Responder<'a, 'a> {
    // json input:
    // email, username, password, confirmPassword
    // json inserted:
    // email, username, password, profile_picture
    // json returned:
    // id, username, profile_picture

    user.validate()
        .map_err(|_| (Status::BadRequest, json_error("Failed JSON validation")))?;

    if user.confirm_password != user.password {
        return Err((Status::BadRequest, json_error("Passwords do not match")));
    }

    let username_exists = sqlx::query_scalar!(
        "SELECT 1 FROM users 
        WHERE username = $1",
        user.username.to_lowercase()
    )
    .fetch_optional(&mut **db)
    .await
    .map_err(|_| (Status::BadRequest, json_error("Database error")))?;

    if username_exists.is_some() {
        return Err((Status::BadRequest, json_error("Username already exists")));
    }

    let user_response =
        create_user(&mut db, user.email, user.username, user.password.as_bytes()).await?;

    let cookie = generate_token::jwt_cookie(user_response.user_id);
    jar.add(cookie);

    // Ok(status::Created::new("/signup").body(Json(user_response)))
    Ok((Status::Created, Json(user_response)))
}

// Hashes the password and inserts the user; a taken username is a 409.
async fn create_user(
    db: &mut PgConnection,
    email: &str,
    username: &str,
    password: &[u8],
) -> Result<models::User, (Status, Json<ErrorResponse>)> {
    let salt = SaltString::generate(&mut OsRng);
    let password_hash = Argon2::default()
        .hash_password(password, &salt)
        .map_err(|_| {
            (
                Status::InternalServerError,
                json_error("Could not hash password"),
            )
        })?
        .to_string();

    sqlx::query_as!(
        models::User,
        "INSERT INTO users (email, username, display_username, hashed_password)
        VALUES ($1, $2, $3, $4)
        RETURNING user_id, display_username, s3_icon_key, date_joined",
        email,
        username.to_lowercase(),
        username,
        password_hash,
    )
    .fetch_one(db)
    .await
    .map_err(|e| match e.as_database_error() {
        Some(e) if e.is_unique_violation() => {
            (Status::Conflict, json_error("Username already exists"))
        }
        _ => (
            Status::InternalServerError,
            json_error("Database insertion failed"),
        ),
    })
}

const GUEST_LIMIT: usize = 10;
const GUEST_WINDOW: Duration = Duration::from_secs(60 * 60);

// ponytail: per-process and in-memory, so it resets on restart and isn't shared
// across replicas; move to Redis/Postgres if the backend ever scales out.
#[derive(Default)]
pub struct GuestLimiter(Mutex<HashMap<IpAddr, Vec<Instant>>>);

impl GuestLimiter {
    pub fn allow(&self, ip: IpAddr, now: Instant) -> bool {
        let mut map = self.0.lock().unwrap();
        map.retain(|_, hits| {
            hits.retain(|t| now.duration_since(*t) < GUEST_WINDOW);
            !hits.is_empty()
        });
        let hits = map.entry(ip).or_default();
        if hits.len() >= GUEST_LIMIT {
            return false;
        }
        hits.push(now);
        true
    }
}

// IpAddr comes from Request::client_ip(), which trusts Rocket's default
// ip_header (X-Real-IP); nginx.conf and demo/Caddyfile overwrite it with the
// real client address. format = "json" keeps cross-site HTML forms out.
#[post("/guest", format = "json")]
pub async fn guest<'a>(
    ip: IpAddr,
    limiter: &State<GuestLimiter>,
    mut db: Connection<database::HarmonyDb>,
    jar: &'a CookieJar<'_>,
) -> impl Responder<'a, 'a> {
    if !limiter.allow(ip, Instant::now()) {
        return Err((
            Status::TooManyRequests,
            json_error("Too many guest accounts, try again later"),
        ));
    }

    // random password nobody knows, so the account can't be logged into
    let mut password = [0u8; 32];
    OsRng.fill_bytes(&mut password);

    for _ in 0..5 {
        let username = format!("Guest-{}", &uuid::Uuid::new_v4().simple().to_string()[..6]);
        match create_user(&mut db, "guest@harmony.invalid", &username, &password).await {
            Err((status, _)) if status == Status::Conflict => continue,
            result => {
                let user = result?;
                jar.add(generate_token::jwt_cookie(user.user_id));
                return Ok(Json(user));
            }
        }
    }

    Err((
        Status::InternalServerError,
        json_error("Could not create guest account"),
    ))
}

#[post("/login", format = "json", data = "<user>")]
pub async fn login<'a>(
    user: Json<models::UserLoginInput<'_>>,
    mut db: Connection<database::HarmonyDb>,
    jar: &'a CookieJar<'_>,
) -> impl Responder<'a, 'a> {
    let user_data = sqlx::query!(
        "SELECT *
        FROM users 
        WHERE username = $1",
        user.username.to_lowercase()
    )
    .fetch_optional(&mut **db)
    .await
    .map_err(|_| (Status::BadRequest, json_error("Database error")))?
    .ok_or((Status::NotFound, json_error("Username doesn't exist")))?;

    let user_id = user_data.user_id;
    let display_username = user_data.display_username;
    let hashed_password = user_data.hashed_password;
    let s3_icon_key = user_data.s3_icon_key;
    let date_joined = user_data.date_joined;

    // check password correct
    let parsed_hash = PasswordHash::new(hashed_password.as_str()).map_err(|_| {
        (
            Status::InternalServerError,
            json_error("Could not parse hash"),
        )
    })?;
    if Argon2::default()
        .verify_password(user.password.as_bytes(), &parsed_hash)
        .is_err()
    {
        return Err((Status::BadRequest, json_error("Password incorrect")));
    }

    let cookie = generate_token::jwt_cookie(user_id);
    jar.add(cookie);

    let response = Json(models::User {
        user_id,
        display_username,
        s3_icon_key,
        date_joined,
    });

    Ok(response)
}

#[post("/logout")]
pub fn logout(jar: &CookieJar<'_>) -> Status {
    jar.remove("JWT");

    // "logged out successfully".to_string()
    Status::NoContent
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guest_limiter_caps_per_ip_per_window() {
        let limiter = GuestLimiter::default();
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        let now = Instant::now();

        for _ in 0..GUEST_LIMIT {
            assert!(limiter.allow(ip, now));
        }
        assert!(!limiter.allow(ip, now));
        assert!(limiter.allow("5.6.7.8".parse().unwrap(), now));
        assert!(limiter.allow(ip, now + GUEST_WINDOW));
    }
}
