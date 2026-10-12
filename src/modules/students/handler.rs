//! Handlers HTTP del módulo `students`.
//!
//! Capa fina, como en `sections`: validan la entrada (con el mismo
//! `preparar_entrada` del alta y la edición), delegan en `queries` y deciden
//! el código de estado. Las reglas de negocio viven en `domain.rs` y
//! `queries.rs`; acá el registro de un estudiante se envuelve en una
//! transacción (RNF-2) para que la fila y su auditoría nazcan o no juntas.

use crate::modules::students::domain::{preparar_entrada, ErrorEntrada};
use crate::modules::students::queries;
use crate::shared::error::AppError;
use crate::shared::filtro::leer_incluir_inactivas;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;

/// El cuerpo del alta. **No** lleva `id_section`: la ruta (`POST
/// /sections/{id}/students`) ya la declara, y pedirla además en el body crea
/// dos versiones de la misma verdad (plan 003 §7).
#[derive(Deserialize)]
pub struct EntradaAlta {
    apellido: String,
    nombre: String,
    telefono: Option<String>,
}

/// El cuerpo de la edición. Acá **sí** entra `id_seccion`: la ruta
/// (`PUT /students/{id}`) no la contiene y el cambio de sección es parte de
/// la edición (RF-12).
#[derive(Deserialize)]
pub struct EntradaEdicion {
    id_seccion: Uuid,
    apellido: String,
    nombre: String,
    telefono: Option<String>,
}

/// Filtro del listado: `?incluir_inactivas=true`, tanto en el padrón por
/// sección como en el listado global.
#[derive(Deserialize)]
pub struct FiltroIncluirInactivas {
    incluir_inactivas: Option<String>,
}

/// Convierte un `ErrorEntrada` (campo vacío, largo excedido) en un `400` con
/// su mensaje en español.
fn entrada_a_bad_request(error: ErrorEntrada) -> AppError {
    AppError::BadRequest(error.to_string())
}

/// `POST /sections/{id}/students` — alta (RF-1).
pub async fn crear(
    State(pool): State<PgPool>,
    Path(id_seccion): Path<Uuid>,
    Json(entrada): Json<EntradaAlta>,
) -> Result<(StatusCode, Json<queries::Estudiante>), AppError> {
    let datos = preparar_entrada(
        id_seccion,
        &entrada.apellido,
        &entrada.nombre,
        entrada.telefono.as_deref(),
    )
    .map_err(entrada_a_bad_request)?;

    let mut tx = pool.begin().await?;
    let estudiante = queries::crear(&mut tx, datos).await?;
    tx.commit().await?;

    Ok((StatusCode::CREATED, Json(estudiante)))
}

/// `GET /sections/{id}/students?incluir_inactivas=true` — padrón de la
/// sección (RF-6, RF-7).
///
/// El parámetro se lee como `String` y lo convierte `leer_incluir_inactivas`
/// (plan 003 §7): el `Option<bool>` de serde podría responder 400 también,
/// pero el rechazo de extracción de axum no se compila contra `AppError` sin
/// conversiones extra.
pub async fn listar(
    State(pool): State<PgPool>,
    Path(id_seccion): Path<Uuid>,
    Query(filtro): Query<FiltroIncluirInactivas>,
) -> Result<Json<Vec<queries::Estudiante>>, AppError> {
    let incluir_inactivas = leer_incluir_inactivas(filtro.incluir_inactivas.as_deref())?;

    let mut tx = pool.begin().await?;
    let estudiantes = queries::listar_por_seccion(&mut tx, id_seccion, incluir_inactivas).await?;
    tx.commit().await?;

    Ok(Json(estudiantes))
}

/// `GET /students?incluir_inactivas=true` — listado global (RF-1, RF-2, RF-3).
///
/// La vista "todos juntos" del front: todas las secciones en una sola llamada,
/// con el mismo filtro y la misma forma que el padrón por sección.
pub async fn listar_todos(
    State(pool): State<PgPool>,
    Query(filtro): Query<FiltroIncluirInactivas>,
) -> Result<Json<Vec<queries::Estudiante>>, AppError> {
    let incluir_inactivas = leer_incluir_inactivas(filtro.incluir_inactivas.as_deref())?;

    let estudiantes = queries::listar_todos(&pool, incluir_inactivas).await?;
    Ok(Json(estudiantes))
}

/// `GET /students/{id}` — consulta por identificador (RF-8); `404` si no
/// existe (RF-9).
pub async fn consultar(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> Result<Json<queries::Estudiante>, AppError> {
    let estudiante = queries::consultar(&pool, id).await?;
    Ok(Json(estudiante))
}

/// `PUT /students/{id}` — edición (RF-10 a RF-13): mismo cuerpo y misma
/// validación que el alta, más la sección porque el cambio de sección es
/// parte de la edición.
pub async fn editar(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
    Json(entrada): Json<EntradaEdicion>,
) -> Result<Json<queries::Estudiante>, AppError> {
    let datos = preparar_entrada(
        entrada.id_seccion,
        &entrada.apellido,
        &entrada.nombre,
        entrada.telefono.as_deref(),
    )
    .map_err(entrada_a_bad_request)?;

    let mut tx = pool.begin().await?;
    let estudiante = queries::editar(&mut tx, id, datos).await?;
    tx.commit().await?;

    Ok(Json(estudiante))
}

/// `POST /students/{id}/baja` — baja lógica (RF-14, RF-15).
pub async fn baja(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> Result<Json<queries::Estudiante>, AppError> {
    cambiar_estado(pool, id, false).await
}

/// `POST /students/{id}/reactivar` — reactivación (RF-16, RF-17).
pub async fn reactivar(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> Result<Json<queries::Estudiante>, AppError> {
    cambiar_estado(pool, id, true).await
}

/// Implementación compartida de baja y reactivación: misma query (cambia el
/// booleano), misma forma de respuesta y misma distinción entre inexistente
/// (`404`, desde `ErrorEstudiante`) e idempotente (la fila, `200` sin
/// cambios).
async fn cambiar_estado(
    pool: PgPool,
    id: Uuid,
    activo: bool,
) -> Result<Json<queries::Estudiante>, AppError> {
    let mut tx = pool.begin().await?;
    let estudiante = queries::set_active(&mut tx, id, activo).await?;
    tx.commit().await?;

    Ok(Json(estudiante))
}
