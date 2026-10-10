//! T6 — la integración del módulo `lapses` contra `civik_dev`, con el patrón
//! transacción/rollback (ver skill `testing-transaccional`): cada test abre su
//! propia transacción, opera sobre filas que nunca llegan a confirmarse y la
//! descarta con `rollback`. La base queda exactamente como estaba.
//!
//! Estos tests ejercitan las capas `domain` + `queries` + triggers de la base
//! (los mismos bloques que arma `service.rs`), porque el `service` abre y
//! **confirma** su propia transacción sobre el pool y eso sería residuo. El
//! camino de HTTP completo (404 vs 409 sobre el service) vive en
//! `http_lapses.rs` (T7), y los triggers de defensa aparte, con SQL directo,
//! en `conflictos_bd.rs` (T8).
//!
//! Mapa RF → test (plan §9), con los nombres literales del plan:
//! - RF-1  `el_alta_persiste_la_fila_en_estado_planeado`
//! - RF-2  unit tests de `domain.rs` (sin base) + 400 en `http_lapses.rs`
//! - RF-3  `el_alta_duplicada_en_mismo_anio_y_numero_falla`
//! - RF-4  `el_alta_solapada_falla`
//! - RF-5  `listar_ordena_por_anio_descendente_y_numero_ascendente`
//! - RF-6  `consultar_devuelve_el_lapso_con_su_estado`
//! - RF-7  `consultar_inexistente_no_devuelve_fila`
//! - RF-8  `la_edicion_valida_igual_que_el_alta_y_no_toca_el_estado`
//! - RF-9  `editar_un_lapso_cerrado_se_rechaza` (trigger `trg_lapses_closed_frozen`)
//! - RF-10 `editar_hacia_un_anio_y_numero_ajenos_falla` + `editar_hacia_solape_falla`
//! - RF-11 `mover_fechas_dejando_un_evento_fuera_se_rechaza` (pre-chequeo del
//!   service + trigger `trg_lapses_dates_against_events` como defensa)
//! - RF-12 `la_activacion_pasa_un_lapso_planeado_a_activo`
//! - RF-13 `la_segunda_activacion_con_otro_lapso_activo_falla`
//! - RF-14 `activar_un_lapso_no_planeado_falla` (sin cambio y sin auditoría)
//! - RF-15 `el_cierre_pasa_un_lapso_activo_a_cerrado_e_inmutable`
//! - RF-16 `cerrar_un_lapso_no_activo_falla` (sin cambio y sin auditoría)
//! - RF-17 `eliminar_un_lapso_vacio_lo_borra_y_deja_auditoria`
//! - RF-18 `eliminar_un_lapso_cerrado_se_rechaza`,
//!   `eliminar_un_lapso_terminado_se_rechaza`,
//!   `eliminar_un_lapso_con_evento_se_rechaza`
//! - RF-19 `el_alta_deja_su_entrada_en_el_historial`,
//!   `la_activacion_y_el_cierre_dejan_su_auditoria`,
//!   `el_borrado_deja_su_auditoria_antes_de_desaparecer`
//!
//! Las fechas son arbitrarias (no hay ningún lapso real en `civik_dev` ni
//! `seed`). Cada test usa tramos de fechas ENTERAMENTE AJENOS a los demás:
//! el EXCLUDE `lapses_no_overlap` es global y los tests corren en paralelo;
//! dos tramos vivos que se solapen entre transacciones distintas se
//! enzarzan en predicate locks del índice GiST y ABORTAN con deadlock 40P01
//! (pasó con el helper único de "2026-09-14..2026-12-18"). Solo dentro de un
//! mismo test el solape es intencional (y el segundo insert falla).
//!
//! Los intentos que dejan la transacción ABORTADA (trigger o RESTRICT) viven en
//! un `SAVEPOINT` que se deshace justo al volver, mismo patrón que `sections.rs`.

mod common;

use civik_backend::modules::lapses::domain::{preparar_entrada, DatosLapso};
use civik_backend::modules::lapses::queries::{
    activar, cerrar, crear, editar, eliminar, get_by_id, listar, Lapso,
};
use uuid::Uuid;

type Resultado = Result<(), Box<dyn std::error::Error>>;

/// Un lapso válido con su tramo de fechas, para que cada test aísle su rango.
fn entrada(
    anio: &str,
    numero: &str,
    inicio: &str,
    fin: &str,
) -> Result<DatosLapso, civik_backend::modules::lapses::domain::ErrorDominio> {
    preparar_entrada(anio, numero, inicio, fin, "THURSDAY")
}

/// Convierte un `sqlx::Error` en su (código, restricción) para los asserts de
/// defensa, o hace panic si el error no es de base de datos. Toma una
/// referencia para que el llamador pueda reutilizar el error (p. ej. pasarlo a
/// `AppError::from`) después de inspeccionarlo.
fn codigo_de(error: &sqlx::Error) -> (String, Option<String>) {
    match error {
        sqlx::Error::Database(base) => (
            base.code().map(|c| c.into_owned()).unwrap_or_default(),
            base.constraint().map(str::to_string),
        ),
        _ => panic!("esperaba un error de base de datos, llegó: {error}"),
    }
}

// --- RF-1: alta ---

#[tokio::test]
async fn el_alta_persiste_la_fila_en_estado_planeado() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let lapso = crear(&mut tx, entrada("2026", "1", "2026-01-12", "2026-03-27")?).await?;

    assert_ne!(lapso.id, Uuid::nil(), "RF-1: devuelve identificador");
    assert_eq!(lapso.status, "PLANNED", "RF-1: nace en estado planeado");
    assert_eq!(lapso.practice_day, "THURSDAY");
    assert_eq!(lapso.school_year, 2026);
    assert_eq!(lapso.lapse_number, 1);

    // Contraste contra la fila real, no solo contra lo que devuelve `crear`.
    let (anio, numero, estado, dia): (i32, i16, String, String) = sqlx::query_as(
        "SELECT school_year, lapse_number, status::text, practice_day::text
         FROM lapses WHERE id = $1",
    )
    .bind(lapso.id)
    .fetch_one(&mut *tx)
    .await?;

    assert_eq!(anio, 2026);
    assert_eq!(numero, 1);
    assert_eq!(estado, "PLANNED");
    assert_eq!(dia, "THURSDAY");

    tx.rollback().await?;
    Ok(())
}

// --- RF-3, RF-4: unicidad y solape ---

#[tokio::test]
async fn el_alta_duplicada_en_mismo_anio_y_numero_falla() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    crear(&mut tx, entrada("2027", "1", "2027-01-11", "2027-03-26")?).await?;

    let resultado = crear(&mut tx, entrada("2027", "1", "2027-01-11", "2027-03-26")?).await;

    let Err(error) = resultado else {
        panic!("la segunda alta del mismo (año, número) debe fallar (RF-3)");
    };

    let (codigo, restriccion) = codigo_de(&error);
    assert_eq!(codigo, "23505");
    assert_eq!(restriccion.as_deref(), Some("uq_lapses_year_number"));

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn el_alta_solapada_falla() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    crear(&mut tx, entrada("2028", "1", "2028-01-10", "2028-03-24")?).await?;

    // Solape parcial con el anterior, dentro del mismo año y del mismo test.
    // El EXCLUDE `daterange '[]' WITH &&` lo detecta.
    let resultado = crear(&mut tx, entrada("2028", "2", "2028-03-01", "2028-03-31")?).await;

    let Err(error) = resultado else {
        panic!("un lapso que solapa a otro debe fallar (RF-4)");
    };

    let (codigo, restriccion) = codigo_de(&error);
    assert_eq!(
        codigo, "23P01",
        "el EXCLUDE emite exclusion_violation, no 23505"
    );
    assert_eq!(restriccion.as_deref(), Some("lapses_no_overlap"));

    tx.rollback().await?;
    Ok(())
}

// --- RF-5, RF-6, RF-7: lecturas ---

#[tokio::test]
async fn listar_ordena_por_anio_descendente_y_numero_ascendente() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    // Se crean desordenadas a propósito: solo el ORDER BY puede estabilizarlas.
    // Los rangos no se solapan entre sí (EXCLUDE) y quedan en 2024/2025, lejos
    // de los años que usan los demás tests.
    let _l2024_n1 = crear(
        &mut tx,
        preparar_entrada("2024", "1", "2024-01-08", "2024-04-12", "MONDAY")?,
    )
    .await?;
    let _l2025_n2 = crear(
        &mut tx,
        preparar_entrada("2025", "2", "2025-08-01", "2025-11-14", "TUESDAY")?,
    )
    .await?;
    let _l2025_n1 = crear(
        &mut tx,
        preparar_entrada("2025", "1", "2025-01-13", "2025-04-30", "WEDNESDAY")?,
    )
    .await?;
    let _l2024_n3 = crear(
        &mut tx,
        preparar_entrada("2024", "3", "2024-05-06", "2024-07-19", "THURSDAY")?,
    )
    .await?;

    let lista: Vec<Lapso> = listar(&mut *tx).await?;

    let obtenido: Vec<(i32, i16)> = lista
        .iter()
        .map(|l| (l.school_year, l.lapse_number))
        .collect();
    // Año descendente, y dentro del mismo año número ascendente (RNF-3).
    let esperado = vec![(2025, 1), (2025, 2), (2024, 1), (2024, 3)];
    assert_eq!(obtenido, esperado, "RF-5: el orden debe ser el canónico");

    // RNF-3: dos listados seguidos producen exactamente el mismo orden.
    let segunda: Vec<Lapso> = listar(&mut *tx).await?;
    assert_eq!(segunda.len(), lista.len());
    assert_eq!(segunda[0].id, lista[0].id);
    assert_eq!(segunda[1].id, lista[1].id);
    assert_eq!(segunda[2].id, lista[2].id);

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn consultar_devuelve_el_lapso_con_su_estado() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let lapso = crear(&mut tx, entrada("2029", "2", "2029-01-08", "2029-03-29")?).await?;

    let consultada = get_by_id(&mut *tx, lapso.id).await?;
    assert_eq!(
        consultada.id, lapso.id,
        "RF-6: se consulta por identificador"
    );
    assert_eq!(consultada.status, "PLANNED");
    assert_eq!(consultada.practice_day, "THURSDAY");
    assert_eq!(consultada.start_date.to_string(), "2029-01-08");

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn consultar_inexistente_no_devuelve_fila() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let resultado = get_by_id(&mut *tx, Uuid::new_v4()).await;
    assert!(
        matches!(resultado, Err(sqlx::Error::RowNotFound)),
        "RF-7: un identificador inexistente es RowNotFound, llegó: {resultado:?}"
    );

    tx.rollback().await?;
    Ok(())
}

// --- RF-8, RF-9, RF-10, RF-11: edición ---

#[tokio::test]
async fn la_edicion_valida_igual_que_el_alta_y_no_toca_el_estado() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let original = crear(&mut tx, entrada("2030", "1", "2030-01-07", "2030-03-28")?).await?;

    // RF-8: la edición pasa por `preparar_entrada` (normaliza igual que el
    // alta) y no toca el estado: se edita mientras sigue PLANNED.
    let editada = editar(
        &mut tx,
        original.id,
        preparar_entrada(" 2040 ", " 2 ", "2040-01-05", "2040-03-31", " friday ")?,
    )
    .await?;

    assert_eq!(editada.id, original.id);
    assert_eq!(editada.school_year, 2040);
    assert_eq!(editada.lapse_number, 2);
    assert_eq!(editada.practice_day, "FRIDAY");
    assert_eq!(
        editada.status, "PLANNED",
        "RF-8: una edición estructural no cambia el estado"
    );

    let (estado, dia): (String, String) =
        sqlx::query_as("SELECT status::text, practice_day::text FROM lapses WHERE id = $1")
            .bind(original.id)
            .fetch_one(&mut *tx)
            .await?;
    assert_eq!(estado, "PLANNED");
    assert_eq!(dia, "FRIDAY");

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn editar_un_lapso_cerrado_se_rechaza() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let lapso = crear(&mut tx, entrada("2032", "1", "2032-01-06", "2032-03-26")?).await?;
    activar(&mut tx, lapso.id).await?;
    cerrar(&mut tx, lapso.id).await?;

    // RF-9: sobre un CLOSED la edición es ilegal. El `service` lo rechaza con
    // su `validar_transicion`; acá, a nivel de queries, la defensa de la base
    // (`trg_lapses_closed_frozen`) responde con 23514.
    sqlx::query("SAVEPOINT intento_editar")
        .execute(&mut *tx)
        .await?;
    let resultado = editar(
        &mut tx,
        lapso.id,
        entrada("2047", "2", "2047-01-06", "2047-03-27")?,
    )
    .await;

    let Err(error) = resultado else {
        panic!("editar un lapso cerrado debe fallar (RF-9)");
    };
    let (codigo, _) = codigo_de(&error);
    assert_eq!(
        codigo, "23514",
        "el trigger del CLOSED lo rechaza con check_violation"
    );

    sqlx::query("ROLLBACK TO SAVEPOINT intento_editar")
        .execute(&mut *tx)
        .await?;

    // La fila sigue intacta (no era CLOSED antes del intento y sigue CLOSED, con
    // sus fechas originales).
    let (estado, anio): (String, i32) =
        sqlx::query_as("SELECT status::text, school_year FROM lapses WHERE id = $1")
            .bind(lapso.id)
            .fetch_one(&mut *tx)
            .await?;
    assert_eq!(estado, "CLOSED");
    assert_eq!(anio, 2032, "la edición fallida no cambió los datos");

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn editar_hacia_un_anio_y_numero_ajenos_falla() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    crear(&mut tx, entrada("2033", "1", "2033-01-04", "2033-03-27")?).await?;
    let otra = crear(&mut tx, entrada("2033", "2", "2033-05-02", "2033-07-25")?).await?;

    // RF-10: editar `otra` hacia el (año, número) que ya ocupa la primera.
    let resultado = editar(
        &mut tx,
        otra.id,
        entrada("2033", "1", "2033-09-01", "2033-11-24")?,
    )
    .await;

    let Err(error) = resultado else {
        panic!("editar hacia un (año, número) ajenos debe fallar (RF-10)");
    };

    let (codigo, restriccion) = codigo_de(&error);
    assert_eq!(codigo, "23505");
    assert_eq!(restriccion.as_deref(), Some("uq_lapses_year_number"));

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn editar_hacia_solape_falla() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    crear(&mut tx, entrada("2034", "1", "2034-01-03", "2034-03-31")?).await?;
    let otra = crear(&mut tx, entrada("2034", "2", "2034-05-01", "2034-07-28")?).await?;

    // RF-10: editarla hacia un rango que invade al primer lapso.
    let resultado = editar(
        &mut tx,
        otra.id,
        entrada("2034", "2", "2034-02-01", "2034-04-30")?,
    )
    .await;

    let Err(error) = resultado else {
        panic!("editar hacia un rango solapado debe fallar (RF-10)");
    };

    let (codigo, restriccion) = codigo_de(&error);
    assert_eq!(codigo, "23P01");
    assert_eq!(restriccion.as_deref(), Some("lapses_no_overlap"));

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn mover_fechas_dejando_un_evento_fuera_se_rechaza() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let lapso = crear(&mut tx, entrada("2035", "1", "2035-01-07", "2035-12-16")?).await?;

    // Un evento dentro del rango original, el 15 de septiembre de 2035.
    sqlx::query(
        "INSERT INTO events (id_lapse, event_date, type_event, source, event_name)
         VALUES ($1, '2035-09-15', 'PRACTICA_REGULAR', 'MANUAL', 'Clase del tramo')",
    )
    .bind(lapso.id)
    .execute(&mut *tx)
    .await?;

    // Pre-chequeo del service (RF-11): las fechas nuevas (hasta agosto) dejan
    // el evento de septiembre fuera; el conteo debe delatarlo.
    let afuera = civik_backend::modules::lapses::queries::eventos_fuera_del_rango(
        tx.as_mut(),
        lapso.id,
        "2035-01-07".parse()?,
        "2035-08-31".parse()?,
    )
    .await?;
    assert_eq!(afuera, 1, "hay exactamente un evento fuera del nuevo rango");

    // Defensa: si el service fallara, el trigger `trg_lapses_dates_against_events`
    // bloquea el UPDATE de fechas con un 23514.
    sqlx::query("SAVEPOINT intento_mover")
        .execute(&mut *tx)
        .await?;
    let resultado = editar(
        &mut tx,
        lapso.id,
        entrada("2035", "1", "2035-01-07", "2035-08-31")?,
    )
    .await;

    let Err(error) = resultado else {
        panic!("mover fechas dejando eventos fuera debe rechazarse (RF-11)");
    };
    let (codigo, _) = codigo_de(&error);
    assert_eq!(
        codigo, "23514",
        "el trigger de fechas vs eventos responde con check_violation"
    );

    sqlx::query("ROLLBACK TO SAVEPOINT intento_mover")
        .execute(&mut *tx)
        .await?;

    let (inicio, fin): (chrono::NaiveDate, chrono::NaiveDate) =
        sqlx::query_as("SELECT start_date, end_date FROM lapses WHERE id = $1")
            .bind(lapso.id)
            .fetch_one(&mut *tx)
            .await?;
    assert_eq!(
        inicio.to_string(),
        "2035-01-07",
        "las fechas originales siguen"
    );
    assert_eq!(fin.to_string(), "2035-12-16");

    tx.rollback().await?;
    Ok(())
}

// --- RF-12, RF-13, RF-14: activación ---

#[tokio::test]
async fn la_activacion_pasa_un_lapso_planeado_a_activo() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let lapso = crear(&mut tx, entrada("2036", "3", "2036-01-06", "2036-03-28")?).await?;

    let activo = activar(&mut tx, lapso.id).await?;
    assert_eq!(activo.status, "ACTIVE", "RF-12: pasa a activo");

    let estado: String = sqlx::query_scalar("SELECT status::text FROM lapses WHERE id = $1")
        .bind(lapso.id)
        .fetch_one(&mut *tx)
        .await?;
    assert_eq!(estado, "ACTIVE");

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn la_segunda_activacion_con_otro_lapso_activo_falla() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let primero = crear(&mut tx, entrada("2037", "1", "2037-01-05", "2037-03-27")?).await?;
    // Rango sin solape con el primero (EXCLUDE).
    let segundo = crear(&mut tx, entrada("2037", "2", "2037-04-01", "2037-06-25")?).await?;

    activar(&mut tx, primero.id).await?;

    // RF-13: con un ACTIVE en pie, activar un segundo lapso viola el índice
    // parcial `uq_lapses_single_active` (23505).
    sqlx::query("SAVEPOINT intento_activar")
        .execute(&mut *tx)
        .await?;
    let resultado = activar(&mut tx, segundo.id).await;

    let Err(error) = resultado else {
        panic!("activar un segundo lapso con otro activo debe fallar (RF-13)");
    };

    let (codigo, restriccion) = codigo_de(&error);
    assert_eq!(codigo, "23505");
    assert_eq!(restriccion.as_deref(), Some("uq_lapses_single_active"));

    sqlx::query("ROLLBACK TO SAVEPOINT intento_activar")
        .execute(&mut *tx)
        .await?;

    // El segundo sigue planeado.
    let estado: String = sqlx::query_scalar("SELECT status::text FROM lapses WHERE id = $1")
        .bind(segundo.id)
        .fetch_one(&mut *tx)
        .await?;
    assert_eq!(estado, "PLANNED");

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn activar_un_lapso_no_planeado_falla() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let lapso = crear(&mut tx, entrada("2038", "1", "2038-01-04", "2038-03-26")?).await?;
    activar(&mut tx, lapso.id).await?;

    // RF-14: `activar` un lapso ya ACTIVE no puede hacer nada. El `set_status`
    // no toca fila (assert PLANNED) y `get_by_id` devuelve el lapso tal cual,
    // sin estado nuevo y sin auditoría falsa.
    let repetida = activar(&mut tx, lapso.id).await?;
    assert_eq!(repetida.status, "ACTIVE", "un lapso ya activo sigue activo");

    let activaciones: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_logs WHERE entity_id = $1 AND action = 'activar'",
    )
    .bind(lapso.id)
    .fetch_one(&mut *tx)
    .await?;
    assert_eq!(activaciones, 1, "solo se audita la activación real");

    tx.rollback().await?;
    Ok(())
}

// --- RF-15, RF-16: cierre ---

#[tokio::test]
async fn el_cierre_pasa_un_lapso_activo_a_cerrado_e_inmutable() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let lapso = crear(&mut tx, entrada("2039", "1", "2039-01-03", "2039-03-31")?).await?;
    activar(&mut tx, lapso.id).await?;

    let cerrado = cerrar(&mut tx, lapso.id).await?;
    assert_eq!(cerrado.status, "CLOSED", "RF-15: el cierre pone CLOSED");

    // RF-15: un CLOSED es terminal. Una segunda transición no hace nada (el
    // WHERE exige ACTIVE) y no escribe una auditoría falsa.
    let repetida = cerrar(&mut tx, lapso.id).await?;
    assert_eq!(repetida.status, "CLOSED");

    // ... y una edición estructural es imposible aunque pase el service
    // (defensa `trg_lapses_closed_frozen`).
    sqlx::query("SAVEPOINT intento_editar")
        .execute(&mut *tx)
        .await?;
    let resultado = editar(
        &mut tx,
        lapso.id,
        entrada("2047", "2", "2047-01-06", "2047-03-27")?,
    )
    .await;
    let Err(error) = resultado else {
        panic!("un lapso CLOSED no se puede editar (RF-15 terminal)");
    };
    let (codigo, _) = codigo_de(&error);
    assert_eq!(codigo, "23514");

    sqlx::query("ROLLBACK TO SAVEPOINT intento_editar")
        .execute(&mut *tx)
        .await?;

    let cierres: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_logs WHERE entity_id = $1 AND action = 'cerrar'",
    )
    .bind(lapso.id)
    .fetch_one(&mut *tx)
    .await?;
    assert_eq!(
        cierres, 1,
        "RF-19: una sola entrada de cierre en el historial"
    );

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn cerrar_un_lapso_no_activo_falla() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let lapso = crear(&mut tx, entrada("2041", "1", "2041-01-02", "2041-03-29")?).await?;

    // RF-16: cerrar un PLANNED no cambia nada (el assert del WHERE exige
    // ACTIVE) y no deja auditoría.
    let repetida = cerrar(&mut tx, lapso.id).await?;
    assert_eq!(repetida.status, "PLANNED", "un planeado no se cierra");

    let cierres: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_logs WHERE entity_id = $1 AND action = 'cerrar'",
    )
    .bind(lapso.id)
    .fetch_one(&mut *tx)
    .await?;
    assert_eq!(cierres, 0, "no se audita un cierre que no hizo nada");

    tx.rollback().await?;
    Ok(())
}

// --- RF-17, RF-18, RF-19: borrado ---

#[tokio::test]
async fn eliminar_un_lapso_vacio_lo_borra_y_deja_auditoria() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let lapso = crear(&mut tx, entrada("2042", "1", "2042-01-08", "2042-03-28")?).await?;

    let borrado = eliminar(&mut tx, lapso.id).await?;
    assert_eq!(borrado.id, lapso.id, "RF-17: devuelve la fila que se borró");

    let quedan: i64 = sqlx::query_scalar("SELECT count(*) FROM lapses WHERE id = $1")
        .bind(lapso.id)
        .fetch_one(&mut *tx)
        .await?;
    assert_eq!(quedan, 0, "RF-17: el lapso vacío se borra de verdad");

    // RF-19: la auditoría del "eliminar" sobrevive y conserva los valores de la
    // fila que desapareció, incluido el estado.
    let (action, data_raw): (String, String) = sqlx::query_as(
        "SELECT action, data::text FROM audit_logs WHERE entity_id = $1 AND action = 'eliminar'",
    )
    .bind(lapso.id)
    .fetch_one(&mut *tx)
    .await?;

    assert_eq!(action, "eliminar");
    let data: serde_json::Value = serde_json::from_str(&data_raw)?;
    assert_eq!(data["anio_escolar"], 2042);
    assert_eq!(data["numero_lapso"], 1);
    assert_eq!(data["inicio"], "2042-01-08");
    assert_eq!(data["fin"], "2042-03-28");
    assert_eq!(data["dia_practica"], "THURSDAY");
    assert_eq!(data["estado"], "PLANNED");

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn eliminar_un_lapso_cerrado_se_rechaza() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let lapso = crear(&mut tx, entrada("2043", "1", "2043-01-07", "2043-03-27")?).await?;
    activar(&mut tx, lapso.id).await?;
    cerrar(&mut tx, lapso.id).await?;

    // RF-18: un CLOSED es historial: ni el service lo elimina ni el
    // guardarraíl `lapses_delete_guard` deja pasar el DELETE.
    sqlx::query("SAVEPOINT intento_eliminar")
        .execute(&mut *tx)
        .await?;
    let resultado = eliminar(&mut tx, lapso.id).await;

    let Err(error) = resultado else {
        panic!("eliminar un lapso cerrado debe fallar (RF-18)");
    };
    let (codigo, _) = codigo_de(&error);
    assert_eq!(
        codigo, "23514",
        "el guardarraíl del borrado responde con check_violation"
    );

    sqlx::query("ROLLBACK TO SAVEPOINT intento_eliminar")
        .execute(&mut *tx)
        .await?;

    let quedan: i64 = sqlx::query_scalar("SELECT count(*) FROM lapses WHERE id = $1")
        .bind(lapso.id)
        .fetch_one(&mut *tx)
        .await?;
    assert_eq!(quedan, 1, "el lapso cerrado sigue existiendo");

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn eliminar_un_lapso_terminado_se_rechaza() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    // Un lapso con fechas pasadas: terminó aunque nunca se haya cerrado.
    let lapso = crear(
        &mut tx,
        preparar_entrada("2020", "3", "2020-01-03", "2020-03-27", "FRIDAY")?,
    )
    .await?;

    // RF-18: el guardarraíl también lo protege (end_date < CURRENT_DATE).
    sqlx::query("SAVEPOINT intento_eliminar")
        .execute(&mut *tx)
        .await?;
    let resultado = eliminar(&mut tx, lapso.id).await;

    let Err(error) = resultado else {
        panic!("eliminar un lapso terminado debe fallar (RF-18)");
    };
    let (codigo, _) = codigo_de(&error);
    assert_eq!(codigo, "23514");

    sqlx::query("ROLLBACK TO SAVEPOINT intento_eliminar")
        .execute(&mut *tx)
        .await?;

    let quedan: i64 = sqlx::query_scalar("SELECT count(*) FROM lapses WHERE id = $1")
        .bind(lapso.id)
        .fetch_one(&mut *tx)
        .await?;
    assert_eq!(quedan, 1);

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn eliminar_un_lapso_con_evento_se_rechaza() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let lapso = crear(&mut tx, entrada("2044", "1", "2044-01-06", "2044-03-26")?).await?;
    sqlx::query(
        "INSERT INTO events (id_lapse, event_date, type_event, source, event_name)
         VALUES ($1, '2044-02-15', 'PRACTICA_REGULAR', 'MANUAL', 'Clase del tramo')",
    )
    .bind(lapso.id)
    .execute(&mut *tx)
    .await?;

    // El pre-chequeo del service (`tiene_datos`) detecta la fila en `events`;
    // sobre él responde el `409` entendible antes de llegar al DELETE.
    let tiene_datos =
        civik_backend::modules::lapses::queries::tiene_datos(tx.as_mut(), lapso.id).await?;
    assert!(
        tiene_datos,
        "el pre-chequeo del service ve los eventos del lapso"
    );

    // RF-18: por si el service fallara, `lapses_delete_guard` aborta el DELETE
    // con 23514 antes de que la FK RESTRICT de `events` pueda sonar: el guard
    // cuenta las filas de `events` y aborta primero (el código 23001 de la FK
    // por este camino nunca se alcanza).
    sqlx::query("SAVEPOINT intento_eliminar")
        .execute(&mut *tx)
        .await?;
    let resultado = eliminar(&mut tx, lapso.id).await;

    let Err(error) = resultado else {
        panic!("eliminar un lapso con eventos debe fallar (RF-18)");
    };

    let (codigo, _) = codigo_de(&error);
    assert_eq!(
        codigo, "23514",
        "el guardarraíl se adelanta a la FK RESTRICT"
    );
    assert!(
        error.to_string().contains("evento"),
        "el mensaje del guard debe señalar los eventos: {error}"
    );

    sqlx::query("ROLLBACK TO SAVEPOINT intento_eliminar")
        .execute(&mut *tx)
        .await?;

    let quedan: i64 = sqlx::query_scalar("SELECT count(*) FROM lapses WHERE id = $1")
        .bind(lapso.id)
        .fetch_one(&mut *tx)
        .await?;
    assert_eq!(quedan, 1, "el lapso con eventos sigue existiendo");

    tx.rollback().await?;
    Ok(())
}

// --- RF-19: auditoría de cada operación ---

#[tokio::test]
async fn el_alta_deja_su_entrada_en_el_historial() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let lapso = crear(&mut tx, entrada("2045", "1", "2045-01-05", "2045-03-25")?).await?;

    let (entity, action, data_raw): (String, String, String) =
        sqlx::query_as("SELECT entity, action, data::text FROM audit_logs WHERE entity_id = $1")
            .bind(lapso.id)
            .fetch_one(&mut *tx)
            .await?;

    assert_eq!(entity, "lapses");
    assert_eq!(action, "alta");

    let data: serde_json::Value = serde_json::from_str(&data_raw)?;
    assert_eq!(
        data["anio_escolar"], 2045,
        "la auditoría guarda los valores"
    );
    assert_eq!(data["numero_lapso"], 1);
    assert_eq!(data["inicio"], "2045-01-05");
    assert_eq!(data["dia_practica"], "THURSDAY");

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn la_activacion_y_el_cierre_dejan_su_auditoria() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let lapso = crear(&mut tx, entrada("2046", "2", "2046-01-04", "2046-03-24")?).await?;
    activar(&mut tx, lapso.id).await?;
    cerrar(&mut tx, lapso.id).await?;

    // RF-19: cada transición real deja su entrada, con el estado como payload.
    let (accion, data_raw): (String, String) = sqlx::query_as(
        "SELECT action, data::text FROM audit_logs WHERE entity_id = $1 AND action = 'activar'",
    )
    .bind(lapso.id)
    .fetch_one(&mut *tx)
    .await?;
    assert_eq!(accion, "activar");
    let data: serde_json::Value = serde_json::from_str(&data_raw)?;
    assert_eq!(data["estado"], "ACTIVE");

    let (accion, data_raw): (String, String) = sqlx::query_as(
        "SELECT action, data::text FROM audit_logs WHERE entity_id = $1 AND action = 'cerrar'",
    )
    .bind(lapso.id)
    .fetch_one(&mut *tx)
    .await?;
    assert_eq!(accion, "cerrar");
    let data: serde_json::Value = serde_json::from_str(&data_raw)?;
    assert_eq!(data["estado"], "CLOSED");

    tx.rollback().await?;
    Ok(())
}

/// RF-19 (variante de `eliminar` con el ciclo completo): la auditoría del
/// "eliminar" precede al borrado aunque el lapso haya pasado por otras
/// operaciones.
#[tokio::test]
async fn el_borrado_deja_su_auditoria_antes_de_desaparecer() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let lapso = crear(&mut tx, entrada("2047", "1", "2047-01-03", "2047-03-23")?).await?;
    activar(&mut tx, lapso.id).await?;

    eliminar(&mut tx, lapso.id).await?;

    let acciones: Vec<(String, String)> = sqlx::query_as(
        "SELECT action, data::text FROM audit_logs WHERE entity_id = $1 ORDER BY id",
    )
    .bind(lapso.id)
    .fetch_all(&mut *tx)
    .await?;

    let nombres: Vec<&str> = acciones.iter().map(|(accion, _)| accion.as_str()).collect();
    assert_eq!(nombres, ["alta", "activar", "eliminar"]);

    let (_, data_raw) = acciones.last().unwrap();
    let data: serde_json::Value = serde_json::from_str(data_raw)?;
    assert_eq!(
        data["estado"], "ACTIVE",
        "la auditoría conserva el último estado"
    );

    tx.rollback().await?;
    Ok(())
}
