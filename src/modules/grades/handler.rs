//! Capa HTTP de `grades`: traduce los `Path` al caso de uso y arma el cuerpo.
//! El endpoint es por lapso y sección (RF-9) y devuelve la nota de cada
//! estudiante del roster, o `null` si no tiene.

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use uuid::Uuid;

use crate::shared::error::AppError;

use super::adapter_pg::PgGradeRepository;
use super::service::{self, GradeService};

/// Una fila del listado: la identidad del estudiante y su nota como string (o
/// `null`). El string conserva un decimal en el cálculo y los dos del override
/// (RF-8).
#[derive(serde::Serialize)]
pub struct NotaDto {
    id_student: Uuid,
    first_name: String,
    last_name: String,
    nota: Option<String>,
}

pub fn router() -> Router<sqlx::PgPool> {
    Router::new().route(
        "/lapses/{id_lapse}/sections/{id_section}/grades",
        get(listar_notas),
    )
}

/// RF-9: la nota de cada estudiante de la sección en el lapso, en orden
/// canónico. Lapso inexistente `404` (RF-10); sección inexistente `404`
/// (RF-11).
async fn listar_notas(
    State(pool): State<sqlx::PgPool>,
    Path((id_lapse, id_section)): Path<(Uuid, Uuid)>,
) -> Result<Json<Vec<NotaDto>>, AppError> {
    let repo = PgGradeRepository::new(pool);
    let servicio = GradeService::new(repo);
    let filas = servicio.notas_de_seccion(id_lapse, id_section).await?;

    let respuesta = filas
        .into_iter()
        .map(|fila| NotaDto {
            id_student: fila.id_student,
            first_name: fila.first_name,
            last_name: fila.last_name,
            nota: service::formatear_nota(&fila.nota),
        })
        .collect();

    Ok(Json(respuesta))
}
