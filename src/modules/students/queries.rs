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
/// nº 7), no los del dominio (`domain::DatosEstudiante`): `phone` es
/// `Option` porque la columna es nullable y el teléfono puede no estar
/// informado.
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Estudiante {
    pub id: Uuid,
    pub id_section: Uuid,
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
    /// Cualquier fallo de `sqlx`.
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
/// la misma transacción (RF-18, RNF-2).
///
/// Antes de insertar, verifica la sección dentro de la misma transacción:
/// inexistente → `SeccionNoExiste` (RF-3), inactiva → `SeccionInactiva`
/// (RF-4). No hay cédula que validar: `students` no tiene constraints UNIQUE
/// tras la migración `remove_students_national_id`.
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
        datos.telefono.as_deref(),
    )
    .fetch_one(&mut **tx)
    .await?;

    let payload = json!({
        "id_seccion": datos.id_seccion,
        "apellido": datos.apellido,
        "nombre": datos.nombre,
        "telefono": datos.telefono,
    });
    audit::write_audit_log_tx(tx, "students", fila.id, "alta", Some(&payload)).await?;

    Ok(fila)
}

/// Un estudiante por su identificador, activo o inactivo (RF-8); si no
/// existe, `EstudianteNoExiste` (RF-9).
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

/// El padrón de una sección, en el orden canónico del listado (RF-6, RF-7,
/// RF-9 y RNF-3): `lower(apellido), lower(nombre)` con un `id` como
/// desempate.
///
/// La sección debe existir — la sección inexistente responde `404` (RF-9 del
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

/// El listado global de estudiantes, sin filtrar por sección (RF-1, RF-2): el
/// punto de entrada de la vista "todos juntos" del front.
///
/// Una sola consulta, así que acepta un `PgExecutor` genérico (pool o
/// transacción): `incluir_inactivas = false` devuelve solo los activos;
/// `true`, también los inactivos con su estado. El orden canónico es el mismo
/// del padrón por sección (`lower(last_name), lower(first_name), id`), de modo
/// que ambas vistas coinciden (RNF-2).
pub async fn listar_todos(
    pool: impl PgExecutor<'_>,
    incluir_inactivas: bool,
) -> sqlx::Result<Vec<Estudiante>> {
    let filas = sqlx::query_file_as!(
        Estudiante,
        "src/modules/students/queries/list_all.sql",
        incluir_inactivas,
    )
    .fetch_all(pool)
    .await?;

    Ok(filas)
}

/// Edita un estudiante ya normalizado y deja su "editar" en el historial
/// (RF-10, RF-18). Se usa el mismo `preparar_entrada` del alta, así la
/// normalización y la validación de obligatorios son idénticas (RF-10, RF-11).
///
/// La sección se valida **solo cuando la edición la cambia** (RF-12, literal:
/// "CUANDO la edición cambia la sección"). Eso hace que corregir el teléfono
/// de un estudiante de una sección ya dada de baja no exija nada nuevo: el
/// estudiante ya está donde está. Si cambia, la nueva sección debe existir
/// (`SeccionNoExiste`) y estar activa (`SeccionInactiva`).
///
/// Un identificador inexistente responde `EstudianteNoExiste` (el 404 de la
/// edición). Leer primero el estado actual no es ceremonia: decide si la
/// validación de sección aplica.
pub async fn editar(
    tx: &mut PgTransaction<'_>,
    id: Uuid,
    datos: DatosEstudiante,
) -> Result<Estudiante, ErrorEstudiante> {
    let actual = consultar(&mut **tx, id).await?;

    if actual.id_section != datos.id_seccion {
        match estado_seccion(&mut **tx, datos.id_seccion).await? {
            EstadoSeccion::NoExiste => return Err(ErrorEstudiante::SeccionNoExiste),
            EstadoSeccion::Inactiva => return Err(ErrorEstudiante::SeccionInactiva),
            EstadoSeccion::Activa => {}
        }
    }

    let fila = sqlx::query_file_as!(
        Estudiante,
        "src/modules/students/queries/update.sql",
        id,
        datos.id_seccion,
        &datos.nombre,
        &datos.apellido,
        datos.telefono.as_deref(),
    )
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(ErrorEstudiante::EstudianteNoExiste)?;

    let payload = json!({
        "id_seccion": datos.id_seccion,
        "apellido": datos.apellido,
        "nombre": datos.nombre,
        "telefono": datos.telefono,
    });
    audit::write_audit_log_tx(tx, "students", fila.id, "editar", Some(&payload)).await?;

    Ok(fila)
}

/// Marca a un estudiante activo o inactivo según `activo` (RF-14, RF-16),
/// de forma idempotente (RF-15, RF-17).
///
/// `set_active.sql` solo actualiza si el estado cambia (`IS DISTINCT FROM`);
/// cuando no devuelve filas hay dos posibilidades que la base no separa:
/// si el estudiante no existe, responde `EstudianteNoExiste`; si existe, ya
/// estaba en el estado pedido y se devuelve tal cual (sin mover `updated_at`).
///
/// La auditoría (RF-18) se escribe únicamente cuando el estado cambió de
/// verdad: repetir la llamada "no modifica nada" y no debe dejar una entrada
/// falsa en el historial.
pub async fn set_active(
    tx: &mut PgTransaction<'_>,
    id: Uuid,
    activo: bool,
) -> Result<Estudiante, ErrorEstudiante> {
    let fila = sqlx::query_file_as!(
        Estudiante,
        "src/modules/students/queries/set_active.sql",
        id,
        activo,
    )
    .fetch_optional(&mut **tx)
    .await?;

    let Some(fila) = fila else {
        // Sin filas: inexistente o idempotente. `consultar` separa los dos.
        return consultar(&mut **tx, id).await;
    };

    let accion = if activo { "reactivar" } else { "baja" };
    let payload = json!({ "activo": activo });
    audit::write_audit_log_tx(tx, "students", fila.id, accion, Some(&payload)).await?;

    Ok(fila)
}
