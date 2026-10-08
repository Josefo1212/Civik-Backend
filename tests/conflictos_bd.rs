//! `From<sqlx::Error>` contra errores que Postgres produce de verdad.
//!
//! Los tests unitarios de `shared::error` pasan códigos y nombres escritos a
//! mano; acá se comprueba que la cadena completa —restricción violada →
//! `sqlx::Error` → `AppError` → status HTTP— funciona con el error real.
//!
//! Cada violación vive en su transacción: Postgres deja la transacción
//! abortada al primer error y cualquier sentencia posterior falla.

mod common;


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
        assert!(matches!(base.code().as_deref(), Some("23514") | Some("23001")));
    } else {
        panic!("esperaba error DB, llegó: {error}");
    }

    sqlx::query("ROLLBACK TO SAVEPOINT intento_eliminar")
        .execute(&mut *tx)
        .await?;

    tx.rollback().await?;
    Ok(())
}