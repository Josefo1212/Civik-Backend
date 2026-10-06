//! T4 — el alta de secciones y sus duplicados.
//!
//! Qué se prueba acá y cómo se relaciona con la spec:
//! - RF-1, RF-2, RF-3, RF-4: `crear` persiste lo que `preparar_entrada`
//!   normalizó (`" 1RO "`, `" a "`, `"matutino"` → `1ro`, `a`, `MATUTINO`).
//! - RF-8: el alta duplicada falla, esté la original activa o dada de baja; la
//!   combinación no se libera nunca.
//! - RF-27: el alta deja su entrada de auditoría en la misma transacción.
//!
//! Los casos de validación pura (RF-5/6/7) ya viven en los tests de
//! `domain.rs`; acá se ejercita el camino completo hasta la base de datos.

mod common;

use civik_backend::modules::sections::domain::preparar_entrada;
use civik_backend::modules::sections::queries::{crear, Seccion};
use civik_backend::shared::error::AppError;
use uuid::Uuid;

type Resultado = Result<(), Box<dyn std::error::Error>>;

#[tokio::test]
async fn el_alta_persiste_la_fila_normalizada() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let datos = preparar_entrada(" 1RO ", "  a ", "matutino")?;
    let seccion: Seccion = crear(&mut tx, datos).await?;

    assert_ne!(seccion.id, Uuid::nil(), "RF-1: devuelve identificador");
    assert_eq!(seccion.grade_level, "1ro", "RF-2: minúscula");
    assert_eq!(seccion.section, "a", "RF-4: recortado");
    assert_eq!(seccion.shift, "MATUTINO", "RF-3: mayúscula");

    // Contraste contra la fila real, no solo contra lo que devuelve `crear`.
    let (grado, subdivision, turno): (String, String, String) = sqlx::query_as(
        "SELECT grade_level, section, shift::text FROM sections WHERE id = $1",
    )
    .bind(seccion.id)
    .fetch_one(&mut *tx)
    .await?;

    assert_eq!(grado, "1ro");
    assert_eq!(subdivision, "a");
    assert_eq!(turno, "MATUTINO");

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn el_alta_duplicada_de_una_seccion_activa_falla() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let datos = preparar_entrada("1ro", "a", "MATUTINO")?;
    crear(&mut tx, datos.clone()).await?;

    let resultado = crear(&mut tx, datos).await;

    let Err(error) = resultado else {
        panic!("la segunda alta de la misma combinación debe fallar (RF-8)");
    };

    if let sqlx::Error::Database(base) = &error {
        assert_eq!(base.code().as_deref(), Some("23505"));
        assert_eq!(base.constraint(), Some("uq_sections_grade_section_shift"));
    } else {
        panic!("esperaba un error de base de datos, llegó: {error}");
    }

    let app_error: AppError = error.into();
    assert!(matches!(app_error, AppError::Conflict(_)));
    assert!(
        app_error.to_string().contains("uso"),
        "el mensaje debe explicar que la combinación está en uso: {app_error}"
    );

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn el_alta_duplicada_de_una_seccion_dada_de_baja_tambien_falla() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let datos = preparar_entrada("1ro", "a", "MATUTINO")?;
    let seccion = crear(&mut tx, datos.clone()).await?;

    sqlx::query("UPDATE sections SET active = false WHERE id = $1")
        .bind(seccion.id)
        .execute(&mut *tx)
        .await?;

    let resultado = crear(&mut tx, datos).await;

    let Err(error) = resultado else {
        panic!("dar de baja la original no libera la combinación (RF-8, caso límite)");
    };

    if let sqlx::Error::Database(base) = &error {
        assert_eq!(base.code().as_deref(), Some("23505"));
        assert_eq!(base.constraint(), Some("uq_sections_grade_section_shift"));
    } else {
        panic!("esperaba un error de base de datos, llegó: {error}");
    }

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn el_alta_deja_su_entrada_en_el_historial() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let datos = preparar_entrada("1ro", "a", "MATUTINO")?;
    let seccion = crear(&mut tx, datos).await?;

    let (entity, action, data_raw): (String, String, String) =
        sqlx::query_as("SELECT entity, action, data::text FROM audit_logs WHERE entity_id = $1")
            .bind(seccion.id)
            .fetch_one(&mut *tx)
            .await?;

    assert_eq!(entity, "sections");
    assert_eq!(action, "alta", "RF-27: el alta registra su acción");

    let data: serde_json::Value = serde_json::from_str(&data_raw)?;
    assert_eq!(data["grado"], "1ro", "la auditoría guarda los valores");
    assert_eq!(data["seccion"], "a");
    assert_eq!(data["turno"], "MATUTINO");

    tx.rollback().await?;
    Ok(())
}