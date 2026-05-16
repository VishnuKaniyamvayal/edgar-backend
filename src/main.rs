mod config;
mod db;
mod handlers;
mod models;
mod prompt;
mod repository;
mod routes;
mod state;

use axum::http::{HeaderValue, Method};
use std::sync::Arc;

use config::Config;
use state::AppState;
use tower_http::cors::{Any, CorsLayer, AllowOrigin};

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let config = Config::from_env();

    // Connect to Supabase Postgres
    let pool = db::create_pool().await;
    println!("Connected to database");

    let shared_state = Arc::new(AppState::new(pool));
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::list([
            HeaderValue::from_static("https://cheerful-cupcake-e47628.netlify.app"),
            HeaderValue::from_static("http://localhost:5173"),
            HeaderValue::from_static("http://127.0.0.1:5173"),
        ]))
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers(Any);

    let app = routes::create_router(shared_state).layer(cors);

    let listener = tokio::net::TcpListener::bind(config.addr()).await.unwrap();
    println!("Listening on: http://{}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}