use serde_json::{Map, Value};
use sqlx::{PgExecutor, PgTransaction};
use uuid::Uuid;

/// Escribe una entrada en audit_logs dentro de una conexión o transacción.
///
/// `entity_id` es `Uuid` y no `Option<Uuid>`: la columna es `uuid NOT NULL`,
/// así que un `Option` no habilita ningún caso de uso real — solo permitiría
/// pasar `None` y descubrir el error en la base en lugar de en la
/// compilación.
///
/// `data` sí es opcional en la firma: puede quererse registrar una acción sin
/// payload. El vacío se guarda como `{}`, que es lo que la columna ya trae
/// como `DEFAULT`, y no como NULL.
pub async fn write_audit_log<'a, E>(
    conn: E,
    entity: &str,
    entity_id: Uuid,
    action: &str,
    data: Option<&Value>,
) -> sqlx::Result<()>
where
    E: PgExecutor<'a>,
{
    let data = data.cloned().unwrap_or_else(|| Value::Object(Map::new()));

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
    entity_id: Uuid,
    action: &str,
    data: Option<&Value>,
) -> sqlx::Result<()> {
    write_audit_log(tx.as_mut(), entity, entity_id, action, data).await
}
