pub mod admin;
pub mod auth;
pub mod cart;
pub mod home;
pub mod listings;
pub mod orders;
pub mod seller;
pub mod wallet;

use axum::http::StatusCode;

/// Health check endpoint
pub async fn health() -> StatusCode {
    StatusCode::OK
}
