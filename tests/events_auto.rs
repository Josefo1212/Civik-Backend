//! T5 — integración de la fase 6: generación automática de prácticas y
//! reprogramación del día de práctica (RF-14 a RF-18, RF-20).
//!
//! Los `eventos_service::generar_practicas_auto` y
//! `reprogramar_practice_day` abren su propia transacción y hacen COMMIT,
//! así que cada test crea su lapso (committed), ejercita el servicio y luego
//! limpia con `DISABLE TRIGGER ALL` + DELETE, mismo patrón que `events.rs`.
//! Un mutex serializa los tests de este binario para que el desactivado de
//! triggers nunca se pise con otro test del mismo proceso.
//!
//! Las fechas se calculan en el test (no se hardcodean): `primer_dia` devuelve
//! el primer `Weekday` del año y 27 días más, lo que garantiza exactamente 4
//! días de práctica y que cada uno tenga el día siguiente dentro del rango.

mod common;

use chrono::{Datelike, Duration, NaiveDate, Weekday};
use civik_backend::modules::events::queries as events_queries;
use civik_backend::modules::events::service as events_service;
use civik_backend::modules::lapses::domain::preparar_entrada;
use civik_backend::modules::lapses::queries as lapses_queries;
use sqlx::PgPool;
use uuid::Uuid;

type Resultado = Result<(), Box<dyn std::error::Error>>;

static CICLO_DE_VIDA: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Primer `dia` del año + 27 días: 4 días de práctica, todos con el día
/// siguiente dentro del rango.
fn primer_dia(anio: i32, dia: Weekday) -> (NaiveDate, NaiveDate) {
    let base = NaiveDate::from_ymd_opt(anio, 1, 1).unwrap();
    let mut inicio = base;
    while inicio.weekday() != dia {
        inicio = inicio.succ_opt().unwrap();
    }
    (inicio, inicio + Duration::days(27))
}

fn contar_dias(inicio: NaiveDate, fin: NaiveDate, dia: Weekday) -> usize {
    let mut cantidad = 0;
    let mut fecha = inicio;
    while fecha <= fin {
        if fecha.weekday() == dia {
            cantidad += 1;
        }
        fecha = fecha.succ_opt().unwrap();
    }
    cantidad
}

async fn crear_lapso(
    pool: &PgPool,
    inicio: NaiveDate,
    fin: NaiveDate,
    dia: &str,
) -> Result<Uuid, Box<dyn std::error::Error>> {
    let anio = inicio.year();
    let mut tx = pool.begin().await?;
    let lapso = lapses_queries::crear(
        &mut tx,
        preparar_entrada(
            &anio.to_string(),
            "1",
            &inicio.to_string(),
            &fin.to_string(),
            dia,
        )
        .unwrap(),
    )
    .await?;
    tx.commit().await?;
    Ok(lapso.id)
}

async fn contar_auditoria(pool: &PgPool, accion: &str, id_lapse: Uuid) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT count(*) FROM audit_logs
         WHERE entity = 'lapses' AND entity_id = $1 AND action = $2",
    )
    .bind(id_lapse)
    .bind(accion)
    .fetch_one(pool)
    .await
}

/// Borra lo creado por un test sin tocar `audit_logs` (append-only).
#[allow(clippy::too_many_arguments)]
async fn limpiar(
    pool: &PgPool,
    id_lapse: Uuid,
    id_estudiante: Option<Uuid>,
    id_seccion: Option<Uuid>,
) -> Resultado {
    let mut tx = pool.begin().await?;
    let tablas = ["attendance", "events", "students", "sections", "lapses"];
    for tabla in tablas {
        sqlx::query(&format!("ALTER TABLE {tabla} DISABLE TRIGGER ALL"))
            .execute(tx.as_mut())
            .await?;
    }
    sqlx::query(
        "DELETE FROM attendance WHERE id_event IN (SELECT id FROM events WHERE id_lapse = $1)",
    )
    .bind(id_lapse)
    .execute(tx.as_mut())
    .await?;
    sqlx::query("DELETE FROM events WHERE id_lapse = $1")
        .bind(id_lapse)
        .execute(tx.as_mut())
        .await?;
    if let Some(id) = id_estudiante {
        sqlx::query("DELETE FROM students WHERE id = $1")
            .bind(id)
            .execute(tx.as_mut())
            .await?;
    }
    if let Some(id) = id_seccion {
        sqlx::query("DELETE FROM sections WHERE id = $1")
            .bind(id)
            .execute(tx.as_mut())
            .await?;
    }
    sqlx::query("DELETE FROM lapses WHERE id = $1")
        .bind(id_lapse)
        .execute(tx.as_mut())
        .await?;
    for tabla in tablas {
        sqlx::query(&format!("ALTER TABLE {tabla} ENABLE TRIGGER ALL"))
            .execute(tx.as_mut())
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

#[tokio::test]
async fn generar_crea_una_auto_por_dia_de_practica() -> Resultado {
    let _lock = CICLO_DE_VIDA.lock().await;
    let pool = common::pool().await?;
    let (inicio, fin) = primer_dia(2080, Weekday::Thu);
    let id_lapse = crear_lapso(&pool, inicio, fin, "THURSDAY").await?;

    let generados = events_service::generar_practicas_auto(&pool, id_lapse).await?;

    assert!(!generados.is_empty());
    for evento in &generados {
        assert_eq!(evento.source.as_deref(), Some("AUTO"));
        assert_eq!(evento.type_event.as_deref(), Some("PRACTICA_REGULAR"));
        assert_eq!(evento.status.as_deref(), Some("SCHEDULED"));
        assert_eq!(evento.event_date.weekday(), Weekday::Thu);
    }
    assert_eq!(generados.len(), contar_dias(inicio, fin, Weekday::Thu));

    // Una sola entrada de auditoría por operación.
    assert_eq!(
        contar_auditoria(&pool, "generar_practicas_auto", id_lapse).await?,
        1
    );

    limpiar(&pool, id_lapse, None, None).await
}

#[tokio::test]
async fn generar_es_idempotente_segunda_llamada_no_crea() -> Resultado {
    let _lock = CICLO_DE_VIDA.lock().await;
    let pool = common::pool().await?;
    let (inicio, fin) = primer_dia(2081, Weekday::Thu);
    let id_lapse = crear_lapso(&pool, inicio, fin, "THURSDAY").await?;

    let generados = events_service::generar_practicas_auto(&pool, id_lapse).await?;
    assert!(!generados.is_empty());

    let segunda = events_service::generar_practicas_auto(&pool, id_lapse).await?;
    assert!(segunda.is_empty(), "segunda llamada no debe crear nada");

    assert_eq!(
        events_queries::listar_por_lapso(&pool, id_lapse)
            .await?
            .len(),
        generados.len()
    );
    assert_eq!(
        contar_auditoria(&pool, "generar_practicas_auto", id_lapse).await?,
        2
    );

    limpiar(&pool, id_lapse, None, None).await
}

#[tokio::test]
async fn generar_omite_fecha_con_regular_manual_o_cancelada() -> Resultado {
    let _lock = CICLO_DE_VIDA.lock().await;
    let pool = common::pool().await?;
    let (inicio, fin) = primer_dia(2082, Weekday::Thu);
    let id_lapse = crear_lapso(&pool, inicio, fin, "THURSDAY").await?;

    let jueves = [inicio, inicio + Duration::days(7)];
    let mut tx = pool.begin().await?;
    let manual = events_queries::crear_manual(
        tx.as_mut(),
        id_lapse,
        jueves[0],
        "PRACTICA_REGULAR",
        "Regular manual",
    )
    .await?;
    let a_cancelar = events_queries::crear_manual(
        tx.as_mut(),
        id_lapse,
        jueves[1],
        "PRACTICA_REGULAR",
        "Regular cancelada",
    )
    .await?;
    tx.commit().await?;
    events_service::cancelar(&pool, a_cancelar.id, "prueba").await?;

    let generados = events_service::generar_practicas_auto(&pool, id_lapse).await?;

    // Las dos fechas ocupadas por regular (una viva y una cancelada) se omiten.
    assert_eq!(
        generados.len(),
        contar_dias(inicio, fin, Weekday::Thu).saturating_sub(2)
    );
    assert!(generados
        .iter()
        .all(|e| e.event_date != jueves[0] && e.event_date != jueves[1]));
    assert!(manual.id != a_cancelar.id);

    limpiar(&pool, id_lapse, None, None).await
}

#[tokio::test]
async fn extra_y_presentacion_sobre_practice_day_no_bloquean_la_regular() -> Resultado {
    let _lock = CICLO_DE_VIDA.lock().await;
    let pool = common::pool().await?;
    let (inicio, fin) = primer_dia(2083, Weekday::Thu);
    let id_lapse = crear_lapso(&pool, inicio, fin, "THURSDAY").await?;

    let mut tx = pool.begin().await?;
    events_queries::crear_manual(tx.as_mut(), id_lapse, inicio, "PRACTICA_EXTRA", "Extra").await?;
    events_queries::crear_manual(
        tx.as_mut(),
        id_lapse,
        inicio + Duration::days(7),
        "PRESENTACION",
        "Presentación",
    )
    .await?;
    tx.commit().await?;

    let generados = events_service::generar_practicas_auto(&pool, id_lapse).await?;

    // EXTRA/PRESENTACION no ocupan el cupo de la práctica regular.
    assert_eq!(generados.len(), contar_dias(inicio, fin, Weekday::Thu));

    limpiar(&pool, id_lapse, None, None).await
}

#[tokio::test]
async fn generar_en_lapso_cerrado_responde_conflict() -> Resultado {
    let _lock = CICLO_DE_VIDA.lock().await;
    let pool = common::pool().await?;
    let (inicio, fin) = primer_dia(2086, Weekday::Thu);
    let id_lapse = crear_lapso(&pool, inicio, fin, "THURSDAY").await?;

    let mut tx = pool.begin().await?;
    sqlx::query("UPDATE lapses SET status = 'CLOSED' WHERE id = $1")
        .bind(id_lapse)
        .execute(tx.as_mut())
        .await?;
    tx.commit().await?;

    let resultado = events_service::generar_practicas_auto(&pool, id_lapse).await;
    assert!(
        matches!(
            resultado,
            Err(civik_backend::shared::error::AppError::Conflict(_))
        ),
        "un lapso CLOSED debe responder 409, llegó: {resultado:?}"
    );

    limpiar(&pool, id_lapse, None, None).await
}

#[tokio::test]
async fn reprogramar_mueve_pendientes_y_actualiza_practice_day() -> Resultado {
    let _lock = CICLO_DE_VIDA.lock().await;
    let pool = common::pool().await?;
    let (inicio, fin) = primer_dia(2084, Weekday::Thu);
    let id_lapse = crear_lapso(&pool, inicio, fin, "THURSDAY").await?;

    events_service::generar_practicas_auto(&pool, id_lapse).await?;
    let extra_fecha = inicio + Duration::days(3);
    let mut tx = pool.begin().await?;
    let extra = events_queries::crear_manual(
        tx.as_mut(),
        id_lapse,
        extra_fecha,
        "PRACTICA_EXTRA",
        "Extra",
    )
    .await?;
    tx.commit().await?;

    let resultado = events_service::reprogramar_practice_day(&pool, id_lapse, "MONDAY").await?;
    assert_eq!(resultado.dia_anterior, "THURSDAY");
    assert_eq!(resultado.dia_nuevo, "MONDAY");
    assert_eq!(resultado.movidos, 4);
    assert_eq!(resultado.no_movidos, 0);

    let dia_actual: String =
        sqlx::query_scalar("SELECT practice_day::text FROM lapses WHERE id = $1")
            .bind(id_lapse)
            .fetch_one(&pool)
            .await?;
    assert_eq!(dia_actual, "MONDAY");

    // Los AUTO pendientes quedan en el lunes siguiente a su jueves original.
    let eventos = events_queries::listar_por_lapso(&pool, id_lapse).await?;
    let autos: Vec<_> = eventos
        .iter()
        .filter(|e| e.source.as_deref() == Some("AUTO"))
        .map(|e| e.event_date)
        .collect();
    let esperado = [
        inicio + Duration::days(4),
        inicio + Duration::days(11),
        inicio + Duration::days(18),
        inicio + Duration::days(25),
    ];
    assert_eq!(autos, esperado);

    // EXTRA no es PRACTICA_REGULAR: no se mueve.
    let extra_db = eventos
        .iter()
        .find(|e| e.id == extra.id)
        .expect("el evento extra debe seguir existiendo");
    assert_eq!(extra_db.event_date, extra_fecha);

    assert_eq!(
        contar_auditoria(&pool, "reprogramar_practice_day", id_lapse).await?,
        1
    );

    limpiar(&pool, id_lapse, None, None).await
}

#[tokio::test]
async fn reprogramar_no_mueve_evento_con_asistencia() -> Resultado {
    let _lock = CICLO_DE_VIDA.lock().await;
    let pool = common::pool().await?;
    let (inicio, fin) = primer_dia(2085, Weekday::Thu);
    let id_lapse = crear_lapso(&pool, inicio, fin, "THURSDAY").await?;

    let autos = events_service::generar_practicas_auto(&pool, id_lapse).await?;

    let mut tx = pool.begin().await?;
    let id_seccion: Uuid = sqlx::query_scalar(
        "INSERT INTO sections (grade_level, section, shift) VALUES ('Prueba2085', 'Z', 'MATUTINO') RETURNING id",
    )
    .fetch_one(tx.as_mut())
    .await?;
    let id_estudiante: Uuid = sqlx::query_scalar(
        "INSERT INTO students (id_section, first_name, last_name) VALUES ($1, 'Test', 'Alumno') RETURNING id",
    )
    .bind(id_seccion)
    .fetch_one(tx.as_mut())
    .await?;
    // Con asistencia, el evento «ocurrió» y queda congelado (regla 9).
    sqlx::query("INSERT INTO attendance (id_event, id_student, state) VALUES ($1, $2, 'PRESENTE')")
        .bind(autos[0].id)
        .bind(id_estudiante)
        .execute(tx.as_mut())
        .await?;
    tx.commit().await?;

    let resultado = events_service::reprogramar_practice_day(&pool, id_lapse, "MONDAY").await?;
    assert_eq!(resultado.movidos, 3);
    assert_eq!(resultado.no_movidos, 0);
    assert_eq!(resultado.dia_nuevo, "MONDAY");

    let eventos = events_queries::listar_por_lapso(&pool, id_lapse).await?;
    let congelado = eventos
        .iter()
        .find(|e| e.id == autos[0].id)
        .expect("el evento con asistencia debe seguir existiendo");
    assert_eq!(
        congelado.event_date, inicio,
        "el congelado no puede moverse"
    );

    limpiar(&pool, id_lapse, Some(id_estudiante), Some(id_seccion)).await
}

#[tokio::test]
async fn reprogramar_no_mueve_cancelado_ni_extra_presentacion() -> Resultado {
    let _lock = CICLO_DE_VIDA.lock().await;
    let pool = common::pool().await?;
    let (inicio, fin) = primer_dia(2087, Weekday::Thu);
    let id_lapse = crear_lapso(&pool, inicio, fin, "THURSDAY").await?;

    let autos = events_service::generar_practicas_auto(&pool, id_lapse).await?;
    events_service::cancelar(&pool, autos[0].id, "prueba").await?;

    let mut tx = pool.begin().await?;
    let extra_fecha = inicio + Duration::days(2);
    events_queries::crear_manual(
        tx.as_mut(),
        id_lapse,
        extra_fecha,
        "PRACTICA_EXTRA",
        "Extra",
    )
    .await?;
    events_queries::crear_manual(
        tx.as_mut(),
        id_lapse,
        inicio + Duration::days(3),
        "PRESENTACION",
        "Presentación",
    )
    .await?;
    tx.commit().await?;

    let resultado = events_service::reprogramar_practice_day(&pool, id_lapse, "MONDAY").await?;
    // Solo los 3 AUTO pendientes se mueven; el cancelado y los no-regulares se quedan.
    assert_eq!(resultado.movidos, 3);
    assert_eq!(resultado.no_movidos, 0);

    let eventos = events_queries::listar_por_lapso(&pool, id_lapse).await?;
    let cancelado = eventos
        .iter()
        .find(|e| e.id == autos[0].id)
        .expect("el cancelado debe seguir existiendo");
    assert_eq!(
        cancelado.event_date, inicio,
        "el CANCELLED no puede moverse"
    );
    assert!(eventos
        .iter()
        .any(|e| e.type_event.as_deref() == Some("PRACTICA_EXTRA") && e.event_date == extra_fecha));
    assert!(eventos
        .iter()
        .any(|e| e.type_event.as_deref() == Some("PRESENTACION")
            && e.event_date == inicio + Duration::days(3)));

    limpiar(&pool, id_lapse, None, None).await
}

#[tokio::test]
async fn reprogramar_respeta_destino_ocupado() -> Resultado {
    let _lock = CICLO_DE_VIDA.lock().await;
    let pool = common::pool().await?;
    let (inicio, fin) = primer_dia(2088, Weekday::Thu);
    let id_lapse = crear_lapso(&pool, inicio, fin, "THURSDAY").await?;

    // Dos regulares MANUAL en el primer y segundo jueves.
    let mut tx = pool.begin().await?;
    let primera =
        events_queries::crear_manual(tx.as_mut(), id_lapse, inicio, "PRACTICA_REGULAR", "Primera")
            .await?;
    let segunda = events_queries::crear_manual(
        tx.as_mut(),
        id_lapse,
        inicio + Duration::days(7),
        "PRACTICA_REGULAR",
        "Segunda",
    )
    .await?;
    tx.commit().await?;

    // Se planta una regular MANUAL CANCELLED en el destino de la primera (el
    // viernes siguiente) con los triggers desactivados: la base no admite una
    // regular fuera del practice_day, pero el service debe respetar el cupo si
    // la fecha estuviera ocupada de cualquier forma.
    let destino_ocupado = inicio + Duration::days(1);
    let mut tx = pool.begin().await?;
    sqlx::query("ALTER TABLE events DISABLE TRIGGER ALL")
        .execute(tx.as_mut())
        .await?;
    let intruso: Uuid = sqlx::query_scalar(
        "INSERT INTO events (id_lapse, event_date, type_event, source, event_name, status, cancelled_reason)
         VALUES ($1, $2, 'PRACTICA_REGULAR', 'MANUAL', 'Ocupa', 'CANCELLED', 'prueba')
         RETURNING id",
    )
    .bind(id_lapse)
    .bind(destino_ocupado)
    .fetch_one(tx.as_mut())
    .await?;
    sqlx::query("ALTER TABLE events ENABLE TRIGGER ALL")
        .execute(tx.as_mut())
        .await?;
    tx.commit().await?;

    let resultado = events_service::reprogramar_practice_day(&pool, id_lapse, "FRIDAY").await?;
    assert_eq!(resultado.dia_nuevo, "FRIDAY");
    // La primera no tiene cupo (su viernes lo ocupa el CANCELLED); la segunda
    // sí se mueve a su siguiente viernes libre.
    assert_eq!(resultado.movidos, 1);
    assert_eq!(resultado.no_movidos, 1);

    let eventos = events_queries::listar_por_lapso(&pool, id_lapse).await?;
    let primera_db = eventos
        .iter()
        .find(|e| e.id == primera.id)
        .expect("la primera regular debe seguir existiendo");
    assert_eq!(
        primera_db.event_date, inicio,
        "el destino ocupado se respeta"
    );
    let segunda_db = eventos
        .iter()
        .find(|e| e.id == segunda.id)
        .expect("la segunda regular debe seguir existiendo");
    assert_eq!(segunda_db.event_date, inicio + Duration::days(8));
    let intruso_db = eventos
        .iter()
        .find(|e| e.id == intruso)
        .expect("el intruso debe seguir existiendo");
    assert_eq!(
        intruso_db.event_date, destino_ocupado,
        "lo CANCELLED no se mueve"
    );
    assert_eq!(intruso_db.status.as_deref(), Some("CANCELLED"));

    limpiar(&pool, id_lapse, None, None).await
}

#[tokio::test]
async fn reprogramar_sin_cupo_posterior_suma_no_movidos() -> Resultado {
    let _lock = CICLO_DE_VIDA.lock().await;
    let pool = common::pool().await?;
    let (inicio, _) = primer_dia(2079, Weekday::Thu);
    // El lapso termina justo en el tercer jueves: ese AUTO no tiene un lunes
    // posterior dentro del rango (RF-18).
    let fin = inicio + Duration::days(14);
    let id_lapse = crear_lapso(&pool, inicio, fin, "THURSDAY").await?;

    let autos = events_service::generar_practicas_auto(&pool, id_lapse).await?;
    assert_eq!(autos.len(), 3);

    let resultado = events_service::reprogramar_practice_day(&pool, id_lapse, "MONDAY").await?;
    assert_eq!(resultado.dia_nuevo, "MONDAY");
    assert_eq!(resultado.movidos, 2);
    assert_eq!(resultado.no_movidos, 1);

    let eventos = events_queries::listar_por_lapso(&pool, id_lapse).await?;
    let ultimo = eventos
        .iter()
        .find(|e| e.id == autos[2].id)
        .expect("el AUTO de la última semana debe seguir existiendo");
    assert_eq!(
        ultimo.event_date, fin,
        "sin cupo posterior se queda en su fecha"
    );
    let primero = eventos
        .iter()
        .find(|e| e.id == autos[0].id)
        .expect("el primer AUTO debe seguir existiendo");
    assert_eq!(primero.event_date, inicio + Duration::days(4));

    assert_eq!(
        contar_auditoria(&pool, "reprogramar_practice_day", id_lapse).await?,
        1
    );

    limpiar(&pool, id_lapse, None, None).await
}

#[tokio::test]
async fn reprogramar_con_el_mismo_dia_es_noop_sin_auditoria() -> Resultado {
    let _lock = CICLO_DE_VIDA.lock().await;
    let pool = common::pool().await?;
    let (inicio, fin) = primer_dia(2089, Weekday::Thu);
    let id_lapse = crear_lapso(&pool, inicio, fin, "THURSDAY").await?;

    let autos = events_service::generar_practicas_auto(&pool, id_lapse).await?;

    let resultado = events_service::reprogramar_practice_day(&pool, id_lapse, "THURSDAY").await?;
    assert_eq!(resultado.movidos, 0);
    assert_eq!(resultado.no_movidos, 0);

    let eventos = events_queries::listar_por_lapso(&pool, id_lapse).await?;
    let fechas: Vec<_> = eventos
        .iter()
        .filter(|e| e.source.as_deref() == Some("AUTO"))
        .map(|e| e.event_date)
        .collect();
    let originales: Vec<_> = autos.iter().map(|e| e.event_date).collect();
    assert_eq!(fechas, originales, "el no-op no toca ningún evento");

    // El no-op no deja entrada de auditoría.
    assert_eq!(
        contar_auditoria(&pool, "reprogramar_practice_day", id_lapse).await?,
        0
    );

    limpiar(&pool, id_lapse, None, None).await
}
