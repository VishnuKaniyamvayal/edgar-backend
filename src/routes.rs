use axum::{
    routing::{get, post},
    Router,
};
use std::sync::Arc;

use crate::handlers::{chat_handler, login_handler};
use crate::state::AppState;

pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(|| async { "Welcome to Edgar!" }))
        .route("/chat", post(chat_handler))
        .route("/api/login", post(login_handler))
        .with_state(state)
}
