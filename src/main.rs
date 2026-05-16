mod config;
mod db;
mod handlers;
mod models;
mod prompt;
mod repository;
mod routes;
mod state;

use std::sync::Arc;

use config::Config;
use state::AppState;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let config = Config::from_env();

    // Connect to Supabase Postgres
    let pool = db::create_pool().await;
    println!("Connected to database");

    let shared_state = Arc::new(AppState::new(pool));
    let app = routes::create_router(shared_state);

    let listener = tokio::net::TcpListener::bind(config.addr()).await.unwrap();
    println!("Listening on: http://{}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}