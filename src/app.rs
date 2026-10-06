use axum::{extract::State, http::StatusCode, routing::get, Router};
use sqlx::PgPool;

pub fn build_router(pool: PgPool) -> Router {
    Router::new()
        .route("/_health", get(health_check))
        .with_state(pool)
}

#[axum::debug_handler]
async fn health_check(State(pool): State<PgPool>) -> StatusCode {
    let _ = sqlx::query("SELECT 1").fetch_one(&pool).await;
    StatusCode::OK
}
