use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::PgPool;

/// Creates and returns a connection pool to the Supabase Postgres database.
/// The DATABASE_URL env var should be your Supabase connection string:
/// postgresql://postgres:<password>@db.<project>.supabase.co:5432/postgres
pub async fn create_pool() -> PgPool {
    let database_url = dotenvy::var("DATABASE_URL")
        .expect("DATABASE_URL must be set in .env");

    // Supabase transaction poolers (pgbouncer) can error with prepared statements.
    // Disable sqlx statement cache to avoid "prepared statement already exists".
    let connect_options = database_url
        .parse::<PgConnectOptions>()
        .expect("Invalid DATABASE_URL")
        .statement_cache_capacity(0);

    PgPoolOptions::new()
        .max_connections(5)
        .connect_with(connect_options)
        .await
        .expect("Failed to connect to database")
}
