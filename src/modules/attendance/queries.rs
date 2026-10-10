use chrono::{DateTime, Utc};
use sqlx::{PgExecutor, Result, Row};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct Attendance {
    pub id_event: Uuid,
    pub id_student: Uuid,
    pub state: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

pub async fn list_by_event<'e, E>(executor: E, id_event: Uuid) -> Result<Vec<Attendance>>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query(
        "SELECT id_event, id_student, state::text AS state, created_at, updated_at
         FROM attendance WHERE id_event = $1 ORDER BY id_student",
    )
    .bind(id_event)
    .fetch_all(executor)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| Attendance {
            id_event: r.get("id_event"),
            id_student: r.get("id_student"),
            state: r.get("state"),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
        })
        .collect())
}

pub async fn set_state<'e, E>(
    executor: E,
    id_event: Uuid,
    id_student: Uuid,
    state: &str,
) -> Result<Attendance>
where
    E: PgExecutor<'e>,
{
    let r = sqlx::query(
        "UPDATE attendance SET state = $3::attendance_state, updated_at = NOW()
         WHERE id_event = $1 AND id_student = $2
         RETURNING id_event, id_student, state::text AS state, created_at, updated_at",
    )
    .bind(id_event)
    .bind(id_student)
    .bind(state)
    .fetch_one(executor)
    .await?;
    Ok(Attendance {
        id_event: r.get("id_event"),
        id_student: r.get("id_student"),
        state: r.get("state"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    })
}

pub async fn insert<'e, E>(
    executor: E,
    id_event: Uuid,
    id_student: Uuid,
    state: &str,
) -> Result<Attendance>
where
    E: PgExecutor<'e>,
{
    let r = sqlx::query(
        "INSERT INTO attendance (id_event, id_student, state)
         VALUES ($1, $2, $3::attendance_state)
         RETURNING id_event, id_student, state::text AS state, created_at, updated_at",
    )
    .bind(id_event)
    .bind(id_student)
    .bind(state)
    .fetch_one(executor)
    .await?;
    Ok(Attendance {
        id_event: r.get("id_event"),
        id_student: r.get("id_student"),
        state: r.get("state"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    })
}
