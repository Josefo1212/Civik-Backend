//! Acceso a datos de `attendance`: una función por `.sql`, verificado en
//! compilación (`query_file_as!`, constitución nº 1).
//!
//! `state` se lee y escribe como **texto** (`state::text` al leer,
//! `$n::text::attendance_state` al escribir): los macros de `sqlx` no infieren
//! el enum propio de Postgres (aprendizaje de `sections.shift`, `MEMORY.md`).
//!
//! Las funciones que combinan los dos ids reciben los newtypes del dominio
//! (`EventId`, `StudentId`), no `Uuid` sueltos (RNF-2): pasar un `id_student`
//! donde va un `id_event` no compila.

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgExecutor;
use uuid::Uuid;

use super::domain::{EventId, StudentId};

/// Una fila de `attendance`, en la forma exacta que devuelven las queries.
///
/// `state` es `Option<String>` porque la columna se lee con `state::text` y
/// sqlx no puede probar que el cast sea no nulo al verificar la consulta
/// (mismo caso que `events.status`): la columna en sí es `NOT NULL`.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Attendance {
    pub id_event: Uuid,
    pub id_student: Uuid,
    pub state: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Una fila del padrón de un evento: un estudiante aplicable con su estado
/// (o `None` si no está marcado). No es una fila de `attendance`: por eso el
/// `state` es opcional.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct FilaPadron {
    pub id_student: Uuid,
    pub first_name: String,
    pub last_name: String,
    pub state: Option<String>,
}

/// Crea la fila del par (evento, estudiante) o actualiza su estado si ya
/// existe (RF-1, RF-7).
pub async fn upsert<'e, E>(
    executor: E,
    id_event: EventId,
    id_student: StudentId,
    state: &str,
) -> sqlx::Result<Attendance>
where
    E: PgExecutor<'e>,
{
    sqlx::query_file_as!(
        Attendance,
        "src/modules/attendance/queries/upsert.sql",
        id_event.0,
        id_student.0,
        state,
    )
    .fetch_one(executor)
    .await
}

/// Corrige el estado de una fila existente (RF-8). El service lee la fila
/// antes con `get` (para el `404` y la idempotencia de RF-10), así que acá
/// se hace un `UPDATE` directo que devuelve la fila resultante.
pub async fn set_state<'e, E>(
    executor: E,
    id_event: EventId,
    id_student: StudentId,
    state: &str,
) -> sqlx::Result<Attendance>
where
    E: PgExecutor<'e>,
{
    sqlx::query_file_as!(
        Attendance,
        "src/modules/attendance/queries/set_state.sql",
        id_event.0,
        id_student.0,
        state,
    )
    .fetch_one(executor)
    .await
}

/// La fila de un par (evento, estudiante), o `None` si no está registrada.
pub async fn get<'e, E>(
    executor: E,
    id_event: EventId,
    id_student: StudentId,
) -> sqlx::Result<Option<Attendance>>
where
    E: PgExecutor<'e>,
{
    sqlx::query_file_as!(
        Attendance,
        "src/modules/attendance/queries/get.sql",
        id_event.0,
        id_student.0,
    )
    .fetch_optional(executor)
    .await
}

/// Todas las filas de asistencia de un evento (para el padrón y para saber
/// quiénes están ya registrados).
pub async fn rows_by_event<'e, E>(executor: E, id_event: EventId) -> sqlx::Result<Vec<Attendance>>
where
    E: PgExecutor<'e>,
{
    sqlx::query_file_as!(
        Attendance,
        "src/modules/attendance/queries/rows_by_event.sql",
        id_event.0,
    )
    .fetch_all(executor)
    .await
}

/// El padrón de un evento (RF-13): los estudiantes activos más los que ya
/// tengan fila (aunque estén de baja), con su estado o `None`.
pub async fn padron<'e, E>(executor: E, id_event: EventId) -> sqlx::Result<Vec<FilaPadron>>
where
    E: PgExecutor<'e>,
{
    sqlx::query_file_as!(
        FilaPadron,
        "src/modules/attendance/queries/padron.sql",
        id_event.0,
    )
    .fetch_all(executor)
    .await
}
