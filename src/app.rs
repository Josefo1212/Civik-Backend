use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Router,
};
use sqlx::PgPool;
use tracing::warn;

use crate::modules::events::handler as events;
use crate::modules::lapses::handler as lapses;
use crate::modules::sections::handler;
use crate::modules::students::handler as estudiantes;

pub fn build_router(pool: PgPool) -> Router {
    Router::new()
        .route("/_health", get(health_check))
        .route("/sections", post(handler::crear).get(handler::listar))
        .route(
            "/sections/{id}",
            get(handler::consultar)
                .put(handler::editar)
                .delete(handler::eliminar),
        )
        .route("/sections/{id}/baja", post(handler::baja))
        .route("/sections/{id}/reactivar", post(handler::reactivar))
        .route(
            "/sections/{id}/students",
            post(estudiantes::crear).get(estudiantes::listar),
        )
        .route(
            "/students/{id}",
            get(estudiantes::consultar).put(estudiantes::editar),
        )
        .route("/students/{id}/baja", post(estudiantes::baja))
        .route("/students/{id}/reactivar", post(estudiantes::reactivar))
        .route("/lapses", post(lapses::crear).get(lapses::listar))
        .route(
            "/lapses/{id}",
            get(lapses::consultar)
                .patch(lapses::editar)
                .delete(lapses::eliminar),
        )
        .route("/lapses/{id}/activar", post(lapses::activar))
        .route("/lapses/{id}/cerrar", post(lapses::cerrar))
        .route("/lapses/{id}/generar-practicas", post(events::generar_auto))
        .route(
            "/lapses/{id}/reprogram-practice-day",
            post(events::reprogramar),
        )
        .route("/events", post(events::crear).get(events::listar))
        .route("/events/{id}", get(events::consultar).patch(events::editar))
        .route("/events/{id}/cancel", post(events::cancelar))
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
