//! Prueba de humo de `shared::audit::write_audit_log`.
//!
//! El helper se construyó en la Fase 1 y **nunca se ejecutó**, así que su
//! firma podía estar mal sin que nada lo dijera: bindea `Option<&str>` (texto)
//! contra `audit_logs.entity_id uuid NOT NULL`. RF-27 depende de que esto
//! funcione, y esto lo comprueba antes de construir nada encima.

mod common;

use civik_backend::shared::audit::{write_audit_log, write_audit_log_tx};
use serde_json::json;
use sqlx::Row;
use uuid::Uuid;

type Resultado = Result<(), Box<dyn std::error::Error>>;

#[tokio::test]
async fn escribe_y_se_lee_dentro_de_la_transaccion() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let id_seccion = Uuid::new_v4();

    write_audit_log(
        &mut *tx,
        "sections",
        id_seccion,
        "alta",
        Some(&json!({"grade_level": "1ro", "section": "a"})),
    )
    .await?;

    let (entity, entity_id, action): (String, Uuid, String) =
        sqlx::query_as("SELECT entity, entity_id, action FROM audit_logs WHERE entity_id = $1")
            .bind(id_seccion)
            .fetch_one(&mut *tx)
            .await?;

    assert_eq!(entity, "sections");
    assert_eq!(entity_id, id_seccion);
    assert_eq!(action, "alta");

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn el_payload_omiso_se_guarda_como_objeto_vacio() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let id_seccion = Uuid::new_v4();

    // `audit_logs.data` es `jsonb NOT NULL DEFAULT '{}'`: pasar `None` debe
    // producir `{}`, no un NULL que la columna rechaza.
    write_audit_log(&mut *tx, "sections", id_seccion, "baja", None).await?;

    let data: serde_json::Value = sqlx::query("SELECT data FROM audit_logs WHERE entity_id = $1")
        .bind(id_seccion)
        .fetch_one(&mut *tx)
        .await?
        .get("data");

    assert_eq!(data, json!({}), "esperaba {{}}, llegó {data}");

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn la_variante_tx_escribe_en_la_misma_transaccion() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let id_seccion = Uuid::new_v4();

    write_audit_log_tx(
        &mut tx,
        "sections",
        id_seccion,
        "editar",
        Some(&json!({"seccion_anterior": "b"})),
    )
    .await?;

    let total: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_logs WHERE entity_id = $1 AND action = 'editar'",
    )
    .bind(id_seccion)
    .fetch_one(&mut *tx)
    .await?;

    assert_eq!(
        total, 1,
        "la fila escrita con _tx debe estar en esta transacción"
    );

    tx.rollback().await?;
    Ok(())
}
