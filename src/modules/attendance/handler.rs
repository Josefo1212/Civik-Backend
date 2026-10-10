//! Capa HTTP de `attendance`: traduce `Path`/`Json` a los casos de uso y
//! devuelve el cuerpo con su código. Las rutas anidan la asistencia bajo el
//! evento (`/events/{id}/attendance`), que es el recurso al que pertenece.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{patch, post};
use axum::{Json, Router};
use serde::Deserialize;
use uuid::Uuid;

use crate::shared::error::AppError;

use super::domain::{EventId, StudentId};
use super::queries::{Attendance, FilaPadron};
use super::service::{self, RegistroLote};

#[derive(Deserialize)]
pub struct PatchBody {
    pub state: String,
}

pub fn router() -> Router<sqlx::PgPool> {
    Router::new()
        .route("/events/{id_event}/attendance", post(registrar).get(listar))
        .route(
            "/events/{id_event}/attendance/{id_student}",
            patch(corregir),
        )
}

/// RF-1: registra el lote y devuelve `200` con el padrón actualizado.
async fn registrar(
    State(pool): State<sqlx::PgPool>,
    Path(id_event): Path<Uuid>,
    Json(lote): Json<RegistroLote>,
) -> Result<(StatusCode, Json<Vec<FilaPadron>>), AppError> {
    let padron = service::registrar_listado(&pool, EventId(id_event), lote).await?;
    Ok((StatusCode::OK, Json(padron)))
}

/// RF-13: el padrón del evento.
async fn listar(
    State(pool): State<sqlx::PgPool>,
    Path(id_event): Path<Uuid>,
) -> Result<(StatusCode, Json<Vec<FilaPadron>>), AppError> {
    let padron = service::listar_padron(&pool, EventId(id_event)).await?;
    Ok((StatusCode::OK, Json(padron)))
}

/// RF-8: corrige el estado del par (evento, estudiante).
async fn corregir(
    State(pool): State<sqlx::PgPool>,
    Path((id_event, id_student)): Path<(Uuid, Uuid)>,
    Json(body): Json<PatchBody>,
) -> Result<(StatusCode, Json<Attendance>), AppError> {
    let fila =
        service::corregir_asistencia(&pool, EventId(id_event), StudentId(id_student), &body.state)
            .await?;
    Ok((StatusCode::OK, Json(fila)))
}
