use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::env;

/// Pool para los tests de integración. No usa `config::database::create_pool`
/// porque los módulos del binario no son visibles desde `tests/`.
pub async fn pool() -> Result<PgPool, Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    let database_url = env::var("DATABASE_URL")?;
    Ok(PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await?)
}
