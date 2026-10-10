//! Tests de `attendance` a nivel de queries y dominio, con el patrón
//! transacción/rollback: cada test corre dentro de una transacción que nunca
//! se confirma, así que la base queda intacta y no hay colisión entre tests en
//! paralelo (RNF-5).

mod common;

use chrono::{Datelike, NaiveDate, Weekday};
use civik_backend::modules::attendance::domain::{
    puede_escribir_asistencia, validar_state, AttendanceState, ErrorAsistencia, EventId, StudentId,
};
use civik_backend::modules::attendance::queries as att_queries;
use civik_backend::modules::events::queries as events_queries;
use civik_backend::modules::lapses::domain::preparar_entrada;
use civik_backend::modules::lapses::queries as lapses_queries;
use civik_backend::modules::sections::domain::{DatosSeccion, Grado, Turno};
use civik_backend::modules::sections::queries as sections_queries;
use civik_backend::modules::students::domain::DatosEstudiante;
use civik_backend::modules::students::queries as students_queries;
use sqlx::PgTransaction;
use uuid::Uuid;

type Resultado = Result<(), Box<dyn std::error::Error>>;

fn fecha(anio: i32, mes: u32, dia: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(anio, mes, dia).expect("fecha válida")
}

/// El primer jueves de septiembre del año: la regla 1 exige que una práctica
/// regular caiga en el `practice_day` del lapso (que armamos como THURSDAY).
fn primer_jueves(anio: i32) -> NaiveDate {
    let mut dia = fecha(anio, 9, 1);
    while dia.weekday() != Weekday::Thu {
        dia = dia.succ_opt().expect("hay jueves en septiembre");
    }
    dia
}

/// Arma lapso + sección + estudiante + evento dentro de la transacción dada y
/// devuelve sus ids. `seccion` debe ser única en la corrida.
async fn montar(
    tx: &mut PgTransaction<'_>,
    anio: i32,
    seccion: &str,
) -> Result<(Uuid, Uuid, Uuid, Uuid), Box<dyn std::error::Error>> {
    let lap = lapses_queries::crear(
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
    let sec = sections_queries::crear(
        tx,
        DatosSeccion {
            grado: Grado::Primero,
            seccion: seccion.into(),
            turno: Turno::Matutino,
        },
    )
    .await?;
    let est = students_queries::crear(
        tx,
        DatosEstudiante {
            id_seccion: sec.id,
            nombre: "Ana".into(),
            apellido: "Alfa".into(),
            telefono: None,
        },
    )
    .await?;
    let ev = events_queries::crear_manual(
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
async fn upsert_crea_y_luego_actualiza_sin_duplicar() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let (_lap, _sec, est, ev) = montar(&mut tx, 2080, "A1").await?;
    let id_event = EventId(ev);
    let id_student = StudentId(est);

    att_queries::upsert(tx.as_mut(), id_event, id_student, "PRESENTE").await?;
    att_queries::upsert(tx.as_mut(), id_event, id_student, "AUSENTE").await?;

    let filas = att_queries::rows_by_event(tx.as_mut(), id_event).await?;
    assert_eq!(filas.len(), 1, "el upsert no debe duplicar la PK compuesta");
    assert_eq!(filas[0].state.as_deref(), Some("AUSENTE"));

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn corregir_cambia_estado_sin_borrar_fila() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let (_lap, _sec, est, ev) = montar(&mut tx, 2081, "A2").await?;
    let id_event = EventId(ev);
    let id_student = StudentId(est);

    att_queries::upsert(tx.as_mut(), id_event, id_student, "PRESENTE").await?;
    let corregida = att_queries::set_state(tx.as_mut(), id_event, id_student, "AUSENTE").await?;
    assert_eq!(corregida.state.as_deref(), Some("AUSENTE"));

    let filas = att_queries::rows_by_event(tx.as_mut(), id_event).await?;
    assert_eq!(filas.len(), 1, "corregir no borra ni duplica");
    assert_eq!(filas[0].state.as_deref(), Some("AUSENTE"));

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn get_devuelve_none_si_no_hay_fila() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let (_lap, _sec, est, ev) = montar(&mut tx, 2082, "A3").await?;
    let fila = att_queries::get(tx.as_mut(), EventId(ev), StudentId(est)).await?;
    assert!(fila.is_none());

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn padron_incluye_activos_y_marca_lo_no_registrado() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let (_lap, sec, _est, ev) = montar(&mut tx, 2083, "A4").await?;
    let id_event = EventId(ev);

    // Un segundo estudiante, con apellido posterior, para verificar el orden.
    let zeta = students_queries::crear(
        &mut tx,
        DatosEstudiante {
            id_seccion: sec,
            nombre: "Beto".into(),
            apellido: "Zeta".into(),
            telefono: None,
        },
    )
    .await?;
    let alfa = students_queries::crear(
        &mut tx,
        DatosEstudiante {
            id_seccion: sec,
            nombre: "Ana".into(),
            apellido: "Alfa".into(),
            telefono: None,
        },
    )
    .await?;

    att_queries::upsert(tx.as_mut(), id_event, StudentId(alfa.id), "JUSTIFICADO").await?;

    let padron = att_queries::padron(tx.as_mut(), id_event).await?;
    let pos_alfa = padron
        .iter()
        .position(|f| f.id_student == alfa.id)
        .expect("Alfa está en el padrón");
    let pos_zeta = padron
        .iter()
        .position(|f| f.id_student == zeta.id)
        .expect("Zeta está en el padrón");

    assert!(pos_alfa < pos_zeta, "orden canónico por apellido");
    assert_eq!(padron[pos_alfa].state.as_deref(), Some("JUSTIFICADO"));
    assert!(
        padron[pos_zeta].state.is_none(),
        "un estudiante sin fila sale con estado nulo"
    );

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn padron_incluye_de_baja_con_asistencia_y_excluye_de_baja_sin_ella() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let (_lap, sec, est, ev) = montar(&mut tx, 2084, "A5").await?;
    let id_event = EventId(ev);
    let baja_con_fila = students_queries::crear(
        &mut tx,
        DatosEstudiante {
            id_seccion: sec,
            nombre: "Carla".into(),
            apellido: "Baja".into(),
            telefono: None,
        },
    )
    .await?;
    let baja_sin_fila = students_queries::crear(
        &mut tx,
        DatosEstudiante {
            id_seccion: sec,
            nombre: "Ceci".into(),
            apellido: "Cero".into(),
            telefono: None,
        },
    )
    .await?;

    att_queries::upsert(
        tx.as_mut(),
        id_event,
        StudentId(baja_con_fila.id),
        "AUSENTE",
    )
    .await?;
    students_queries::set_active(&mut tx, baja_con_fila.id, false).await?;
    students_queries::set_active(&mut tx, baja_sin_fila.id, false).await?;

    let padron = att_queries::padron(tx.as_mut(), id_event).await?;
    assert!(
        padron.iter().any(|f| f.id_student == baja_con_fila.id),
        "un estudiante de baja con fila sigue en el padrón"
    );
    assert!(
        !padron.iter().any(|f| f.id_student == baja_sin_fila.id),
        "un estudiante de baja sin fila no aparece"
    );

    // El de alta también está (el padrón es global).
    assert!(padron.iter().any(|f| f.id_student == est));

    tx.rollback().await?;
    Ok(())
}

// --- Dominio puro, ejercitado desde fuera de la crate (RNF-2, RF-5) ---

#[test]
fn el_dominio_publico_valida_estados_y_elegibilidad() {
    assert_eq!(validar_state("AUSENTE"), Ok(AttendanceState::Ausente));
    assert!(matches!(
        validar_state("presente"),
        Err(ErrorAsistencia::EstadoInvalido(_))
    ));
    assert_eq!(AttendanceState::Justificado.as_str(), "JUSTIFICADO");

    assert_eq!(
        puede_escribir_asistencia(Some("SCHEDULED"), Some("CLOSED")),
        Err(ErrorAsistencia::LapsoCerrado)
    );
    assert!(puede_escribir_asistencia(Some("SCHEDULED"), Some("ACTIVE")).is_ok());
}
