use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Router,
};
use sqlx::PgPool;
use tracing::warn;

use crate::modules::attendance::handler as attendance_handler;
use crate::modules::events::handler as events;
use crate::modules::grades::handler as grades_handler;
use crate::modules::lapses::handler as lapses;
use crate::modules::sections::handler as sections_handler;
use crate::modules::students::handler as estudiantes;

pub fn build_router(pool: PgPool) -> Router {
    Router::new()
        .route("/_health", get(health_check))
        .route(
            "/sections",
            post(sections_handler::crear).get(sections_handler::listar),
        )
        .route(
            "/sections/{id}",
            get(sections_handler::consultar)
                .put(sections_handler::editar)
                .delete(sections_handler::eliminar),
        )
        .route("/sections/{id}/baja", post(sections_handler::baja))
        .route(
            "/sections/{id}/reactivar",
            post(sections_handler::reactivar),
        )
        .route(
            "/sections/{id}/students",
            post(estudiantes::crear).get(estudiantes::listar),
        )
        .route("/students", get(estudiantes::listar_todos))
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
        .merge(attendance_handler::router())
        .merge(grades_handler::router())
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
