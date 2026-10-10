//! `From<sqlx::Error>` contra errores que Postgres produce de verdad.
//!
//! Los tests unitarios de `shared::error` pasan códigos y nombres escritos a
//! mano; acá se comprueba que la cadena completa —restricción violada →
//! `sqlx::Error` → `AppError` → status HTTP— funciona con el error real.
//!
//! Cada violación vive en su transacción: Postgres deja la transacción
//! abortada al primer error y cualquier sentencia posterior falla.

mod common;

use chrono::{Datelike, Weekday};
use civik_backend::shared::error::AppError;

type Resultado = Result<(), Box<dyn std::error::Error>>;
#[tokio::test]
async fn eliminar_lapso_cerrado_emite_check_violation_23514() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let id = sqlx::query_scalar::<_, uuid::Uuid>(
        "INSERT INTO lapses (school_year, lapse_number, start_date, end_date, practice_day, status)
         VALUES (3001, 1, '3001-02-01', '3001-12-20', 'THURSDAY', 'PLANNED')
         RETURNING id",
    )
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query("UPDATE lapses SET status = 'ACTIVE' WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE lapses SET status = 'CLOSED' WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    sqlx::query("SAVEPOINT intento_eliminar")
        .execute(&mut *tx)
        .await?;
    let resultado = sqlx::query("DELETE FROM lapses WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await;

    let Err(error) = resultado else {
        panic!("el guardarraíl `lapses_delete_guard` debe rechazar borrar un CLOSED");
    };
    if let sqlx::Error::Database(base) = &error {
        assert_eq!(base.code().as_deref(), Some("23514"));
    } else {
        panic!("esperaba error DB, llegó: {error}");
    }

    sqlx::query("ROLLBACK TO SAVEPOINT intento_eliminar")
        .execute(&mut *tx)
        .await?;

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn eliminar_lapso_terminado_emite_check_violation_23514() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let id = sqlx::query_scalar::<_, uuid::Uuid>(
        "INSERT INTO lapses (school_year, lapse_number, start_date, end_date, practice_day, status)
         VALUES (2020, 1, '2020-01-03', '2020-03-27', 'FRIDAY', 'PLANNED')
         RETURNING id",
    )
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query("SAVEPOINT intento_eliminar")
        .execute(&mut *tx)
        .await?;
    let resultado = sqlx::query("DELETE FROM lapses WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await;

    let Err(error) = resultado else {
        panic!("el guardarraíl debe rechazar borrar un lapso terminado (fin < hoy)");
    };
    if let sqlx::Error::Database(base) = &error {
        assert_eq!(base.code().as_deref(), Some("23514"));
    } else {
        panic!("esperaba error DB, llegó: {error}");
    }

    sqlx::query("ROLLBACK TO SAVEPOINT intento_eliminar")
        .execute(&mut *tx)
        .await?;

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn activar_segundo_lapso_emite_unicidad_23505() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let a = sqlx::query_scalar::<_, uuid::Uuid>(
        "INSERT INTO lapses (school_year, lapse_number, start_date, end_date, practice_day)
         VALUES (3002, 1, '3002-01-05', '3002-03-27', 'THURSDAY')
         RETURNING id",
    )
    .fetch_one(&mut *tx)
    .await?;
    let b = sqlx::query_scalar::<_, uuid::Uuid>(
        "INSERT INTO lapses (school_year, lapse_number, start_date, end_date, practice_day)
         VALUES (3003, 1, '3003-01-05', '3003-03-27', 'THURSDAY')
         RETURNING id",
    )
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query("UPDATE lapses SET status = 'ACTIVE' WHERE id = $1")
        .bind(a)
        .execute(&mut *tx)
        .await?;

    sqlx::query("SAVEPOINT intento_activar")
        .execute(&mut *tx)
        .await?;
    let resultado = sqlx::query("UPDATE lapses SET status = 'ACTIVE' WHERE id = $1")
        .bind(b)
        .execute(&mut *tx)
        .await;

    let Err(error) = resultado else {
        panic!("el índice parcial `uq_lapses_single_active` debe violarse al activar el segundo");
    };
    if let sqlx::Error::Database(base) = &error {
        assert_eq!(base.code().as_deref(), Some("23505"));
        assert_eq!(base.constraint(), Some("uq_lapses_single_active"));
    } else {
        panic!("esperaba error DB, llegó: {error}");
    }

    sqlx::query("ROLLBACK TO SAVEPOINT intento_activar")
        .execute(&mut *tx)
        .await?;

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn solape_de_lapsos_emite_exclusion_violation_23p01() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    sqlx::query(
        "INSERT INTO lapses (school_year, lapse_number, start_date, end_date, practice_day)
         VALUES (3004, 1, '3004-01-05', '3004-03-27', 'THURSDAY')",
    )
    .execute(&mut *tx)
    .await?;

    sqlx::query("SAVEPOINT intento_solape")
        .execute(&mut *tx)
        .await?;
    let resultado = sqlx::query(
        "INSERT INTO lapses (school_year, lapse_number, start_date, end_date, practice_day)
         VALUES (3004, 2, '3004-02-01', '3004-03-31', 'FRIDAY')",
    )
    .execute(&mut *tx)
    .await;

    let Err(error) = resultado else {
        panic!("el EXCLUDE `lapses_no_overlap` debe detectar el solape con 23P01");
    };
    if let sqlx::Error::Database(base) = &error {
        assert_eq!(base.code().as_deref(), Some("23P01"));
        assert_eq!(base.constraint(), Some("lapses_no_overlap"));
    } else {
        panic!("esperaba error DB, llegó: {error}");
    }

    sqlx::query("ROLLBACK TO SAVEPOINT intento_solape")
        .execute(&mut *tx)
        .await?;

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn mover_fechas_vs_evento_emite_check_violation_23514() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let id = sqlx::query_scalar::<_, uuid::Uuid>(
        "INSERT INTO lapses (school_year, lapse_number, start_date, end_date, practice_day)
         VALUES (3005, 1, '3005-01-07', '3005-12-16', 'THURSDAY')
         RETURNING id",
    )
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO events (id_lapse, event_date, type_event, source, event_name)
         VALUES ($1, '3005-09-15', 'PRACTICA_REGULAR', 'MANUAL', 'Clase')",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;

    sqlx::query("SAVEPOINT intento_mover")
        .execute(&mut *tx)
        .await?;
    let resultado = sqlx::query("UPDATE lapses SET end_date = '3005-08-31' WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await;

    let Err(error) = resultado else {
        panic!("`trg_lapses_dates_against_events` debe rechazar mover fechas dejando eventos fuera (23514)");
    };
    if let sqlx::Error::Database(base) = &error {
        assert_eq!(base.code().as_deref(), Some("23514"));
    } else {
        panic!("esperaba error DB, llegó: {error}");
    }

    sqlx::query("ROLLBACK TO SAVEPOINT intento_mover")
        .execute(&mut *tx)
        .await?;

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn eliminar_lapso_con_evento_emite_restrict_23001() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let id = sqlx::query_scalar::<_, uuid::Uuid>(
        "INSERT INTO lapses (school_year, lapse_number, start_date, end_date, practice_day)
         VALUES (3006, 1, '3006-01-06', '3006-03-26', 'THURSDAY')
         RETURNING id",
    )
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO events (id_lapse, event_date, type_event, source, event_name)
         VALUES ($1, '3006-02-15', 'PRACTICA_REGULAR', 'MANUAL', 'Clase')",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;

    sqlx::query("SAVEPOINT intento_eliminar")
        .execute(&mut *tx)
        .await?;
    let resultado = sqlx::query("DELETE FROM lapses WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await;

    let Err(error) = resultado else {
        panic!("la FK `events_id_lapse_fkey` (RESTRICT) debe bloquear el DELETE con 23001 si el guardaraíl fallara");
    };
    // Nota: con el guardarraíl `lapses_delete_guard` contamos eventos y abortamos
    // primero (23514). Si ese guard fuera removido o cambiara, la FK sería el
    // último escollo: por eso lo dejamos explícito para documentar la política
    // RESTRICT.
    if let sqlx::Error::Database(base) = &error {
        assert!(matches!(
            base.code().as_deref(),
            Some("23514") | Some("23001")
        ));
    } else {
        panic!("esperaba error DB, llegó: {error}");
    }

    sqlx::query("ROLLBACK TO SAVEPOINT intento_eliminar")
        .execute(&mut *tx)
        .await?;

    tx.rollback().await?;
    Ok(())
}
#[tokio::test]
async fn evento_fuera_de_rango_lapso_emite_check_violation_23514() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;
    let anio = 2070;
    let lapso = civik_backend::modules::lapses::queries::crear(
        &mut tx,
        civik_backend::modules::lapses::domain::preparar_entrada(
            &anio.to_string(),
            "1",
            &format!("{anio}-09-01"),
            &format!("{anio}-12-20"),
            "THURSDAY",
        )
        .unwrap(),
    )
    .await?;
    let res = civik_backend::modules::events::queries::crear_manual(
        tx.as_mut(),
        lapso.id,
        chrono::NaiveDate::from_ymd_opt(anio, 8, 1).unwrap(),
        "PRACTICA_EXTRA",
        "Extra",
    )
    .await;
    assert!(
        matches!(res, Err(sqlx::Error::Database(db_err)) if db_err.code().as_deref() == Some("23514"))
    );
    Ok(())
}

#[tokio::test]
async fn dos_auto_el_mismo_dia_emiten_unicidad_23505() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let anio = 2097;
    let lapso = civik_backend::modules::lapses::queries::crear(
        &mut tx,
        civik_backend::modules::lapses::domain::preparar_entrada(
            &anio.to_string(),
            "1",
            &format!("{anio}-01-01"),
            &format!("{anio}-03-26"),
            "THURSDAY",
        )
        .unwrap(),
    )
    .await?;

    // Primer jueves del lapso (la regla 1 exige que un AUTO caiga ahí).
    let mut jueves = chrono::NaiveDate::from_ymd_opt(anio, 1, 1).unwrap();
    while jueves.weekday() != Weekday::Thu {
        jueves = jueves.succ_opt().unwrap();
    }

    sqlx::query(
        "INSERT INTO events (id_lapse, event_date, type_event, source, event_name)
         VALUES ($1, $2, 'PRACTICA_REGULAR', 'AUTO', 'Primera')",
    )
    .bind(lapso.id)
    .bind(jueves)
    .execute(tx.as_mut())
    .await?;

    // El índice parcial `uq_events_auto_one_per_day` debe rechazar el segundo.
    let resultado = sqlx::query(
        "INSERT INTO events (id_lapse, event_date, type_event, source, event_name)
         VALUES ($1, $2, 'PRACTICA_REGULAR', 'AUTO', 'Segunda')",
    )
    .bind(lapso.id)
    .bind(jueves)
    .execute(tx.as_mut())
    .await;

    let Err(error) = resultado else {
        panic!("dos AUTO el mismo día deben violar `uq_events_auto_one_per_day`");
    };
    if let sqlx::Error::Database(base) = &error {
        assert_eq!(base.code().as_deref(), Some("23505"));
        assert_eq!(base.constraint(), Some("uq_events_auto_one_per_day"));
    } else {
        panic!("esperaba error DB, llegó: {error}");
    }

    // La cadena completa: el error real se traduce a 409, no a 500.
    assert!(matches!(AppError::from(error), AppError::Conflict(_)));

    tx.rollback().await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Asistencia: reglas 4, 7 y 10 (defensa de base, SQL directo)
// ---------------------------------------------------------------------------

/// El primer jueves de septiembre del año, para una práctica regular válida.
fn primer_jueves(anio: i32) -> chrono::NaiveDate {
    let mut dia = chrono::NaiveDate::from_ymd_opt(anio, 9, 1).unwrap();
    while dia.weekday() != Weekday::Thu {
        dia = dia.succ_opt().unwrap();
    }
    dia
}

/// Arma lapso (PLANNED) + sección + estudiante + evento regular dentro de la
/// transacción y devuelve sus ids.
async fn montar_evento(
    tx: &mut sqlx::PgTransaction<'_>,
    anio: i32,
    seccion: &str,
) -> Result<(uuid::Uuid, uuid::Uuid, uuid::Uuid, uuid::Uuid), Box<dyn std::error::Error>> {
    use civik_backend::modules::events::queries as events_q;
    use civik_backend::modules::lapses::{domain::preparar_entrada, queries as lapses_q};
    use civik_backend::modules::sections::{
        domain::{DatosSeccion, Grado, Turno},
        queries as sections_q,
    };
    use civik_backend::modules::students::{domain::DatosEstudiante, queries as students_q};

    let lap = lapses_q::crear(
        tx,
        preparar_entrada(
            &anio.to_string(),
            "1",
            &format!("{anio}-09-01"),
            &format!("{anio}-12-20"),
            "THURSDAY",
        )?,
    )
    .await?;
    let sec = sections_q::crear(
        tx,
        DatosSeccion {
            grado: Grado::Primero,
            seccion: seccion.into(),
            turno: Turno::Matutino,
        },
    )
    .await?;
    let est = students_q::crear(
        tx,
        DatosEstudiante {
            id_seccion: sec.id,
            nombre: "N".into(),
            apellido: "A".into(),
            telefono: None,
        },
    )
    .await?;
    let ev = events_q::crear_manual(
        tx.as_mut(),
        lap.id,
        primer_jueves(anio),
        "PRACTICA_REGULAR",
        "Clase",
    )
    .await?;
    Ok((lap.id, sec.id, est.id, ev.id))
}

#[tokio::test]
async fn asistencia_en_evento_cancelado_emite_check_violation_23514() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let (_lap, _sec, est, ev) = montar_evento(&mut tx, 2095, "B1").await?;
    sqlx::query("UPDATE events SET status = 'CANCELLED', cancelled_reason = 'x' WHERE id = $1")
        .bind(ev)
        .execute(tx.as_mut())
        .await?;

    sqlx::query("SAVEPOINT intento_asistencia")
        .execute(tx.as_mut())
        .await?;
    let resultado = sqlx::query(
        "INSERT INTO attendance (id_event, id_student, state) VALUES ($1, $2, 'PRESENTE')",
    )
    .bind(ev)
    .bind(est)
    .execute(tx.as_mut())
    .await;

    let Err(error) = resultado else {
        panic!("la regla 7 (`check_attendance_guards`) debe rechazar asistencia en CANCELLED");
    };
    if let sqlx::Error::Database(base) = &error {
        assert_eq!(base.code().as_deref(), Some("23514"));
    } else {
        panic!("esperaba error DB, llegó: {error}");
    }

    sqlx::query("ROLLBACK TO SAVEPOINT intento_asistencia")
        .execute(tx.as_mut())
        .await?;
    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn asistencia_en_lapso_cerrado_emite_check_violation_23514() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let (lap, _sec, est, ev) = montar_evento(&mut tx, 2094, "B2").await?;
    sqlx::query("UPDATE lapses SET status = 'ACTIVE' WHERE id = $1")
        .bind(lap)
        .execute(tx.as_mut())
        .await?;
    sqlx::query("UPDATE lapses SET status = 'CLOSED' WHERE id = $1")
        .bind(lap)
        .execute(tx.as_mut())
        .await?;

    sqlx::query("SAVEPOINT intento_asistencia")
        .execute(tx.as_mut())
        .await?;
    let resultado = sqlx::query(
        "INSERT INTO attendance (id_event, id_student, state) VALUES ($1, $2, 'PRESENTE')",
    )
    .bind(ev)
    .bind(est)
    .execute(tx.as_mut())
    .await;

    let Err(error) = resultado else {
        panic!("la regla 4 (`assert_lapse_is_open`) debe rechazar asistencia en lapso CLOSED");
    };
    if let sqlx::Error::Database(base) = &error {
        assert_eq!(base.code().as_deref(), Some("23514"));
    } else {
        panic!("esperaba error DB, llegó: {error}");
    }

    sqlx::query("ROLLBACK TO SAVEPOINT intento_asistencia")
        .execute(tx.as_mut())
        .await?;
    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn borrar_asistencia_emite_check_violation_23514() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let (_lap, _sec, est, ev) = montar_evento(&mut tx, 2093, "B3").await?;
    sqlx::query("INSERT INTO attendance (id_event, id_student, state) VALUES ($1, $2, 'PRESENTE')")
        .bind(ev)
        .bind(est)
        .execute(tx.as_mut())
        .await?;

    sqlx::query("SAVEPOINT intento_borrar")
        .execute(tx.as_mut())
        .await?;
    let resultado = sqlx::query("DELETE FROM attendance WHERE id_event = $1")
        .bind(ev)
        .execute(tx.as_mut())
        .await;

    let Err(error) = resultado else {
        panic!("la regla 10 (`attendance_no_delete`) debe impedir borrar asistencia");
    };
    if let sqlx::Error::Database(base) = &error {
        assert_eq!(base.code().as_deref(), Some("23514"));
    } else {
        panic!("esperaba error DB, llegó: {error}");
    }

    sqlx::query("ROLLBACK TO SAVEPOINT intento_borrar")
        .execute(tx.as_mut())
        .await?;
    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn cambiar_identidad_de_asistencia_emite_check_violation_23514() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let (_lap, sec, est, ev) = montar_evento(&mut tx, 2092, "B4").await?;
    let otro = civik_backend::modules::students::queries::crear(
        &mut tx,
        civik_backend::modules::students::domain::DatosEstudiante {
            id_seccion: sec,
            nombre: "Otro".into(),
            apellido: "Alumno".into(),
            telefono: None,
        },
    )
    .await?;
    sqlx::query("INSERT INTO attendance (id_event, id_student, state) VALUES ($1, $2, 'PRESENTE')")
        .bind(ev)
        .bind(est)
        .execute(tx.as_mut())
        .await?;

    sqlx::query("SAVEPOINT intento_mover")
        .execute(tx.as_mut())
        .await?;
    let resultado = sqlx::query("UPDATE attendance SET id_student = $1 WHERE id_event = $2")
        .bind(otro.id)
        .bind(ev)
        .execute(tx.as_mut())
        .await;

    let Err(error) = resultado else {
        panic!("la regla 10 (`attendance_identity_immutable`) debe impedir cambiar de estudiante");
    };
    if let sqlx::Error::Database(base) = &error {
        assert_eq!(base.code().as_deref(), Some("23514"));
    } else {
        panic!("esperaba error DB, llegó: {error}");
    }

    sqlx::query("ROLLBACK TO SAVEPOINT intento_mover")
        .execute(tx.as_mut())
        .await?;
    tx.rollback().await?;
    Ok(())
}
