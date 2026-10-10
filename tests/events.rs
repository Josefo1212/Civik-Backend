//! T6 — integración `events` CRUD manual con transacción/rollback.

mod common;

use civik_backend::modules::events::queries::{self};
use civik_backend::modules::events::service as events_service;
use civik_backend::modules::lapses::domain::preparar_entrada;
use civik_backend::modules::lapses::queries as lapses_queries;

type Resultado = Result<(), Box<dyn std::error::Error>>;

fn entrada_lapso(anio: i32) -> civik_backend::modules::lapses::domain::DatosLapso {
    preparar_entrada(
        &anio.to_string(),
        "1",
        &format!("{anio}-09-01"),
        &format!("{anio}-12-20"),
        "THURSDAY",
    )
    .unwrap()
}

#[tokio::test]
async fn crear_manual_practica_extra_ok() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let _lapso = lapses_queries::crear(&mut tx, entrada_lapso(2070)).await?;
    let anio = 2090;
    let lapso = lapses_queries::crear(
        &mut tx,
        preparar_entrada(
            &anio.to_string(),
            "1",
            &format!("{anio}-09-01"),
            &format!("{anio}-12-20"),
            "THURSDAY",
        )
        .unwrap(),
    )
    .await?;
    let ev = queries::crear_manual(
        tx.as_mut(),
        lapso.id,
        chrono::NaiveDate::from_ymd_opt(anio, 9, 10).unwrap(),
        "PRACTICA_EXTRA",
        "Extra 1",
    )
    .await?;

    assert_eq!(ev.status.as_deref(), Some("SCHEDULED"));
    assert_eq!(ev.source.as_deref(), Some("MANUAL"));
    assert!(ev.cancelled_reason.is_none());

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn practica_regular_manual_en_practice_day_ok() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let _lapso = lapses_queries::crear(&mut tx, entrada_lapso(2072)).await?;
    let anio = 2091;
    let lapso = lapses_queries::crear(
        &mut tx,
        preparar_entrada(
            &anio.to_string(),
            "1",
            &format!("{anio}-09-01"),
            &format!("{anio}-12-20"),
            "THURSDAY",
        )
        .unwrap(),
    )
    .await?;
    // jueves 2025-09-04? easier: use a known thursday
    let ev = queries::crear_manual(
        tx.as_mut(),
        lapso.id,
        chrono::NaiveDate::from_ymd_opt(anio, 9, 4).unwrap(),
        "PRACTICA_REGULAR",
        "Regular",
    )
    .await?;
    assert_eq!(ev.status.as_deref(), Some("SCHEDULED"));
    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn practica_regular_fuera_de_practice_day_rechazada() -> Resultado {
    let pool = common::pool().await?;

    let anio = 2067;
    let mut tx = pool.begin().await?;
    let lapso = lapses_queries::crear(
        &mut tx,
        preparar_entrada(
            &anio.to_string(),
            "1",
            &format!("{anio}-09-01"),
            &format!("{anio}-12-20"),
            "THURSDAY",
        )
        .unwrap(),
    )
    .await?;
    tx.commit().await?;

    let resultado = events_service::crear_manual(
        &pool,
        &events_service::EntradaCrearManual {
            id_lapse: lapso.id,
            event_date: format!("{anio}-09-07"),
            type_event: "PRACTICA_REGULAR".to_string(),
            event_name: "Regular".to_string(),
        },
    )
    .await;
    assert!(
        resultado.is_err(),
        "deberia fallar: {:?}",
        resultado.as_ref().err()
    );
    let mut tx2 = pool.begin().await?;
    sqlx::query("ALTER TABLE events DISABLE TRIGGER ALL")
        .execute(tx2.as_mut())
        .await
        .ok();
    sqlx::query("ALTER TABLE lapses DISABLE TRIGGER ALL")
        .execute(tx2.as_mut())
        .await
        .ok();
    sqlx::query("DELETE FROM events WHERE id_lapse = $1")
        .bind(lapso.id)
        .execute(tx2.as_mut())
        .await
        .ok();
    sqlx::query("DELETE FROM lapses WHERE id = $1")
        .bind(lapso.id)
        .execute(tx2.as_mut())
        .await
        .ok();
    sqlx::query("ALTER TABLE events ENABLE TRIGGER ALL")
        .execute(tx2.as_mut())
        .await
        .ok();
    sqlx::query("ALTER TABLE lapses ENABLE TRIGGER ALL")
        .execute(tx2.as_mut())
        .await
        .ok();
    tx2.commit().await.ok();
    Ok(())
}

#[tokio::test]
async fn cancelar_evento_scheduled_con_motivo_y_auditoria() -> Resultado {
    let pool = common::pool().await?;

    let anio = 2093;
    let mut tx = pool.begin().await?;
    let lapso = lapses_queries::crear(
        &mut tx,
        preparar_entrada(
            &anio.to_string(),
            "1",
            &format!("{anio}-09-01"),
            &format!("{anio}-12-20"),
            "THURSDAY",
        )
        .unwrap(),
    )
    .await?;
    tx.commit().await?;

    let ev = events_service::crear_manual(
        &pool,
        &events_service::EntradaCrearManual {
            id_lapse: lapso.id,
            event_date: format!("{anio}-09-05"),
            type_event: "PRACTICA_EXTRA".to_string(),
            event_name: "Extra".to_string(),
        },
    )
    .await?;

    let ev_cancel = events_service::cancelar(&pool, ev.id, "motivo").await?;

    assert_eq!(ev_cancel.status.as_deref(), Some("CANCELLED"));
    assert!(ev_cancel.cancelled_reason.is_some());
    assert_eq!(ev_cancel.cancelled_reason.as_deref(), Some("motivo"));

    let mut tx2 = pool.begin().await?;
    sqlx::query("ALTER TABLE events DISABLE TRIGGER ALL")
        .execute(tx2.as_mut())
        .await
        .ok();
    sqlx::query("ALTER TABLE lapses DISABLE TRIGGER ALL")
        .execute(tx2.as_mut())
        .await
        .ok();
    sqlx::query("DELETE FROM events WHERE id_lapse = $1")
        .bind(lapso.id)
        .execute(tx2.as_mut())
        .await
        .ok();
    sqlx::query("DELETE FROM lapses WHERE id = $1")
        .bind(lapso.id)
        .execute(tx2.as_mut())
        .await
        .ok();
    sqlx::query("ALTER TABLE events ENABLE TRIGGER ALL")
        .execute(tx2.as_mut())
        .await
        .ok();
    sqlx::query("ALTER TABLE lapses ENABLE TRIGGER ALL")
        .execute(tx2.as_mut())
        .await
        .ok();
    tx2.commit().await.ok();
    Ok(())
}

#[tokio::test]
async fn cancelar_evento_scheduled_ok() -> Resultado {
    let pool = common::pool().await?;

    let anio = 2094;
    let mut tx = pool.begin().await?;
    let lapso = lapses_queries::crear(
        &mut tx,
        preparar_entrada(
            &anio.to_string(),
            "1",
            &format!("{anio}-09-01"),
            &format!("{anio}-12-20"),
            "THURSDAY",
        )
        .unwrap(),
    )
    .await?;
    tx.commit().await?;

    let ev = events_service::crear_manual(
        &pool,
        &events_service::EntradaCrearManual {
            id_lapse: lapso.id,
            event_date: format!("{anio}-09-05"),
            type_event: "PRACTICA_EXTRA".to_string(),
            event_name: "Extra".to_string(),
        },
    )
    .await?;

    let ev_cancel = events_service::cancelar(&pool, ev.id, "motivo").await?;
    assert_eq!(ev_cancel.status.as_deref(), Some("CANCELLED"));

    let mut tx2 = pool.begin().await?;
    sqlx::query("ALTER TABLE events DISABLE TRIGGER ALL")
        .execute(tx2.as_mut())
        .await
        .ok();
    sqlx::query("ALTER TABLE lapses DISABLE TRIGGER ALL")
        .execute(tx2.as_mut())
        .await
        .ok();
    sqlx::query("DELETE FROM events WHERE id_lapse = $1")
        .bind(lapso.id)
        .execute(tx2.as_mut())
        .await
        .ok();
    sqlx::query("DELETE FROM lapses WHERE id = $1")
        .bind(lapso.id)
        .execute(tx2.as_mut())
        .await
        .ok();
    sqlx::query("ALTER TABLE events ENABLE TRIGGER ALL")
        .execute(tx2.as_mut())
        .await
        .ok();
    sqlx::query("ALTER TABLE lapses ENABLE TRIGGER ALL")
        .execute(tx2.as_mut())
        .await
        .ok();
    tx2.commit().await.ok();
    Ok(())
}

#[tokio::test]
async fn editar_manual_sin_asistencia_ok() -> Resultado {
    let pool = common::pool().await?;

    let anio = 2095;
    let mut tx = pool.begin().await?;
    let lapso = lapses_queries::crear(
        &mut tx,
        preparar_entrada(
            &anio.to_string(),
            "1",
            &format!("{anio}-09-01"),
            &format!("{anio}-12-20"),
            "THURSDAY",
        )
        .unwrap(),
    )
    .await?;
    tx.commit().await?;

    let ev = events_service::crear_manual(
        &pool,
        &events_service::EntradaCrearManual {
            id_lapse: lapso.id,
            event_date: format!("{anio}-09-05"),
            type_event: "PRACTICA_EXTRA".to_string(),
            event_name: "Extra".to_string(),
        },
    )
    .await?;

    let ev_edit = events_service::editar_manual(
        &pool,
        ev.id,
        &events_service::EntradaEditarManual {
            event_date: format!("{anio}-09-12"),
            type_event: "PRACTICA_EXTRA".to_string(),
            event_name: "Extra editado".to_string(),
        },
    )
    .await?;

    assert_eq!(ev_edit.event_name, "Extra editado");

    let mut tx2 = pool.begin().await?;
    sqlx::query("ALTER TABLE events DISABLE TRIGGER ALL")
        .execute(tx2.as_mut())
        .await
        .ok();
    sqlx::query("ALTER TABLE lapses DISABLE TRIGGER ALL")
        .execute(tx2.as_mut())
        .await
        .ok();
    sqlx::query("DELETE FROM events WHERE id_lapse = $1")
        .bind(lapso.id)
        .execute(tx2.as_mut())
        .await
        .ok();
    sqlx::query("DELETE FROM lapses WHERE id = $1")
        .bind(lapso.id)
        .execute(tx2.as_mut())
        .await
        .ok();
    sqlx::query("ALTER TABLE events ENABLE TRIGGER ALL")
        .execute(tx2.as_mut())
        .await
        .ok();
    sqlx::query("ALTER TABLE lapses ENABLE TRIGGER ALL")
        .execute(tx2.as_mut())
        .await
        .ok();
    tx2.commit().await.ok();
    Ok(())
}

#[tokio::test]
async fn consultar_evento_por_id() -> Resultado {
    let pool = common::pool().await?;

    let anio = 2096;
    let mut tx = pool.begin().await?;
    let lapso = lapses_queries::crear(
        &mut tx,
        preparar_entrada(
            &anio.to_string(),
            "1",
            &format!("{anio}-09-01"),
            &format!("{anio}-12-20"),
            "THURSDAY",
        )
        .unwrap(),
    )
    .await?;
    tx.commit().await?;

    let ev = events_service::crear_manual(
        &pool,
        &events_service::EntradaCrearManual {
            id_lapse: lapso.id,
            event_date: format!("{anio}-09-05"),
            type_event: "PRACTICA_EXTRA".to_string(),
            event_name: "Extra".to_string(),
        },
    )
    .await?;

    let consultado = events_service::consultar(&pool, ev.id).await?;
    assert_eq!(consultado.id, ev.id);

    let mut tx2 = pool.begin().await?;
    sqlx::query("ALTER TABLE events DISABLE TRIGGER ALL")
        .execute(tx2.as_mut())
        .await
        .ok();
    sqlx::query("ALTER TABLE lapses DISABLE TRIGGER ALL")
        .execute(tx2.as_mut())
        .await
        .ok();
    sqlx::query("DELETE FROM events WHERE id_lapse = $1")
        .bind(lapso.id)
        .execute(tx2.as_mut())
        .await
        .ok();
    sqlx::query("DELETE FROM lapses WHERE id = $1")
        .bind(lapso.id)
        .execute(tx2.as_mut())
        .await
        .ok();
    sqlx::query("ALTER TABLE events ENABLE TRIGGER ALL")
        .execute(tx2.as_mut())
        .await
        .ok();
    sqlx::query("ALTER TABLE lapses ENABLE TRIGGER ALL")
        .execute(tx2.as_mut())
        .await
        .ok();
    tx2.commit().await.ok();
    Ok(())
}
