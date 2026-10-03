use sqlx::postgres::{PgPool, PgPoolOptions};
use std::env;

pub async fn create_pool() -> PgPool {
    dotenvy::dotenv().ok();

    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| panic!("[Config] Falta la variable de entorno DATABASE_URL"));

    PgPoolOptions::new()
        .max_connections(10)
        .connect(&database_url)
        .await
        .unwrap_or_else(|error| panic!("[Database] No se pudo conectar a PostgreSQL: {error}"))
}
