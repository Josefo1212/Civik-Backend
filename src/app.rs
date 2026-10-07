use axum::{extract::State, http::StatusCode, routing::get, Router};
use sqlx::PgPool;
use tracing::warn;

pub fn build_router(pool: PgPool) -> Router {
    Router::new()
        .route("/_health", get(health_check))
        .with_state(pool)
}

#[axum::debug_handler]
async fn health_check(State(pool): State<PgPool>) -> StatusCode {
    match sqlx::query("SELECT 1").fetch_one(&pool).await {
        Ok(_) => StatusCode::OK,
        Err(error) => {
            warn!(%error, "[Health] la base de datos no respondió");
            StatusCode::SERVICE_UNAVAILABLE
        }
    }
}
