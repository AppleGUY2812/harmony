use crate::middleware::protect_route::JwtGuard;
use crate::utils::cloudfront::{bucket_expiry, CloudFrontSigner};
use rocket::serde::json::{json, Json, Value};
use rocket::State;

// Signed query string to append to CloudFront image URLs. Empty when signing is not configured.
#[get("/signature")]
pub fn get_signature(_guard: JwtGuard, signer: &State<Option<CloudFrontSigner>>) -> Json<Value> {
    let (query, expires_at) = match signer.inner() {
        Some(signer) => signer.signed_query(),
        None => (String::new(), bucket_expiry()),
    };
    Json(json!({ "query": query, "expires_at": expires_at }))
}
