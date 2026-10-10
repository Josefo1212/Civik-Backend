//! Handlers HTTP del módulo `lapses`.
//!
//! Capa muy fina: deserializa la entrada, delega en `service` (que decide el
//! flujo, valida el estado y es dueño de la transacción) y elige el código de
//! estado. A diferencia de `sections`/`students` (CRUD directo), acá el handler
//! **no** valida ni abre transacciones: las reglas viven en `domain.rs` y se
//! aplican en `service.rs` (hexagonal parcial, plan §4). Las lecturas
//! (`listar`, `consultar`) van directo a `queries`, como en los demás módulos.

use crate::modules::lapses::queries::{self, Lapso};
use crate::modules::lapses::service::{self, EntradaLapso};
use crate::shared::error::AppError;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use sqlx::PgPool;
use uuid::Uuid;

/// Una fila que no existe es un `404`, no un `500` (RF-7): se traduce antes de
/// que el `?` lo envuelva como `Database`. El `service` ya hizo lo mismo para
/// sus operaciones; `consultar` lo necesita acá porque lee directo de
/// `queries`.
fn a_not_found(error: sqlx::Error) -> AppError {
    match error {
        sqlx::Error::RowNotFound => AppError::NotFound("No se encontró el lapso".into()),
        otro => otro.into(),
    }
}

/// `POST /lapses` — alta (RF-1 a RF-4, RF-19). Responde `201` con el lapso
/// creado (estado `PLANNED`).
pub async fn crear(
    State(pool): State<PgPool>,
    Json(entrada): Json<EntradaLapso>,
) -> Result<(StatusCode, Json<Lapso>), AppError> {
    let lapso = service::crear(&pool, &entrada).await?;

    Ok((StatusCode::CREATED, Json(lapso)))
}

/// `GET /lapses` — listado completo en el orden canónico (RF-5).
pub async fn listar(State(pool): State<PgPool>) -> Result<Json<Vec<Lapso>>, AppError> {
    let lapsos = queries::listar(&pool).await?;

    Ok(Json(lapsos))
}

/// `GET /lapses/{id}` — consulta por identificador, en cualquier estado
/// (RF-6); `404` si no existe (RF-7).
pub async fn consultar(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> Result<Json<Lapso>, AppError> {
    let lapso = queries::get_by_id(&pool, id).await.map_err(a_not_found)?;

    Ok(Json(lapso))
}

/// `PATCH /lapses/{id}` — edición estructural (RF-8 a RF-11, RF-19). Mismo
/// cuerpo que el alta; la validación es la misma (`preparar_entrada`).
pub async fn editar(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
    Json(entrada): Json<EntradaLapso>,
) -> Result<Json<Lapso>, AppError> {
    let lapso = service::editar(&pool, id, &entrada).await?;

    Ok(Json(lapso))
}

/// `POST /lapses/{id}/activar` — pone el lapso en `ACTIVE` (RF-12 a RF-14,
/// RF-19).
pub async fn activar(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> Result<Json<Lapso>, AppError> {
    let lapso = service::activar(&pool, id).await?;

    Ok(Json(lapso))
}

/// `POST /lapses/{id}/cerrar` — cierra el lapso en `CLOSED` (RF-15, RF-16,
/// RF-19), terminal e inmutable desde entonces.
pub async fn cerrar(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> Result<Json<Lapso>, AppError> {
    let lapso = service::cerrar(&pool, id).await?;

    Ok(Json(lapso))
}

/// `DELETE /lapses/{id}` — borrado físico del lapso vacío, no `CLOSED` y no
/// terminado (RF-17, RF-18, RF-19). Responde `204` sin cuerpo y deja la
/// auditoría del "eliminar" dentro de la transacción descartable.
pub async fn eliminar(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    service::eliminar(&pool, id).await?;

    Ok(StatusCode::NO_CONTENT.into_response())
}
