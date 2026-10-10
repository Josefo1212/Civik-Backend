use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{patch, post};
use axum::{Json, Router};
use serde::Deserialize;
use uuid::Uuid;

use crate::shared::error::AppError;

use super::service::{self, RegistroLote};

#[derive(Deserialize)]
pub struct PatchBody {
    state: String,
}

pub fn router() -> Router<sqlx::PgPool> {
    Router::new()
        .route("/events/{id_event}/attendance", post(registrar))
        .route("/attendance/{id_event}/{id_student}", patch(corregir))
}

async fn registrar(
    State(pool): State<sqlx::PgPool>,
    Path(id_event): Path<Uuid>,
    Json(lote): Json<RegistroLote>,
) -> Result<StatusCode, AppError> {
    service::registrar_listado(&pool, id_event, lote).await?;
    Ok(StatusCode::CREATED)
}

async fn corregir(
    State(pool): State<sqlx::PgPool>,
    Path((id_event, id_student)): Path<(Uuid, Uuid)>,
    Json(body): Json<PatchBody>,
) -> Result<StatusCode, AppError> {
    service::corregir_asistencia(&pool, id_event, id_student, &body.state).await?;
    Ok(StatusCode::OK)
}
