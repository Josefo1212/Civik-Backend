use serde_json::Value;
use sqlx::{PgExecutor, PgTransaction};

/// Escribe una entrada en audit_logs dentro de una conexión o transacción.
pub async fn write_audit_log<'a, E>(conn: E, entity: &str, entity_id: Option<&str>, action: &str, data: Option<&Value>) -> sqlx::Result<()>
where
    E: PgExecutor<'a>,
{
    sqlx::query(
        r#"
        INSERT INTO audit_logs (entity, entity_id, action, data)
        VALUES ($1, $2, $3, $4)
        "#,
    )
    .bind(entity)
    .bind(entity_id)
    .bind(action)
    .bind(data)
    .execute(conn)
    .await?;

    Ok(())
}

/// Escribe una entrada en audit_logs dentro de una transacción (alias de write_audit_log).
pub async fn write_audit_log_tx(
    tx: &mut PgTransaction<'_>,
    entity: &str,
    entity_id: Option<&str>,
    action: &str,
    data: Option<&Value>,
) -> sqlx::Result<()> {
    write_audit_log(tx.as_mut(), entity, entity_id, action, data).await
}
