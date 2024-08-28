pub mod auth;
pub mod game;
pub mod lobby;

use axum::http::StatusCode;

pub async fn fallback_handler() -> (StatusCode, &'static str) {
    NOT_FOUND_RESPONSE
}

const NOT_FOUND_RESPONSE: (StatusCode, &str) =
    (StatusCode::NOT_FOUND, "this resource doesn't exist");
