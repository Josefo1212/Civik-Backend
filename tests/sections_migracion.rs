//! Invariantes de esquema de `sections` que fija la migración
//! `sections_active_and_not_blank`.
//!
//! Son las dos defensas que la base pone por debajo del backend: la columna
//! `active` que sostiene la baja lógica (RF-18, RF-20, RF-24) y el `CHECK` de
//! no vacío que sobrevive a un bug del backend (RNF-1). Estas pruebas no
//! ejercitan el módulo `sections`, que todavía no existe: escriben directo con
//! SQL, que es justo lo que un backend defectuoso haría.

mod common;

use sqlx::Row;

type Resultado = Result<(), Box<dyn std::error::Error>>;

#[tokio::test]
async fn active_por_defecto_es_true() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let fila = sqlx::query(
        "INSERT INTO sections (grade_level, section, shift)
         VALUES ('1ro', 'a', 'MATUTINO')
         RETURNING active",
    )
    .fetch_one(&mut *tx)
    .await?;

    // Una sección recién creada se está ofreciendo: `active` nace en true.
    assert!(fila.get::<bool, _>("active"));

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn active_se_puede_desactivar_y_reactivar() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let id: uuid::Uuid = sqlx::query(
        "INSERT INTO sections (grade_level, section, shift)
         VALUES ('2do', 'b', 'VESPERTINO')
         RETURNING id",
    )
    .fetch_one(&mut *tx)
    .await?
    .get("id");

    let baja = sqlx::query("UPDATE sections SET active = false WHERE id = $1 RETURNING active")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    assert!(
        !baja.get::<bool, _>("active"),
        "la baja debe dejar active en false"
    );

    let alta = sqlx::query("UPDATE sections SET active = true WHERE id = $1 RETURNING active")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    assert!(
        alta.get::<bool, _>("active"),
        "la reactivación debe volver a true"
    );

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn grade_level_en_blanco_es_rechazado() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let resultado = sqlx::query(
        "INSERT INTO sections (grade_level, section, shift)
         VALUES ('', 'a', 'MATUTINO')",
    )
    .execute(&mut *tx)
    .await;

    assert!(
        resultado.is_err(),
        "sections_not_blank debe rechazar un grade_level vacío"
    );

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn grade_level_con_solo_espacios_es_rechazado() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let resultado = sqlx::query(
        "INSERT INTO sections (grade_level, section, shift)
         VALUES ('   ', 'a', 'MATUTINO')",
    )
    .execute(&mut *tx)
    .await;

    assert!(
        resultado.is_err(),
        "sections_not_blank debe usar btrim: los espacios no son contenido"
    );

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn section_en_blanco_es_rechazado() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let resultado = sqlx::query(
        "INSERT INTO sections (grade_level, section, shift)
         VALUES ('1ro', '  ', 'MATUTINO')",
    )
    .execute(&mut *tx)
    .await;

    assert!(
        resultado.is_err(),
        "sections_not_blank debe rechazar una subdivisión en blanco"
    );

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn la_combinacion_duplicada_sigue_rechazada() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    sqlx::query(
        "INSERT INTO sections (grade_level, section, shift)
         VALUES ('3ro', 'c', 'MATUTINO')",
    )
    .execute(&mut *tx)
    .await?;

    let duplicado = sqlx::query(
        "INSERT INTO sections (grade_level, section, shift)
         VALUES ('3ro', 'c', 'MATUTINO')",
    )
    .execute(&mut *tx)
    .await;

    assert!(
        duplicado.is_err(),
        "uq_sections_grade_section_shift debe seguir rechazando la combinación repetida"
    );

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn la_combinacion_no_se_libera_tras_la_baja() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let id: uuid::Uuid = sqlx::query(
        "INSERT INTO sections (grade_level, section, shift)
         VALUES ('4to', 'a', 'MATUTINO')
         RETURNING id",
    )
    .fetch_one(&mut *tx)
    .await?
    .get("id");

    sqlx::query("UPDATE sections SET active = false WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    // RF-8 y caso límite: dar de baja no libera la combinación.
    let reuso = sqlx::query(
        "INSERT INTO sections (grade_level, section, shift)
         VALUES ('4to', 'a', 'MATUTINO')",
    )
    .execute(&mut *tx)
    .await;

    assert!(
        reuso.is_err(),
        "una sección dada de baja sigue ocupando su combinación"
    );

    tx.rollback().await?;
    Ok(())
}
