use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use uuid::Uuid;

use crate::shared::error::AppError;
use sqlx::PgPool;

use super::adapter_pg::PgGradeRepository;
use super::service::GradeService;

#[derive(serde::Serialize)]
pub struct NotaResponse {
    nota: String,
}

pub fn router() -> Router<PgPool> {
    Router::new().route("/lapses/{id_lapse}/grades/{id_student}", get(calcular))
}

async fn calcular(
    State(pool): State<PgPool>,
    Path((id_lapse, id_student)): Path<(Uuid, Uuid)>,
) -> Result<(StatusCode, Json<NotaResponse>), AppError> {
    let repo = PgGradeRepository::new(pool);
    let svc = GradeService::new(repo);
    match svc.calcular_nota(id_lapse, id_student).await {
        Ok(nota) => Ok((
            StatusCode::OK,
            Json(NotaResponse {
                nota: nota.to_string(),
            }),
        )),
        Err(e) => Err(AppError::Database(e)),
    }
}
