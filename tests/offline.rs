//! Tests de la sincronización offline (Fase 9). Todos dentro de una
//! transacción descartable (patrón transacción/rollback): `sincronizar_en_tx`
//! ve lo que todavía no se confirmó, que es justo lo que hay que probar.
//!
//! Cada test usa un año y una sección exclusivos y lapso `PLANNED` (no
//! `ACTIVE`): así no pelea con el índice único de lapso activo ni con los
//! demás archivos que corren en paralelo.

mod common;

use chrono::{Duration, NaiveDate, Utc};
use civik_backend::modules::attendance::domain::{EventId, StudentId};
use civik_backend::modules::attendance::queries as att_queries;
use civik_backend::modules::attendance::service::{
    sincronizar_en_tx, ItemResultado, ItemSync, LoteSync,
};
use civik_backend::modules::events::queries as events_queries;
use civik_backend::modules::lapses::domain::preparar_entrada;
use civik_backend::modules::lapses::queries as lapses_queries;
use civik_backend::modules::sections::domain::{DatosSeccion, Grado, Turno};
use civik_backend::modules::sections::queries as sections_queries;
use civik_backend::modules::students::domain::DatosEstudiante;
use civik_backend::modules::students::queries as students_queries;
use civik_backend::shared::error::AppError;
use sqlx::PgTransaction;
use uuid::Uuid;

type Resultado = Result<(), Box<dyn std::error::Error>>;

fn fecha(anio: i32, mes: u32, dia: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(anio, mes, dia).expect("fecha válida")
}

async fn crear_lapso(
    tx: &mut PgTransaction<'_>,
    anio: i32,
    inicio: NaiveDate,
    fin: NaiveDate,
) -> Result<Uuid, Box<dyn std::error::Error>> {
    // Lapso `PLANNED`: admite eventos y asistencia (solo `CLOSED` la bloquea)
    // y no compite por el índice de lapso activo único.
    let lap = lapses_queries::crear(
        tx,
        preparar_entrada(
            &anio.to_string(),
            "1",
            &inicio.format("%Y-%m-%d").to_string(),
            &fin.format("%Y-%m-%d").to_string(),
            "THURSDAY",
        )?,
    )
    .await?;
    Ok(lap.id)
}

async fn crear_seccion(
    tx: &mut PgTransaction<'_>,
    codigo: &str,
) -> Result<Uuid, Box<dyn std::error::Error>> {
    let sec = sections_queries::crear(
        tx,
        DatosSeccion {
            grado: Grado::Primero,
            seccion: codigo.into(),
            turno: Turno::Matutino,
        },
    )
    .await?;
    Ok(sec.id)
}

async fn crear_estudiante(
    tx: &mut PgTransaction<'_>,
    id_seccion: Uuid,
    nombre: &str,
    apellido: &str,
) -> Result<Uuid, Box<dyn std::error::Error>> {
    let est = students_queries::crear(
        tx,
        DatosEstudiante {
            id_seccion,
            nombre: nombre.into(),
            apellido: apellido.into(),
            telefono: None,
        },
    )
    .await?;
    Ok(est.id)
}

/// Un evento dentro del lapso. `PRACTICA_EXTRA` no está atado al día de
/// práctica, así que cualquier fecha del lapso sirve.
async fn crear_evento(
    tx: &mut PgTransaction<'_>,
    id_lapse: Uuid,
    fecha_evento: NaiveDate,
) -> Result<Uuid, Box<dyn std::error::Error>> {
    let ev = events_queries::crear_manual(
        tx.as_mut(),
        id_lapse,
        fecha_evento,
        "PRACTICA_EXTRA",
        "Clase",
    )
    .await?;
    Ok(ev.id)
}

fn item(id_student: Uuid, state: &str, cambio_en: Option<chrono::DateTime<Utc>>) -> ItemSync {
    ItemSync {
        id_student,
        state: state.into(),
        cambio_en,
    }
}

fn resultado_de(resultados: &[ItemResultado], id: Uuid) -> &ItemResultado {
    resultados
        .iter()
        .find(|r| r.id_student == id)
        .expect("el ítem está en los resultados")
}

async fn estado_de(
    tx: &mut PgTransaction<'_>,
    id_event: Uuid,
    id_student: Uuid,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let fila = att_queries::get(tx.as_mut(), EventId(id_event), StudentId(id_student)).await?;
    Ok(fila.and_then(|f| f.state))
}

async fn auditaron(
    tx: &mut PgTransaction<'_>,
    id_event: Uuid,
) -> Result<i64, Box<dyn std::error::Error>> {
    let (n,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM audit_logs
         WHERE entity = 'attendance' AND entity_id = $1 AND action = 'sincronizar_asistencia'",
    )
    .bind(id_event)
    .fetch_one(tx.as_mut())
    .await?;
    Ok(n)
}

#[tokio::test]
async fn lote_mixto_crea_aplica_y_reporta_conflicto() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let lap = crear_lapso(&mut tx, 2010, fecha(2010, 1, 5), fecha(2010, 12, 16)).await?;
    let sec = crear_seccion(&mut tx, "W10").await?;
    let nuevo = crear_estudiante(&mut tx, sec, "Ana", "Alfa").await?;
    let idem = crear_estudiante(&mut tx, sec, "Beto", "Beta").await?;
    let aplicable = crear_estudiante(&mut tx, sec, "Cira", "Gamma").await?;
    let conflictivo = crear_estudiante(&mut tx, sec, "Dora", "Delta").await?;
    let ev = crear_evento(&mut tx, lap, fecha(2010, 6, 15)).await?;

    // Filas previas: el reintento y el conflicto ya tienen estado en el server.
    att_queries::upsert(tx.as_mut(), EventId(ev), StudentId(idem), "PRESENTE").await?;
    att_queries::upsert(tx.as_mut(), EventId(ev), StudentId(aplicable), "PRESENTE").await?;
    att_queries::upsert(tx.as_mut(), EventId(ev), StudentId(conflictivo), "PRESENTE").await?;

    let ahora = Utc::now();
    let lote = LoteSync {
        items: vec![
            item(nuevo, "AUSENTE", None),
            item(idem, "PRESENTE", None),
            item(aplicable, "AUSENTE", Some(ahora)),
            item(conflictivo, "AUSENTE", Some(ahora - Duration::hours(1))),
        ],
    };

    let resultados = sincronizar_en_tx(&mut tx, EventId(ev), lote).await?;

    assert_eq!(resultados.len(), 4);
    assert_eq!(resultado_de(&resultados, nuevo).resultado, "creado");
    assert_eq!(resultado_de(&resultados, idem).resultado, "sin_cambio");
    assert_eq!(resultado_de(&resultados, aplicable).resultado, "aplicado");
    assert_eq!(
        resultado_de(&resultados, conflictivo).resultado,
        "conflicto"
    );

    assert_eq!(
        estado_de(&mut tx, ev, nuevo).await?.as_deref(),
        Some("AUSENTE")
    );
    assert_eq!(
        estado_de(&mut tx, ev, idem).await?.as_deref(),
        Some("PRESENTE")
    );
    assert_eq!(
        estado_de(&mut tx, ev, aplicable).await?.as_deref(),
        Some("AUSENTE")
    );
    // El conflictivo conserva lo del servidor: no se pisó.
    assert_eq!(
        estado_de(&mut tx, ev, conflictivo).await?.as_deref(),
        Some("PRESENTE")
    );

    // Se auditó una sola vez porque hubo al menos una escritura.
    assert_eq!(auditaron(&mut tx, ev).await?, 1);

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn un_evento_cancelado_rechaza_el_lote() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let lap = crear_lapso(&mut tx, 2011, fecha(2011, 1, 5), fecha(2011, 12, 16)).await?;
    let sec = crear_seccion(&mut tx, "W11").await?;
    let est = crear_estudiante(&mut tx, sec, "Ana", "Alfa").await?;
    let ev = crear_evento(&mut tx, lap, fecha(2011, 6, 15)).await?;
    events_queries::cancelar(tx.as_mut(), ev, "se cayó la clase").await?;

    let lote = LoteSync {
        items: vec![item(est, "AUSENTE", None)],
    };
    let resultado = sincronizar_en_tx(&mut tx, EventId(ev), lote).await;

    assert!(matches!(resultado, Err(AppError::Conflict(_))));

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn un_lapso_cerrado_rechaza_el_lote() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let lap = crear_lapso(&mut tx, 2012, fecha(2012, 1, 5), fecha(2012, 12, 16)).await?;
    let sec = crear_seccion(&mut tx, "W12").await?;
    let est = crear_estudiante(&mut tx, sec, "Ana", "Alfa").await?;
    let ev = crear_evento(&mut tx, lap, fecha(2012, 6, 15)).await?;
    // Cerrar exige pasar por ACTIVE: `cerrar` solo mueve ACTIVE -> CLOSED.
    lapses_queries::activar(&mut tx, lap).await?;
    lapses_queries::cerrar(&mut tx, lap).await?;

    let lote = LoteSync {
        items: vec![item(est, "AUSENTE", None)],
    };
    let resultado = sincronizar_en_tx(&mut tx, EventId(ev), lote).await;

    assert!(matches!(resultado, Err(AppError::Conflict(_))));

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn un_estudiante_de_baja_se_corrige_pero_no_se_crea() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let lap = crear_lapso(&mut tx, 2013, fecha(2013, 1, 5), fecha(2013, 12, 16)).await?;
    let sec = crear_seccion(&mut tx, "W13").await?;
    let con_fila = crear_estudiante(&mut tx, sec, "Ana", "Alfa").await?;
    let sin_fila = crear_estudiante(&mut tx, sec, "Beto", "Beta").await?;
    let ev = crear_evento(&mut tx, lap, fecha(2013, 6, 15)).await?;

    att_queries::upsert(tx.as_mut(), EventId(ev), StudentId(con_fila), "PRESENTE").await?;
    students_queries::set_active(&mut tx, con_fila, false).await?;
    students_queries::set_active(&mut tx, sin_fila, false).await?;

    // Corregir a quien ya tiene fila se aplica aunque esté de baja (RF-7).
    let lote = LoteSync {
        items: vec![item(con_fila, "AUSENTE", Some(Utc::now()))],
    };
    let resultados = sincronizar_en_tx(&mut tx, EventId(ev), lote).await?;
    assert_eq!(resultado_de(&resultados, con_fila).resultado, "aplicado");
    assert_eq!(
        estado_de(&mut tx, ev, con_fila).await?.as_deref(),
        Some("AUSENTE")
    );

    // Crear asistencia para un estudiante de baja se rechaza.
    let lote = LoteSync {
        items: vec![item(sin_fila, "AUSENTE", None)],
    };
    let resultado = sincronizar_en_tx(&mut tx, EventId(ev), lote).await;
    assert!(matches!(resultado, Err(AppError::Conflict(_))));

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn repetir_la_sincronizacion_es_idempotente() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let lap = crear_lapso(&mut tx, 2014, fecha(2014, 1, 5), fecha(2014, 12, 16)).await?;
    let sec = crear_seccion(&mut tx, "W14").await?;
    let est = crear_estudiante(&mut tx, sec, "Ana", "Alfa").await?;
    let ev = crear_evento(&mut tx, lap, fecha(2014, 6, 15)).await?;

    let armar = || LoteSync {
        items: vec![item(est, "AUSENTE", None)],
    };

    let primera = sincronizar_en_tx(&mut tx, EventId(ev), armar()).await?;
    assert_eq!(resultado_de(&primera, est).resultado, "creado");
    assert_eq!(auditaron(&mut tx, ev).await?, 1);

    let segunda = sincronizar_en_tx(&mut tx, EventId(ev), armar()).await?;
    assert_eq!(resultado_de(&segunda, est).resultado, "sin_cambio");
    // El reintento no escribió: no hay una segunda auditoría.
    assert_eq!(auditaron(&mut tx, ev).await?, 1);

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn un_estado_invalido_es_bad_request() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let lap = crear_lapso(&mut tx, 2015, fecha(2015, 1, 5), fecha(2015, 12, 16)).await?;
    let sec = crear_seccion(&mut tx, "W15").await?;
    let est = crear_estudiante(&mut tx, sec, "Ana", "Alfa").await?;
    let ev = crear_evento(&mut tx, lap, fecha(2015, 6, 15)).await?;

    // Un lote vacío no toca la base ni exige nada.
    let vacio = sincronizar_en_tx(&mut tx, EventId(ev), LoteSync { items: vec![] }).await?;
    assert!(vacio.is_empty());

    let lote = LoteSync {
        items: vec![item(est, "TAL_VEZ", None)],
    };
    let resultado = sincronizar_en_tx(&mut tx, EventId(ev), lote).await;

    assert!(matches!(resultado, Err(AppError::BadRequest(_))));
    assert_eq!(estado_de(&mut tx, ev, est).await?, None);

    tx.rollback().await?;
    Ok(())
}
