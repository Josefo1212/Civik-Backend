#[allow(dead_code)]
mod common;

use civik_backend::modules::attendance::queries as att_queries;
use civik_backend::modules::events::queries as events_queries;
use civik_backend::modules::lapses::queries as lapses_queries;
use civik_backend::modules::sections::queries as sections_queries;
use civik_backend::modules::students::queries as students_queries;

type Resultado = Result<(), Box<dyn std::error::Error>>;

#[tokio::test]
async fn registrar_idempotente_no_duplica() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;
    let lap = lapses_queries::crear(
        &mut tx,
        civik_backend::modules::lapses::domain::preparar_entrada(
            "2090",
            "1",
            "2090-09-01",
            "2090-12-20",
            "THURSDAY",
        )?,
    )
    .await?;
    let sec = sections_queries::crear(
        &mut tx,
        civik_backend::modules::sections::domain::DatosSeccion {
            grado: civik_backend::modules::sections::domain::Grado::Primero,
            seccion: "A1".into(),
            turno: civik_backend::modules::sections::domain::Turno::Matutino,
        },
    )
    .await?;
    let est = students_queries::crear(
        &mut tx,
        civik_backend::modules::students::domain::DatosEstudiante {
            id_seccion: sec.id,
            nombre: "A".into(),
            apellido: "B".into(),
            telefono: None,
        },
    )
    .await?;
    let ev = events_queries::crear_manual(
        tx.as_mut(),
        lap.id,
        chrono::NaiveDate::from_ymd_opt(2090, 9, 11).unwrap(),
        "PRACTICA_REGULAR",
        "C",
    )
    .await?;
    let _ = att_queries::insert(tx.as_mut(), ev.id, est.id, "PRESENTE").await?;
    let r2 = att_queries::insert(tx.as_mut(), ev.id, est.id, "PRESENTE").await;
    assert!(r2.is_err());
    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn corregir_cambia_estado_sin_borrar_fila() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;
    let lap = lapses_queries::crear(
        &mut tx,
        civik_backend::modules::lapses::domain::preparar_entrada(
            "2091",
            "1",
            "2091-09-01",
            "2091-12-20",
            "THURSDAY",
        )?,
    )
    .await?;
    let sec = sections_queries::crear(
        &mut tx,
        civik_backend::modules::sections::domain::DatosSeccion {
            grado: civik_backend::modules::sections::domain::Grado::Primero,
            seccion: "A2".into(),
            turno: civik_backend::modules::sections::domain::Turno::Matutino,
        },
    )
    .await?;
    let est = students_queries::crear(
        &mut tx,
        civik_backend::modules::students::domain::DatosEstudiante {
            id_seccion: sec.id,
            nombre: "A".into(),
            apellido: "B".into(),
            telefono: None,
        },
    )
    .await?;
    let ev = events_queries::crear_manual(
        tx.as_mut(),
        lap.id,
        chrono::NaiveDate::from_ymd_opt(2091, 9, 11).unwrap(),
        "PRACTICA_REGULAR",
        "C",
    )
    .await?;
    let _ = att_queries::insert(tx.as_mut(), ev.id, est.id, "PRESENTE").await?;
    let _ = att_queries::set_state(tx.as_mut(), ev.id, est.id, "AUSENTE").await?;
    let lista = att_queries::list_by_event(tx.as_mut(), ev.id).await?;
    assert_eq!(lista.len(), 1);
    assert_eq!(lista[0].state.as_deref(), Some("AUSENTE"));
    tx.rollback().await?;
    Ok(())
}
