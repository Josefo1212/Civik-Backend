//! Queries del módulo `events` (SQL verificado en compilación).

use chrono::NaiveDate;
use serde::Serialize;
use sqlx::PgExecutor;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Event {
    pub id: Uuid,
    pub id_lapse: Uuid,
    pub event_date: NaiveDate,
    pub type_event: Option<String>,
    pub source: Option<String>,
    pub event_name: String,
    pub status: Option<String>,
    pub cancelled_reason: Option<String>,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct LapseLite {
    pub id: Uuid,
    pub school_year: i32,
    pub lapse_number: i16,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub practice_day: Option<String>,
    pub status: Option<String>,
}

pub async fn get_by_id<'e, E>(executor: E, id: Uuid) -> sqlx::Result<Event>
where
    E: PgExecutor<'e>,
{
    sqlx::query_file_as!(Event, "src/modules/events/queries/get_by_id.sql", id)
        .fetch_one(executor)
        .await
}

pub async fn listar_all<'e, E>(executor: E) -> sqlx::Result<Vec<Event>>
where
    E: PgExecutor<'e>,
{
    sqlx::query_file_as!(Event, "src/modules/events/queries/list_all.sql")
        .fetch_all(executor)
        .await
}

pub async fn listar_por_lapso<'e, E>(executor: E, id_lapse: Uuid) -> sqlx::Result<Vec<Event>>
where
    E: PgExecutor<'e>,
{
    sqlx::query_file_as!(Event, "src/modules/events/queries/list_by_lapse.sql", id_lapse)
        .fetch_all(executor)
        .await
}

pub async fn crear_manual<'e, E>(
    executor: E,
    id_lapse: Uuid,
    event_date: NaiveDate,
    type_event: &str,
    event_name: &str,
) -> sqlx::Result<Event>
where
    E: PgExecutor<'e>,
{
    sqlx::query_file_as!(
        Event,
        "src/modules/events/queries/create_manual.sql",
        id_lapse,
        event_date,
        type_event,
        event_name
    )
    .fetch_one(executor)
    .await
}

pub async fn cancelar<'e, E>(executor: E, id: Uuid, motivo: &str) -> sqlx::Result<Event>
where
    E: PgExecutor<'e>,
{
    let fila = sqlx::query_file_as!(
        Event,
        "src/modules/events/queries/cancel.sql",
        id,
        motivo
    )
    .fetch_optional(executor)
    .await?;

    let Some(evento) = fila else {
        return Err(sqlx::Error::RowNotFound);
    };
    Ok(evento)
}

pub async fn editar_manual<'e, E>(
    executor: E,
    id: Uuid,
    event_date: NaiveDate,
    type_event: &str,
    event_name: &str,
) -> sqlx::Result<Event>
where
    E: PgExecutor<'e>,
{
    let fila = sqlx::query_file_as!(
        Event,
        "src/modules/events/queries/update_manual.sql",
        id,
        event_date,
        type_event,
        event_name
    )
    .fetch_optional(executor)
    .await?;

    let Some(evento) = fila else {
        return Err(sqlx::Error::RowNotFound);
    };
    Ok(evento)
}

pub async fn tiene_asistencia<'e, E>(executor: E, id: Uuid) -> sqlx::Result<bool>
where
    E: PgExecutor<'e>,
{
    let fila = sqlx::query_file!("src/modules/events/queries/has_attendance.sql", id)
        .fetch_one(executor)
        .await?;
    Ok(fila.cantidad.unwrap_or(0) > 0)
}

pub async fn obtener_lapso<'e, E>(executor: E, id_lapse: Uuid) -> sqlx::Result<LapseLite>
where
    E: PgExecutor<'e>,
{
    sqlx::query_file_as!(LapseLite, "src/modules/events/queries/get_lapse.sql", id_lapse)
        .fetch_one(executor)
        .await
}
