//! Fase 10 — invariantes de base (`db/schema.sql`), patrón transacción/rollback.
//!
//! Fija con tests cada `CHECK`, `UNIQUE`, `EXCLUDE`, `FK ... RESTRICT` y
//! trigger que protege las 10 reglas de dominio, de modo que quitar una
//! restricción haga fallar la suite. El intento que debe fallar vive en un
//! `SAVEPOINT` (Postgres aborta la transacción al primer error; sqlx 0.8 no
//! expone savepoints).
//!
//! Cada caso negativo lleva su **positivo adyacente** (sensibilidad): la
//! operación legal más próxima, para probar que la guarda dispara solo ante la
//! violación (ver skill `testing-transaccional`).
//!
//! Ya cubierto en otros archivos (no se duplica acá):
//! - `conflictos_bd.rs`: `uq_lapses_single_active`, `lapses_no_overlap`,
//!   borrado de lapso (`lapses_delete_guard`), fechas vs eventos, `uq_events_auto_one_per_day`,
//!   guardas de asistencia (`check_attendance_guards`, reglas 4/7/10) y
//!   `attendance_identity_immutable`.
//! - `lapses.rs`: únicos de lapso y `trg_lapses_closed_frozen`.
//! - `sections.rs`: `uq_sections_grade_section_shift`, `students_id_section_fkey`.
//! - `students.rs`: `students_names_not_blank`.
//! - `sections_migracion.rs`: `sections_not_blank`.
//!
//! Años escolares (rango 4xxx) y secciones (`Z*`) son exclusivos de este
//! archivo: el EXCLUDE `lapses_no_overlap` es global y los tests corren en
//! paralelo, así que dos tramos vivos solapados se enzarzan en locks del
//! índice GiST.

mod common;

use chrono::{DateTime, Datelike, NaiveDate, Utc, Weekday};
use uuid::Uuid;

type Resultado = Result<(), Box<dyn std::error::Error>>;

/// Ejecuta una sentencia dentro de un `SAVEPOINT` que se deshace siempre, y
/// devuelve el `Result` del `execute` para que el test lo inspeccione.
macro_rules! intento {
    ($tx:expr, $sql:expr $(, $bind:expr)*) => {{
        sqlx::query("SAVEPOINT intento").execute($tx.as_mut()).await?;
        let resultado = sqlx::query($sql) $(.bind($bind))*.execute($tx.as_mut()).await;
        sqlx::query("ROLLBACK TO SAVEPOINT intento").execute($tx.as_mut()).await?;
        resultado
    }};
}

/// (SQLSTATE, nombre de restricción) de un error de base, o panic si no lo es.
fn sqlstate(error: &sqlx::Error) -> (String, Option<String>) {
    match error {
        sqlx::Error::Database(base) => (
            base.code().map(|c| c.into_owned()).unwrap_or_default(),
            base.constraint().map(str::to_string),
        ),
        _ => panic!("esperaba un error de base de datos, llegó: {error}"),
    }
}

/// Inserta un lapso con SQL directo (saltea el dominio, que es el punto:
/// probamos la base). Devuelve su id.
async fn insertar_lapso(
    tx: &mut sqlx::PgTransaction<'_>,
    anio: i32,
    numero: i16,
    inicio: &str,
    fin: &str,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO lapses (school_year, lapse_number, start_date, end_date, practice_day)
         VALUES ($1, $2, $3::date, $4::date, 'THURSDAY')
         RETURNING id",
    )
    .bind(anio)
    .bind(numero)
    .bind(inicio)
    .bind(fin)
    .fetch_one(tx.as_mut())
    .await
}

/// El primer jueves del año (el `practice_day` que usa este archivo).
fn primer_jueves(anio: i32) -> NaiveDate {
    let mut dia = NaiveDate::from_ymd_opt(anio, 1, 1).expect("fecha válida");
    while dia.weekday() != Weekday::Thu {
        dia = dia.succ_opt().expect("fecha válida");
    }
    dia
}

/// Inserta un evento con SQL directo, casteando los enums desde texto.
#[allow(clippy::too_many_arguments)]
async fn insertar_evento(
    tx: &mut sqlx::PgTransaction<'_>,
    id_lapse: Uuid,
    fecha: NaiveDate,
    type_event: &str,
    source: &str,
    status: &str,
    nombre: &str,
    motivo: Option<&str>,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO events (id_lapse, event_date, type_event, source, status, event_name, cancelled_reason)
         VALUES ($1, $2, $3::text::event_type, $4::text::event_source, $5::text::event_status, $6, $7)
         RETURNING id",
    )
    .bind(id_lapse)
    .bind(fecha)
    .bind(type_event)
    .bind(source)
    .bind(status)
    .bind(nombre)
    .bind(motivo)
    .fetch_one(tx.as_mut())
    .await
}

/// Crea sección + estudiante + fila de asistencia para `id_event`.
/// Devuelve el id del estudiante.
async fn montar_asistencia(
    tx: &mut sqlx::PgTransaction<'_>,
    id_event: Uuid,
    anio: i32,
    seccion: &str,
) -> Result<Uuid, sqlx::Error> {
    let id_seccion: Uuid = sqlx::query_scalar(
        "INSERT INTO sections (grade_level, section, shift)
         VALUES ($1, $2, 'MATUTINO'::shift_type) RETURNING id",
    )
    .bind(format!("{anio}"))
    .bind(seccion)
    .fetch_one(tx.as_mut())
    .await?;
    let id_estudiante: Uuid = sqlx::query_scalar(
        "INSERT INTO students (id_section, first_name, last_name)
         VALUES ($1, 'N', 'A') RETURNING id",
    )
    .bind(id_seccion)
    .fetch_one(tx.as_mut())
    .await?;
    sqlx::query("INSERT INTO attendance (id_event, id_student, state) VALUES ($1, $2, 'PRESENTE')")
        .bind(id_event)
        .bind(id_estudiante)
        .execute(tx.as_mut())
        .await?;
    Ok(id_estudiante)
}

/// Crea sección + estudiante sin asistencia. Devuelve (id_seccion, id_estudiante).
async fn insertar_estudiante(
    tx: &mut sqlx::PgTransaction<'_>,
    anio: i32,
    seccion: &str,
) -> Result<(Uuid, Uuid), sqlx::Error> {
    let id_seccion: Uuid = sqlx::query_scalar(
        "INSERT INTO sections (grade_level, section, shift)
         VALUES ($1, $2, 'MATUTINO'::shift_type) RETURNING id",
    )
    .bind(format!("{anio}"))
    .bind(seccion)
    .fetch_one(tx.as_mut())
    .await?;
    let id_estudiante: Uuid = sqlx::query_scalar(
        "INSERT INTO students (id_section, first_name, last_name)
         VALUES ($1, 'N', 'A') RETURNING id",
    )
    .bind(id_seccion)
    .fetch_one(tx.as_mut())
    .await?;
    Ok((id_seccion, id_estudiante))
}

/// Inserta un override de nota; el valor llega como texto y se castea a numeric.
async fn insertar_override(
    tx: &mut sqlx::PgTransaction<'_>,
    id_student: Uuid,
    id_lapse: Uuid,
    valor: &str,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO grade_overrides (id_student, id_lapse, value)
         VALUES ($1, $2, $3::numeric) RETURNING id",
    )
    .bind(id_student)
    .bind(id_lapse)
    .bind(valor)
    .fetch_one(tx.as_mut())
    .await
}

// ---------------------------------------------------------------------------
// lapses
// ---------------------------------------------------------------------------

#[tokio::test]
async fn lapses_dates_order_rechaza_fin_antes_de_inicio_y_acepta_igual() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    // RF-1: end_date < start_date viola lapses_dates_order.
    let r = intento!(
        tx,
        "INSERT INTO lapses (school_year, lapse_number, start_date, end_date, practice_day)
         VALUES ($1, 1, '4001-06-01', '4001-03-01', 'THURSDAY')",
        4001
    );
    let (codigo, restriccion) = sqlstate(&r.expect_err("fin antes de inicio debe violar el CHECK"));
    assert_eq!(codigo, "23514");
    assert_eq!(restriccion.as_deref(), Some("lapses_dates_order"));

    // Sensibilidad: el borde legal (end_date = start_date) pasa.
    let id = insertar_lapso(&mut tx, 4002, 1, "4002-03-01", "4002-03-01").await?;
    assert_ne!(id, Uuid::nil(), "sensibilidad: fin = inicio es válido");

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn lapses_number_between_1_and_3_rechaza_los_extremos_y_acepta_1_y_3() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    // RF-2: lapse_number fuera de 1..=3 viola el CHECK.
    for (anio, numero) in [(4003, 0_i16), (4004, 4_i16)] {
        let r = intento!(
            tx,
            "INSERT INTO lapses (school_year, lapse_number, start_date, end_date, practice_day)
             VALUES ($1, $2, make_date($1, 1, 10), make_date($1, 3, 20), 'THURSDAY')",
            anio,
            numero
        );
        let (codigo, restriccion) =
            sqlstate(&r.expect_err("lapse_number fuera de 1..=3 debe violar el CHECK"));
        assert_eq!(codigo, "23514", "año {anio}/número {numero}");
        assert_eq!(
            restriccion.as_deref(),
            Some("lapses_number_between_1_and_3")
        );
    }

    // Sensibilidad: 1 y 3 pasan (en años distintos, sin solaparse).
    let uno = insertar_lapso(&mut tx, 4005, 1, "4005-01-10", "4005-03-20").await?;
    let tres = insertar_lapso(&mut tx, 4006, 3, "4006-07-10", "4006-09-20").await?;
    assert_ne!(uno, Uuid::nil());
    assert_ne!(tres, Uuid::nil());

    tx.rollback().await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// events — CHECK de columnas
// ---------------------------------------------------------------------------

#[tokio::test]
async fn events_auto_is_regular_practice_rechaza_auto_no_regular() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;
    let lapso = insertar_lapso(&mut tx, 4101, 1, "4101-01-01", "4101-12-20").await?;
    let jueves = primer_jueves(4101);

    // RF-9: source AUTO con type_event <> PRACTICA_REGULAR viola el CHECK.
    let r = intento!(
        tx,
        "INSERT INTO events (id_lapse, event_date, type_event, source, event_name)
         VALUES ($1, $2, 'PRACTICA_EXTRA', 'AUTO', 'Extra')",
        lapso,
        jueves
    );
    let (codigo, restriccion) = sqlstate(&r.expect_err("AUTO debe ser PRACTICA_REGULAR"));
    assert_eq!(codigo, "23514");
    assert_eq!(
        restriccion.as_deref(),
        Some("events_auto_is_regular_practice")
    );

    // Sensibilidad: AUTO + PRACTICA_REGULAR en el practice_day pasa.
    let id = insertar_evento(
        &mut tx,
        lapso,
        jueves,
        "PRACTICA_REGULAR",
        "AUTO",
        "SCHEDULED",
        "Auto válida",
        None,
    )
    .await?;
    assert_ne!(id, Uuid::nil());

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn events_cancelled_requires_reason_rechaza_sin_motivo_o_en_blanco() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;
    let lapso = insertar_lapso(&mut tx, 4102, 1, "4102-01-01", "4102-12-20").await?;
    let fecha = NaiveDate::from_ymd_opt(4102, 5, 10).expect("fecha válida");

    // RF-10: CANCELLED sin motivo.
    let sin_motivo = intento!(
        tx,
        "INSERT INTO events (id_lapse, event_date, type_event, source, status, event_name)
         VALUES ($1, $2, 'PRACTICA_EXTRA', 'MANUAL', 'CANCELLED', 'Extra')",
        lapso,
        fecha
    );
    let (codigo, restriccion) = sqlstate(&sin_motivo.expect_err("CANCELLED exige motivo"));
    assert_eq!(codigo, "23514");
    assert_eq!(
        restriccion.as_deref(),
        Some("events_cancelled_requires_reason")
    );

    // RF-10: CANCELLED con motivo en blanco.
    let en_blanco = intento!(
        tx,
        "INSERT INTO events (id_lapse, event_date, type_event, source, status, event_name, cancelled_reason)
         VALUES ($1, $2, 'PRACTICA_EXTRA', 'MANUAL', 'CANCELLED', 'Extra', '   ')",
        lapso,
        fecha
    );
    let (codigo, _) = sqlstate(&en_blanco.expect_err("motivo en blanco no cuenta"));
    assert_eq!(codigo, "23514");

    // Sensibilidad: CANCELLED con motivo real pasa.
    let id = insertar_evento(
        &mut tx,
        lapso,
        fecha,
        "PRACTICA_EXTRA",
        "MANUAL",
        "CANCELLED",
        "Extra",
        Some("clase suspendida"),
    )
    .await?;
    assert_ne!(id, Uuid::nil());

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn events_scheduled_has_no_reason_rechaza_motivo_estando_scheduled() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;
    let lapso = insertar_lapso(&mut tx, 4103, 1, "4103-01-01", "4103-12-20").await?;
    let fecha = NaiveDate::from_ymd_opt(4103, 5, 10).expect("fecha válida");

    // RF-11: SCHEDULED con cancelled_reason viola el CHECK.
    let r = intento!(
        tx,
        "INSERT INTO events (id_lapse, event_date, type_event, source, status, event_name, cancelled_reason)
         VALUES ($1, $2, 'PRACTICA_EXTRA', 'MANUAL', 'SCHEDULED', 'Extra', 'no debería')",
        lapso,
        fecha
    );
    let (codigo, restriccion) =
        sqlstate(&r.expect_err("SCHEDULED no admite motivo de cancelación"));
    assert_eq!(codigo, "23514");
    assert_eq!(
        restriccion.as_deref(),
        Some("events_scheduled_has_no_reason")
    );

    // Sensibilidad: SCHEDULED sin motivo pasa.
    let id = insertar_evento(
        &mut tx,
        lapso,
        fecha,
        "PRACTICA_EXTRA",
        "MANUAL",
        "SCHEDULED",
        "Extra",
        None,
    )
    .await?;
    assert_ne!(id, Uuid::nil());

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn events_name_not_blank_rechaza_nombre_en_blanco() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;
    let lapso = insertar_lapso(&mut tx, 4104, 1, "4104-01-01", "4104-12-20").await?;
    let fecha = NaiveDate::from_ymd_opt(4104, 5, 10).expect("fecha válida");

    // RF-12: event_name en blanco viola el CHECK.
    let r = intento!(
        tx,
        "INSERT INTO events (id_lapse, event_date, type_event, source, event_name)
         VALUES ($1, $2, 'PRACTICA_EXTRA', 'MANUAL', '   ')",
        lapso,
        fecha
    );
    let (codigo, restriccion) = sqlstate(&r.expect_err("nombre en blanco no es válido"));
    assert_eq!(codigo, "23514");
    assert_eq!(restriccion.as_deref(), Some("events_name_not_blank"));

    // Sensibilidad: un nombre real pasa.
    let id = insertar_evento(
        &mut tx,
        lapso,
        fecha,
        "PRACTICA_EXTRA",
        "MANUAL",
        "SCHEDULED",
        "Clase",
        None,
    )
    .await?;
    assert_ne!(id, Uuid::nil());

    tx.rollback().await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// events — triggers y FK
// ---------------------------------------------------------------------------

#[tokio::test]
async fn events_date_within_lapse_rechaza_auto_fuera_del_practice_day() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;
    let lapso = insertar_lapso(&mut tx, 4302, 1, "4302-01-01", "4302-12-20").await?;
    let jueves = primer_jueves(4302);
    let viernes = jueves.succ_opt().expect("fecha válida");

    // RF-13: un AUTO que no cae en el practice_day (THURSDAY) viola el trigger.
    let r = intento!(
        tx,
        "INSERT INTO events (id_lapse, event_date, type_event, source, event_name)
         VALUES ($1, $2, 'PRACTICA_REGULAR', 'AUTO', 'Auto')",
        lapso,
        viernes
    );
    let (codigo, _) = sqlstate(&r.expect_err("AUTO debe caer en practice_day"));
    assert_eq!(codigo, "23514");

    // Sensibilidad: el mismo AUTO en el jueves (borde del practice_day) pasa.
    let id = insertar_evento(
        &mut tx,
        lapso,
        jueves,
        "PRACTICA_REGULAR",
        "AUTO",
        "SCHEDULED",
        "Auto",
        None,
    )
    .await?;
    assert_ne!(id, Uuid::nil());

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn events_lapse_open_rechaza_insert_en_lapso_cerrado() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;
    let lapso = insertar_lapso(&mut tx, 4303, 1, "4303-01-01", "4303-12-20").await?;
    let fecha = NaiveDate::from_ymd_opt(4303, 6, 3).expect("fecha válida");
    sqlx::query("UPDATE lapses SET status = 'CLOSED' WHERE id = $1")
        .bind(lapso)
        .execute(tx.as_mut())
        .await?;

    // RF-14: INSERT en un lapso CLOSED viola trg_events_lapse_open.
    let r = intento!(
        tx,
        "INSERT INTO events (id_lapse, event_date, type_event, source, event_name)
         VALUES ($1, $2, 'PRACTICA_EXTRA', 'MANUAL', 'Extra')",
        lapso,
        fecha
    );
    let (codigo, _) = sqlstate(&r.expect_err("un lapso CLOSED no admite eventos nuevos"));
    assert_eq!(codigo, "23514");

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn events_frozen_by_attendance_congela_lo_estructural_y_deja_lo_demas() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;
    let lapso = insertar_lapso(&mut tx, 4304, 1, "4304-01-01", "4304-12-20").await?;
    let jueves = primer_jueves(4304);
    let evento = insertar_evento(
        &mut tx,
        lapso,
        jueves,
        "PRACTICA_EXTRA",
        "MANUAL",
        "SCHEDULED",
        "Clase",
        None,
    )
    .await?;
    montar_asistencia(&mut tx, evento, 4304, "Z4304").await?;

    // RF-16: el evento ya ocurrió (tiene asistencia) y queda congelado.
    let otro_dia = jueves
        .checked_add_days(chrono::Days::new(7))
        .expect("fecha");
    for (sql, etiqueta) in [
        ("UPDATE events SET event_date = $2 WHERE id = $1", "event_date"),
        (
            "UPDATE events SET type_event = 'PRACTICA_REGULAR'::event_type WHERE id = $1",
            "type_event",
        ),
        (
            "UPDATE events SET source = 'AUTO'::event_source WHERE id = $1",
            "source",
        ),
        (
            "UPDATE events SET status = 'CANCELLED'::event_status, cancelled_reason = 'x' WHERE id = $1",
            "status a CANCELLED",
        ),
    ] {
        let r = if etiqueta == "event_date" {
            intento!(tx, sql, evento, otro_dia)
        } else {
            intento!(tx, sql, evento)
        };
        let (codigo, _) = sqlstate(&r.expect_err("un evento ocurrido no se modifica"));
        assert_eq!(codigo, "23514", "cambio estructural: {etiqueta}");
    }

    // Sensibilidad: una columna no estructural (observación) sí se edita.
    sqlx::query("UPDATE events SET observation = 'nota' WHERE id = $1")
        .bind(evento)
        .execute(tx.as_mut())
        .await?;

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn events_no_delete_bloquea_borrar_aunque_no_haya_ocurrido() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;
    let lapso = insertar_lapso(&mut tx, 4305, 1, "4305-01-01", "4305-12-20").await?;
    let fecha = NaiveDate::from_ymd_opt(4305, 6, 3).expect("fecha válida");
    let evento = insertar_evento(
        &mut tx,
        lapso,
        fecha,
        "PRACTICA_EXTRA",
        "MANUAL",
        "SCHEDULED",
        "Extra",
        None,
    )
    .await?;

    // RF-17: regla 8, nada se borra físicamente.
    let r = intento!(tx, "DELETE FROM events WHERE id = $1", evento);
    let (codigo, _) = sqlstate(&r.expect_err("un evento nunca se borra"));
    assert_eq!(codigo, "23514");

    // Sensibilidad: la fila sigue ahí tras el intento fallido.
    let filas: i64 = sqlx::query_scalar("SELECT count(*) FROM events WHERE id = $1")
        .bind(evento)
        .fetch_one(tx.as_mut())
        .await?;
    assert_eq!(filas, 1, "el evento no desapareció");

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn events_id_lapse_fkey_rechaza_lapso_inexistente() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;
    let fecha = NaiveDate::from_ymd_opt(4306, 6, 3).expect("fecha válida");

    // RF-18: id_lapse que no existe viola la FK.
    let r = intento!(
        tx,
        "INSERT INTO events (id_lapse, event_date, type_event, source, event_name)
         VALUES ($1, $2, 'PRACTICA_EXTRA', 'MANUAL', 'Extra')",
        Uuid::new_v4(),
        fecha
    );
    let (codigo, restriccion) = sqlstate(&r.expect_err("lapso inexistente"));
    assert_eq!(codigo, "23503");
    assert_eq!(restriccion.as_deref(), Some("events_id_lapse_fkey"));

    tx.rollback().await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// attendance
// ---------------------------------------------------------------------------

#[tokio::test]
async fn pk_attendance_rechaza_el_mismo_par_evento_estudiante() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;
    let lapso = insertar_lapso(&mut tx, 4401, 1, "4401-01-01", "4401-12-20").await?;
    let fecha = primer_jueves(4401);
    let evento = insertar_evento(
        &mut tx,
        lapso,
        fecha,
        "PRACTICA_EXTRA",
        "MANUAL",
        "SCHEDULED",
        "Extra",
        None,
    )
    .await?;
    let estudiante = montar_asistencia(&mut tx, evento, 4401, "Z4401").await?;

    // RF-23: repetir el par (id_event, id_student) viola la PK.
    let r = intento!(
        tx,
        "INSERT INTO attendance (id_event, id_student, state) VALUES ($1, $2, 'AUSENTE')",
        evento,
        estudiante
    );
    let (codigo, restriccion) =
        sqlstate(&r.expect_err("no puede haber dos filas para el mismo par"));
    assert_eq!(codigo, "23505");
    assert_eq!(restriccion.as_deref(), Some("pk_attendance"));

    // Sensibilidad: otro estudiante en el mismo evento sí se registra.
    let otro: Uuid = sqlx::query_scalar(
        "INSERT INTO students (id_section, first_name, last_name)
         SELECT id_section, 'Otro', 'Alumno' FROM students WHERE id = $1
         RETURNING id",
    )
    .bind(estudiante)
    .fetch_one(tx.as_mut())
    .await?;
    let ok = intento!(
        tx,
        "INSERT INTO attendance (id_event, id_student, state) VALUES ($1, $2, 'AUSENTE')",
        evento,
        otro
    );
    assert!(ok.is_ok());

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn attendance_fks_bloquean_borrar_estudiante_con_asistencia() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;
    let lapso = insertar_lapso(&mut tx, 4402, 1, "4402-01-01", "4402-12-20").await?;
    let fecha = primer_jueves(4402);
    let evento = insertar_evento(
        &mut tx,
        lapso,
        fecha,
        "PRACTICA_EXTRA",
        "MANUAL",
        "SCHEDULED",
        "Extra",
        None,
    )
    .await?;
    let estudiante = montar_asistencia(&mut tx, evento, 4402, "Z4402").await?;

    // RF-24: FK RESTRICT es la segunda capa de "nada se borra" (los estudiantes
    // no tienen trigger de borrado; los eventos sí, y su FK queda tapada por
    // `trg_events_no_delete`, ya probado en T3).
    let r = intento!(tx, "DELETE FROM students WHERE id = $1", estudiante);
    let (codigo, restriccion) = sqlstate(&r.expect_err("un estudiante con asistencia no se borra"));
    assert_eq!(codigo, "23001");
    assert_eq!(restriccion.as_deref(), Some("attendance_id_student_fkey"));

    tx.rollback().await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// students
// ---------------------------------------------------------------------------

#[tokio::test]
async fn students_section_immutable_rechaza_con_asistencia_y_permite_sin_ella() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;
    let lapso = insertar_lapso(&mut tx, 4501, 1, "4501-01-01", "4501-12-20").await?;
    let fecha = primer_jueves(4501);
    let evento = insertar_evento(
        &mut tx,
        lapso,
        fecha,
        "PRACTICA_EXTRA",
        "MANUAL",
        "SCHEDULED",
        "Extra",
        None,
    )
    .await?;
    let con_asistencia = montar_asistencia(&mut tx, evento, 4501, "Z4501").await?;

    let otra_seccion: Uuid = sqlx::query_scalar(
        "INSERT INTO sections (grade_level, section, shift)
         VALUES ('4501', 'Z4501B', 'MATUTINO'::shift_type) RETURNING id",
    )
    .fetch_one(tx.as_mut())
    .await?;

    // RF-26: regla 3, un estudiante con asistencia no cambia de sección.
    let r = intento!(
        tx,
        "UPDATE students SET id_section = $2 WHERE id = $1",
        con_asistencia,
        otra_seccion
    );
    let (codigo, restriccion) = sqlstate(&r.expect_err("con asistencia, la sección es inmutable"));
    assert_eq!(codigo, "23514");
    // Un `RAISE EXCEPTION` de trigger no setea el nombre de la restricción.
    assert_eq!(
        restriccion, None,
        "el rechazo viene del trigger, no de un CHECK"
    );

    // Sensibilidad: sin asistencia, el cambio de sección pasa.
    let sin_asistencia: Uuid = sqlx::query_scalar(
        "INSERT INTO students (id_section, first_name, last_name)
         SELECT id_section, 'Sin', 'Asistencia' FROM students WHERE id = $1
         RETURNING id",
    )
    .bind(con_asistencia)
    .fetch_one(tx.as_mut())
    .await?;
    let ok = intento!(
        tx,
        "UPDATE students SET id_section = $2 WHERE id = $1",
        sin_asistencia,
        otra_seccion
    );
    assert!(
        ok.is_ok(),
        "sin asistencia, el estudiante puede moverse de sección"
    );

    tx.rollback().await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// grade_overrides
// ---------------------------------------------------------------------------

#[tokio::test]
async fn grade_overrides_value_range_rechaza_fuera_de_0_20_y_acepta_los_bordes() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;
    let lapso = insertar_lapso(&mut tx, 4601, 1, "4601-01-01", "4601-12-20").await?;
    let (_sec, est) = insertar_estudiante(&mut tx, 4601, "Z4601").await?;

    // RF-30: valor fuera de 0..=20 viola el CHECK.
    for valor in ["21", "-1"] {
        let r = intento!(
            tx,
            "INSERT INTO grade_overrides (id_student, id_lapse, value) VALUES ($1, $2, $3::numeric)",
            est,
            lapso,
            valor
        );
        let (codigo, restriccion) =
            sqlstate(&r.expect_err("un valor fuera de 0..=20 debe violar el CHECK"));
        assert_eq!(codigo, "23514", "valor {valor}");
        assert_eq!(restriccion.as_deref(), Some("grade_overrides_value_range"));
    }

    // Sensibilidad: 0 y 20 son válidos (estudiantes distintos, mismo lapso).
    let (_s2, est2) = insertar_estudiante(&mut tx, 4601, "Z4601B").await?;
    assert!(insertar_override(&mut tx, est, lapso, "0").await.is_ok());
    assert!(insertar_override(&mut tx, est2, lapso, "20").await.is_ok());

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn grade_overrides_unique_rechaza_el_mismo_estudiante_y_lapso() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;
    let lapso = insertar_lapso(&mut tx, 4602, 1, "4602-01-01", "4602-12-20").await?;
    let (_sec, est) = insertar_estudiante(&mut tx, 4602, "Z4602").await?;
    insertar_override(&mut tx, est, lapso, "10").await?;

    // RF-31: repetir (id_student, id_lapse) viola el único.
    let r = intento!(
        tx,
        "INSERT INTO grade_overrides (id_student, id_lapse, value) VALUES ($1, $2, 12)",
        est,
        lapso
    );
    let (codigo, restriccion) = sqlstate(&r.expect_err("un solo override por estudiante y lapso"));
    assert_eq!(codigo, "23505");
    assert_eq!(
        restriccion.as_deref(),
        Some("uq_grade_overrides_student_lapse")
    );

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn grade_overrides_lapse_open_rechaza_en_lapso_cerrado() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;
    let cerrado = insertar_lapso(&mut tx, 4603, 1, "4603-01-01", "4603-12-20").await?;
    let (_sec, est) = insertar_estudiante(&mut tx, 4603, "Z4603").await?;
    sqlx::query("UPDATE lapses SET status = 'CLOSED' WHERE id = $1")
        .bind(cerrado)
        .execute(tx.as_mut())
        .await?;

    // RF-32: INSERT de override en un lapso CLOSED viola el trigger.
    let r = intento!(
        tx,
        "INSERT INTO grade_overrides (id_student, id_lapse, value) VALUES ($1, $2, 10)",
        est,
        cerrado
    );
    let (codigo, restriccion) = sqlstate(&r.expect_err("un lapso CLOSED no admite overrides"));
    assert_eq!(codigo, "23514");
    assert_eq!(
        restriccion, None,
        "el rechazo viene del trigger, no de un CHECK"
    );

    // Sensibilidad: en un lapso abierto, el mismo override pasa.
    let abierto = insertar_lapso(&mut tx, 4604, 1, "4604-01-01", "4604-12-20").await?;
    let (_s2, est2) = insertar_estudiante(&mut tx, 4604, "Z4604").await?;
    assert!(insertar_override(&mut tx, est2, abierto, "10")
        .await
        .is_ok());

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn grade_overrides_fks_rechazan_referencias_inexistentes_y_borrado() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;
    let lapso = insertar_lapso(&mut tx, 4605, 1, "4605-01-01", "4605-12-20").await?;
    let (_sec, est) = insertar_estudiante(&mut tx, 4605, "Z4605").await?;

    // RF-33: referencias inexistentes.
    let sin_lapso = intento!(
        tx,
        "INSERT INTO grade_overrides (id_student, id_lapse, value) VALUES ($1, $2, 10)",
        est,
        Uuid::new_v4()
    );
    let (codigo, restriccion) = sqlstate(&sin_lapso.expect_err("lapso inexistente"));
    assert_eq!(codigo, "23503");
    assert_eq!(
        restriccion.as_deref(),
        Some("grade_overrides_id_lapse_fkey")
    );

    let sin_estudiante = intento!(
        tx,
        "INSERT INTO grade_overrides (id_student, id_lapse, value) VALUES ($1, $2, 10)",
        Uuid::new_v4(),
        lapso
    );
    let (codigo, restriccion) = sqlstate(&sin_estudiante.expect_err("estudiante inexistente"));
    assert_eq!(codigo, "23503");
    assert_eq!(
        restriccion.as_deref(),
        Some("grade_overrides_id_student_fkey")
    );

    // Sensibilidad + RF-33: con un override real, borrar al estudiante queda
    // bloqueado por la FK RESTRICT. (El borrado del lapso lo aborta primero
    // `lapses_delete_guard`, que cuenta overrides → 23514.)
    insertar_override(&mut tx, est, lapso, "10").await?;
    let r = intento!(tx, "DELETE FROM students WHERE id = $1", est);
    let (codigo, restriccion) = sqlstate(&r.expect_err("un estudiante con override no se borra"));
    assert_eq!(codigo, "23001");
    assert_eq!(
        restriccion.as_deref(),
        Some("grade_overrides_id_student_fkey")
    );

    tx.rollback().await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// audit_logs y funciones auxiliares
// ---------------------------------------------------------------------------

#[tokio::test]
async fn audit_logs_append_only_rechaza_update_y_delete_y_acepta_insert() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    // RF-34: append permitido.
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO audit_logs (entity, entity_id, action) VALUES ('x', $1, 'crear') RETURNING id",
    )
    .bind(Uuid::new_v4())
    .fetch_one(tx.as_mut())
    .await?;
    assert!(id > 0);

    // UPDATE y DELETE prohibidos (regla 6).
    let upd = intento!(
        tx,
        "UPDATE audit_logs SET action = 'editar' WHERE id = $1",
        id
    );
    let (codigo, restriccion) = sqlstate(&upd.expect_err("audit_logs no se edita"));
    assert_eq!(codigo, "23514");
    assert_eq!(restriccion, None, "el rechazo viene del trigger");

    let del = intento!(tx, "DELETE FROM audit_logs WHERE id = $1", id);
    let (codigo, _) = sqlstate(&del.expect_err("audit_logs no se borra"));
    assert_eq!(codigo, "23514");

    // Sensibilidad: la fila sigue intacta tras los intentos fallidos.
    let accion: String = sqlx::query_scalar("SELECT action FROM audit_logs WHERE id = $1")
        .bind(id)
        .fetch_one(tx.as_mut())
        .await?;
    assert_eq!(accion, "crear");

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn set_updated_at_pisa_el_valor_del_cliente_con_now() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;
    let lapso = insertar_lapso(&mut tx, 4701, 1, "4701-01-01", "4701-12-20").await?;

    // RF-35: el cliente intenta fijar updated_at, el trigger lo pisa.
    sqlx::query("UPDATE lapses SET updated_at = '2000-01-01T00:00:00Z' WHERE id = $1")
        .bind(lapso)
        .execute(tx.as_mut())
        .await?;
    let actual: DateTime<Utc> = sqlx::query_scalar("SELECT updated_at FROM lapses WHERE id = $1")
        .bind(lapso)
        .fetch_one(tx.as_mut())
        .await?;
    assert!(
        actual.year() > 2020,
        "updated_at lo pone el servidor (now()), no el cliente: {actual}"
    );

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn event_occurred_depende_de_la_evidencia_no_de_la_fecha() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;
    let lapso = insertar_lapso(&mut tx, 4702, 1, "4702-01-01", "4702-12-20").await?;
    let fecha = primer_jueves(4702);
    let evento = insertar_evento(
        &mut tx,
        lapso,
        fecha,
        "PRACTICA_EXTRA",
        "MANUAL",
        "SCHEDULED",
        "Extra",
        None,
    )
    .await?;

    // RF-36: sin asistencia, no ocurrió.
    let antes: bool = sqlx::query_scalar("SELECT event_occurred($1)")
        .bind(evento)
        .fetch_one(tx.as_mut())
        .await?;
    assert!(!antes, "sin evidencia, el evento no ocurrió");

    // Con una fila de asistencia, ocurrió (regla 5).
    montar_asistencia(&mut tx, evento, 4702, "Z4702").await?;
    let despues: bool = sqlx::query_scalar("SELECT event_occurred($1)")
        .bind(evento)
        .fetch_one(tx.as_mut())
        .await?;
    assert!(despues, "con evidencia, el evento ocurrió");

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn week_day_of_mapea_los_siete_dias() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    // RF-37: 2024-01-01 es lunes; los siete días ISO en orden.
    let esperado = [
        "MONDAY",
        "TUESDAY",
        "WEDNESDAY",
        "THURSDAY",
        "FRIDAY",
        "SATURDAY",
        "SUNDAY",
    ];
    for (i, dia) in esperado.iter().enumerate() {
        let offset = i as i64;
        let fecha = NaiveDate::from_ymd_opt(2024, 1, 1)
            .expect("fecha válida")
            .checked_add_days(chrono::Days::new(offset as u64))
            .expect("fecha válida");
        let obtenido: String = sqlx::query_scalar("SELECT week_day_of($1)::text")
            .bind(fecha)
            .fetch_one(tx.as_mut())
            .await?;
        assert_eq!(&obtenido, dia, "week_day_of({fecha})");
    }

    tx.rollback().await?;
    Ok(())
}
