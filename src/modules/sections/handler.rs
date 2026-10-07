//! Handlers HTTP del módulo `sections`.
//!
//! Capa fina: validan la entrada, delegan en `queries` y deciden el código de
//! estado. No deciden reglas de negocio — eso vive en `domain.rs` y `queries.rs`.
//! Las operaciones que escriben corren una transacción que la consitución exige
//! atómica: si algo falla a mitad, el `?` devuelve antes del `commit` y el
//! `Transaction` se descarta (rollback implícito), así que ninguna queda a
//! medias (RNF-2).

use crate::modules::sections::domain::{preparar_entrada, ErrorDominio, Grado};
use crate::modules::sections::queries;
use crate::shared::error::AppError;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;

/// El cuerpo del alta y de la edición, en la forma que manda el cliente.
///
/// La normalización y las reglas las aplica `domain::preparar_entrada`, el
/// único punto de validación (RF-14): el handler no valida nada por su cuenta.
#[derive(Deserialize)]
pub struct EntradaSeccion {
    grade_level: String,
    section: String,
    shift: String,
}

/// Filtro opcional del listado: `GET /sections?grado=1ro`.
#[derive(Deserialize)]
pub struct FiltroGrado {
    grado: Option<String>,
}

/// Convierte un `ErrorDominio` (campo vacío, largo, grado o turno
/// desconocidos) en un `400` con su mensaje en español.
fn dominios_a_bad_request(error: ErrorDominio) -> AppError {
    AppError::BadRequest(error.to_string())
}

/// Una fila que no existe en la base es un `404`, no un `500`.
///
/// `From<sqlx::Error>` deja todo lo que no sea conflicto como `Database`, pero
/// aquí el caso "no hay fila" es una respuesta esperada de la API y no un
/// fallo del servidor: se traduce antes de que el `?` lo envuelva.
fn a_not_found(error: sqlx::Error) -> AppError {
    match error {
        sqlx::Error::RowNotFound => AppError::NotFound("No se encontró la sección".into()),
        otro => otro.into(),
    }
}

/// `POST /sections` — alta (RF-1).
pub async fn crear(
    State(pool): State<PgPool>,
    Json(entrada): Json<EntradaSeccion>,
) -> Result<(StatusCode, Json<queries::Seccion>), AppError> {
    let datos = preparar_entrada(&entrada.grade_level, &entrada.section, &entrada.shift)
        .map_err(dominios_a_bad_request)?;

    let mut tx = pool.begin().await?;
    let seccion = queries::crear(&mut tx, datos).await?;
    tx.commit().await?;

    Ok((StatusCode::CREATED, Json(seccion)))
}

/// `GET /sections?grado=...` — listado de activas (RF-9 a RF-11, RF-24).
///
/// Un grado en el filtro que no esté en el catálogo es un `400` con el mismo
/// mensaje que el alta (RF-7): la enumeración sale del enum, no de un literal.
pub async fn listar(
    State(pool): State<PgPool>,
    Query(filtro): Query<FiltroGrado>,
) -> Result<Json<Vec<queries::Seccion>>, AppError> {
    let grado = match filtro.grado {
        Some(texto) => Some(Grado::try_from(texto.as_str()).map_err(dominios_a_bad_request)?),
        None => None,
    };

    let secciones = queries::listar_activas(&pool, grado).await?;
    Ok(Json(secciones))
}

/// `GET /sections/{id}` — consulta por identificador, activa o dada de baja
/// (RF-12); `404` si la sección no existe (RF-13).
pub async fn consultar(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> Result<Json<queries::Seccion>, AppError> {
    let seccion = queries::get_by_id(&pool, id).await.map_err(a_not_found)?;
    Ok(Json(seccion))
}

/// `PUT /sections/{id}` — edición (RF-14 a RF-17): mismo cuerpo y misma
/// validación que el alta.
pub async fn editar(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
    Json(entrada): Json<EntradaSeccion>,
) -> Result<Json<queries::Seccion>, AppError> {
    let datos = preparar_entrada(&entrada.grade_level, &entrada.section, &entrada.shift)
        .map_err(dominios_a_bad_request)?;

    let mut tx = pool.begin().await?;
    let seccion = queries::editar(&mut tx, id, datos)
        .await
        .map_err(a_not_found)?;
    tx.commit().await?;

    Ok(Json(seccion))
}

/// `POST /sections/{id}/baja` — baja lógica (RF-18, RF-19).
pub async fn baja(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> Result<Json<queries::Seccion>, AppError> {
    cambiar_estado(pool, id, false).await
}

/// `POST /sections/{id}/reactivar` — reactivación (RF-20, RF-21).
pub async fn reactivar(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> Result<Json<queries::Seccion>, AppError> {
    cambiar_estado(pool, id, true).await
}

/// Implementación compartida de baja y reactivación: misma query (cambia el
/// booleano), misma forma de respuesta y misma distinción entre inexistente
/// (`404`) e idempotente (la fila, `200` sin cambios).
async fn cambiar_estado(
    pool: PgPool,
    id: Uuid,
    activo: bool,
) -> Result<Json<queries::Seccion>, AppError> {
    let mut tx = pool.begin().await?;
    let seccion = queries::set_active(&mut tx, id, activo)
        .await
        .map_err(a_not_found)?;
    tx.commit().await?;

    Ok(Json(seccion))
}

/// `DELETE /sections/{id}` — eliminación física, solo si la sección no tiene
/// estudiantes (RF-22, RF-23). Responde `204` sin cuerpo y deja la auditoría
/// del "eliminar" dentro de la transacción descartable.
pub async fn eliminar(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let mut tx = pool.begin().await?;
    queries::eliminar(&mut tx, id).await.map_err(a_not_found)?;
    tx.commit().await?;

    Ok(StatusCode::NO_CONTENT.into_response())
}
