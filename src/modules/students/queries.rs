//! Acceso a datos de `students`: una función por `.sql`, con la auditoría en
//! la misma transacción (RNF-2).
//!
//! El resultado de las operaciones no es `sqlx::Result` sino
//! `Result<_, ErrorEstudiante>` porque hay casos que la base no separa por sí
//! sola: la sección puede no existir (`404`), estar inactiva —una regla de la
//! API, no una invariante de la base— (`409`), y hasta la sección referencia
//! validarse en la misma transacción que el insert.

use crate::modules::students::domain::DatosEstudiante;
use crate::shared::{audit, error::AppError};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::json;
use sqlx::{FromRow, PgExecutor, PgTransaction};
use uuid::Uuid;

/// Una fila de `students`, en la forma exacta que devuelven las queries.
///
/// Los nombres de los campos son los de la base y del JSON (constitución
/// nº 7), no los del dominio (`domain::DatosEstudiante`): `national_id` y
/// `phone` son `Option` porque la columna es nullable y los nombres pueden no
/// estar informados.
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Estudiante {
    pub id: Uuid,
    pub id_section: Uuid,
    pub national_id: Option<String>,
    pub first_name: String,
    pub last_name: String,
    pub phone: Option<String>,
    pub active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Los estados en los que puede estar la sección que referencia un estudiante.
enum EstadoSeccion {
    Activa,
    Inactiva,
    NoExiste,
}

/// Los casos que el flujo de un estudiante distingue y la base no.
///
/// `SeccionInactiva` no tiene equivalente en un código de Postgres (`active`
/// es un booleano, no una constraint), así que no pasa por
/// `mapear_violacion`: se produce acá, dentro de la transacción (plan §8.2).
#[derive(Debug, thiserror::Error)]
pub enum ErrorEstudiante {
    /// El estudiante que se quiso editar o dar de baja no existe (`404`).
    #[error("No se encontró el estudiante")]
    EstudianteNoExiste,
    /// La sección a la que se intenta vincular no existe (`404`).
    #[error("No se encontró la sección indicada")]
    SeccionNoExiste,
    /// La sección a la que se intenta vincular está dada de baja (`409`).
    #[error("La sección está inactiva: no admite estudiantes")]
    SeccionInactiva,
    /// Cualquier fallo de `sqlx` — incluido el `23505` de la cédula, que se
    /// traduce al 409 en `From<ErrorEstudiante> for AppError`.
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
}

impl From<ErrorEstudiante> for AppError {
    fn from(error: ErrorEstudiante) -> Self {
        match error {
            ErrorEstudiante::EstudianteNoExiste => {
                AppError::NotFound("No se encontró el estudiante".into())
            }
            ErrorEstudiante::SeccionNoExiste => {
                AppError::NotFound("No se encontró la sección indicada".into())
            }
            ErrorEstudiante::SeccionInactiva => {
                AppError::Conflict("La sección está inactiva: no admite estudiantes".into())
            }
            // El `23505` de `uq_students_national_id` viaja hasta acá y
            // `From<sqlx::Error>` lo convierte en `Conflict` con el mensaje
            // correcto (error.rs); el resto queda como 500.
            ErrorEstudiante::Sqlx(inner) => inner.into(),
        }
    }
}

/// El estado de la sección `id`, para decidir 404/409 antes de escribir.
async fn estado_seccion<'e, E>(conn: E, id: Uuid) -> sqlx::Result<EstadoSeccion>
where
    E: PgExecutor<'e>,
{
    let activa: Option<bool> = sqlx::query_scalar("SELECT active FROM sections WHERE id = $1")
        .bind(id)
        .fetch_optional(conn)
        .await?;

    Ok(match activa {
        None => EstadoSeccion::NoExiste,
        Some(true) => EstadoSeccion::Activa,
        Some(false) => EstadoSeccion::Inactiva,
    })
}

/// Da de alta un estudiante ya normalizado y deja su entrada de auditoría en
/// la misma transacción (RF-21, RNF-2).
///
/// Antes de insertar, verifica la sección dentro de la misma transacción:
/// inexistente → `SeccionNoExiste` (RF-3), inactiva → `SeccionInactiva`
/// (RF-4). El `23505` de la cédula sube envuelto en `ErrorEstudiante::Sqlx` y
/// `From<ErrorEstudiante> for AppError` lo traduce a `409` (RF-5); la
/// restricción manda, no hay `SELECT` previo de "¿ya existe la cédula?"
/// (plan §8.2).
pub async fn crear(
    tx: &mut PgTransaction<'_>,
    datos: DatosEstudiante,
) -> Result<Estudiante, ErrorEstudiante> {
    match estado_seccion(&mut **tx, datos.id_seccion).await? {
        EstadoSeccion::NoExiste => return Err(ErrorEstudiante::SeccionNoExiste),
        EstadoSeccion::Inactiva => return Err(ErrorEstudiante::SeccionInactiva),
        EstadoSeccion::Activa => {}
    }

    let fila = sqlx::query_file_as!(
        Estudiante,
        "src/modules/students/queries/create.sql",
        datos.id_seccion,
        &datos.nombre,
        &datos.apellido,
        datos.cedula.as_deref(),
        datos.telefono.as_deref(),
    )
    .fetch_one(&mut **tx)
    .await?;

    let payload = json!({
        "id_seccion": datos.id_seccion,
        "apellido": datos.apellido,
        "nombre": datos.nombre,
        "cedula": datos.cedula,
        "telefono": datos.telefono,
    });
    audit::write_audit_log_tx(tx, "students", fila.id, "alta", Some(&payload)).await?;

    Ok(fila)
}

/// Un estudiante por su identificador, activo o inactivo (RF-10); si no
/// existe, `EstudianteNoExiste` (RF-11).
///
/// Acepta un `PgExecutor` genérico porque hace una sola consulta y no lo
/// reusa para una segunda llamada (mismo criterio que `sections::get_by_id`).
pub async fn consultar<'e, E>(conn: E, id: Uuid) -> Result<Estudiante, ErrorEstudiante>
where
    E: PgExecutor<'e>,
{
    let fila = sqlx::query_file_as!(Estudiante, "src/modules/students/queries/get_by_id.sql", id,)
        .fetch_optional(conn)
        .await?
        .ok_or(ErrorEstudiante::EstudianteNoExiste)?;

    Ok(fila)
}

/// El padrón de una sección, en el orden canónico del listado (RF-8, RF-9,
/// RF-11 y RNF-3): `lower(apellido), lower(nombre)` con un `id` como
/// desempate.
///
/// La sección debe existir — la sección inexistente responde `404` (RF-11 del
/// listado)— pero puede estar inactiva: su padrón sigue siendo consultable
/// (caso límite de la spec). `incluir_inactivas = false` devuelve solo los
/// activos; `true`, también los inactivos con su estado.
///
/// Toma `&mut PgTransaction` y no un `PgExecutor` genérico por la misma razón
/// que `crear`: hace dos llamadas (la verificación de la sección y el listado)
/// y `sqlx` no permite reutilizar un executor genérico.
pub async fn listar_por_seccion(
    tx: &mut PgTransaction<'_>,
    id_seccion: Uuid,
    incluir_inactivas: bool,
) -> Result<Vec<Estudiante>, ErrorEstudiante> {
    match estado_seccion(&mut **tx, id_seccion).await? {
        EstadoSeccion::NoExiste => return Err(ErrorEstudiante::SeccionNoExiste),
        EstadoSeccion::Activa | EstadoSeccion::Inactiva => {}
    }

    let filas = sqlx::query_file_as!(
        Estudiante,
        "src/modules/students/queries/list_by_section.sql",
        id_seccion,
        incluir_inactivas,
    )
    .fetch_all(&mut **tx)
    .await?;

    Ok(filas)
}
