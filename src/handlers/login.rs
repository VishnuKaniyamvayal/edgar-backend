use axum::{http::StatusCode, Json};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct LoginResponse {
    pub email: String,
}

pub async fn login_handler(
    Json(payload): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, StatusCode> {
    if payload.email == "1@1.com" && payload.password == "1" {
        Ok(Json(LoginResponse { email: payload.email }))
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}
