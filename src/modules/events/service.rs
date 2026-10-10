//! Orquestación del módulo `events` (CRUD manual + generación automática y
//! reprogramación de la fase 6).

use crate::modules::events::domain::{
    self, es_practica_regular_manual, fechas_de_practica, preparar_creacion_manual,
    preparar_edicion_manual, proxima_fecha_practica, puede_cancelarse, puede_editarse,
    puede_generar_auto, puede_reprogramarse_evento, DatosCrearManual, DatosEditarManual,
    EstadoEvento,
};
use crate::modules::events::queries::{self, Event};
use crate::modules::lapses::domain::{DiaSemana, EstadoLapso};
use crate::shared::audit::write_audit_log as write_audit;
use crate::shared::error::AppError;
use chrono::Datelike;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;
use std::collections::BTreeSet;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct EntradaCrearManual {
    pub id_lapse: Uuid,
    pub event_date: String,
    pub type_event: String,
    pub event_name: String,
}

#[derive(Deserialize)]
pub struct EntradaEditarManual {
    pub event_date: String,
    pub type_event: String,
    pub event_name: String,
}

#[derive(Deserialize)]
pub struct EntradaCancelar {
    pub motivo: String,
}

fn a_not_found(error: sqlx::Error) -> AppError {
    match error {
        sqlx::Error::RowNotFound => AppError::NotFound("No se encontró el evento".into()),
        otro => otro.into(),
    }
}

fn dominio_a_bad_request(error: domain::ErrorDominio) -> AppError {
    AppError::BadRequest(error.to_string())
}

fn dominio_a_conflict(error: domain::ErrorDominio) -> AppError {
    AppError::Conflict(error.to_string())
}

pub async fn crear_manual(pool: &PgPool, entrada: &EntradaCrearManual) -> Result<Event, AppError> {
    let datos: DatosCrearManual = preparar_creacion_manual(
        &entrada.event_date,
        &entrada.type_event,
        &entrada.event_name,
    )
    .map_err(dominio_a_bad_request)?;

    let mut tx = pool.begin().await?;

    let lapso = queries::obtener_lapso(tx.as_mut(), entrada.id_lapse)
        .await
        .map_err(a_not_found)?;

    // RF-3: rango dentro del lapso
    if datos.event_date < lapso.start_date || datos.event_date > lapso.end_date {
        return Err(AppError::Conflict(format!(
            "La fecha del evento {} no está dentro del rango del lapso",
            datos.event_date
        )));
    }

    // RF-4: práctica regular manual solo en practice_day
    let tipo = datos.type_event;
    if es_practica_regular_manual(tipo, true) {
        let dia_lapso_str = lapso.practice_day.as_deref().unwrap_or("MONDAY");
        let dia_lapso = match dia_lapso_str {
            "MONDAY" => chrono::Weekday::Mon,
            "TUESDAY" => chrono::Weekday::Tue,
            "WEDNESDAY" => chrono::Weekday::Wed,
            "THURSDAY" => chrono::Weekday::Thu,
            "FRIDAY" => chrono::Weekday::Fri,
            "SATURDAY" => chrono::Weekday::Sat,
            "SUNDAY" => chrono::Weekday::Sun,
            _ => chrono::Weekday::Mon,
        };
        if datos.event_date.weekday() != dia_lapso {
            return Err(dominio_a_conflict(
                domain::ErrorDominio::PracticaRegularFueraDeDia {
                    dia_practica: lapso
                        .practice_day
                        .as_deref()
                        .unwrap_or("MONDAY")
                        .to_string(),
                    fecha: datos.event_date,
                    dia_fecha: datos.event_date.weekday().to_string(),
                },
            ));
        }
    }

    let evento = queries::crear_manual(
        tx.as_mut(),
        entrada.id_lapse,
        datos.event_date,
        tipo.as_str(),
        &datos.event_name,
    )
    .await?;

    write_audit(
        tx.as_mut(),
        "events",
        evento.id,
        "crear",
        Some(&json!({
            "id_lapse": evento.id_lapse,
            "event_date": evento.event_date,
            "type_event": evento.type_event.as_deref().unwrap_or(""),
            "source": evento.source.as_deref().unwrap_or("MANUAL"),
            "event_name": evento.event_name,
            "status": evento.status.as_deref().unwrap_or("SCHEDULED"),
        })),
    )
    .await?;

    tx.commit().await?;
    Ok(evento)
}

pub async fn cancelar(pool: &PgPool, id: Uuid, motivo: &str) -> Result<Event, AppError> {
    let motivo = motivo.trim();
    if motivo.is_empty() {
        return Err(AppError::BadRequest(
            "El motivo no puede estar vacío".into(),
        ));
    }

    let mut tx = pool.begin().await?;

    let evento = queries::get_by_id(tx.as_mut(), id)
        .await
        .map_err(a_not_found)?;
    let estado = EstadoEvento::parse(evento.status.as_deref().unwrap_or("")).ok_or_else(|| {
        AppError::Conflict(format!("Estado de evento desconocido: {:?}", evento.status))
    })?;
    let tiene_asistencia = queries::tiene_asistencia(tx.as_mut(), id).await?;
    puede_cancelarse(estado, tiene_asistencia).map_err(dominio_a_conflict)?;

    let actualizado = queries::cancelar(tx.as_mut(), id, motivo)
        .await
        .map_err(a_not_found)?;

    write_audit(
        tx.as_mut(),
        "events",
        actualizado.id,
        "cancelar",
        Some(&json!({
            "id_lapse": actualizado.id_lapse,
            "event_date": actualizado.event_date,
            "type_event": actualizado.type_event.as_deref().unwrap_or(""),
            "status": actualizado.status.as_deref().unwrap_or("SCHEDULED"),
            "motivo_cancelado": actualizado.cancelled_reason,
        })),
    )
    .await?;

    tx.commit().await?;
    Ok(actualizado)
}

pub async fn editar_manual(
    pool: &PgPool,
    id: Uuid,
    entrada: &EntradaEditarManual,
) -> Result<Event, AppError> {
    let datos: DatosEditarManual = preparar_edicion_manual(
        &entrada.event_date,
        &entrada.type_event,
        &entrada.event_name,
    )
    .map_err(dominio_a_bad_request)?;

    let mut tx = pool.begin().await?;

    let evento = queries::get_by_id(tx.as_mut(), id)
        .await
        .map_err(a_not_found)?;
    let estado = EstadoEvento::parse(evento.status.as_deref().unwrap_or("")).ok_or_else(|| {
        AppError::Conflict(format!("Estado de evento desconocido: {:?}", evento.status))
    })?;
    let source_manual = evento.source.as_deref() == Some("MANUAL");
    let tiene_asistencia = queries::tiene_asistencia(tx.as_mut(), id).await?;
    puede_editarse(estado, source_manual, tiene_asistencia).map_err(dominio_a_conflict)?;

    // Validar rango y práctica regular con lapso actual
    let lapso = queries::obtener_lapso(tx.as_mut(), evento.id_lapse)
        .await
        .map_err(a_not_found)?;
    if datos.event_date < lapso.start_date || datos.event_date > lapso.end_date {
        return Err(AppError::Conflict(format!(
            "La fecha del evento {} no está dentro del rango del lapso",
            datos.event_date
        )));
    }
    let tipo = datos.type_event;
    if es_practica_regular_manual(tipo, source_manual) {
        let dia_lapso_str = lapso.practice_day.as_deref().unwrap_or("MONDAY");
        let dia_lapso = match dia_lapso_str {
            "MONDAY" => chrono::Weekday::Mon,
            "TUESDAY" => chrono::Weekday::Tue,
            "WEDNESDAY" => chrono::Weekday::Wed,
            "THURSDAY" => chrono::Weekday::Thu,
            "FRIDAY" => chrono::Weekday::Fri,
            "SATURDAY" => chrono::Weekday::Sat,
            "SUNDAY" => chrono::Weekday::Sun,
            _ => chrono::Weekday::Mon,
        };
        if datos.event_date.weekday() != dia_lapso {
            return Err(dominio_a_conflict(
                domain::ErrorDominio::PracticaRegularFueraDeDia {
                    dia_practica: lapso
                        .practice_day
                        .as_deref()
                        .unwrap_or("MONDAY")
                        .to_string(),
                    fecha: datos.event_date,
                    dia_fecha: datos.event_date.weekday().to_string(),
                },
            ));
        }
    }

    let actualizado = queries::editar_manual(
        tx.as_mut(),
        id,
        datos.event_date,
        tipo.as_str(),
        &datos.event_name,
    )
    .await
    .map_err(a_not_found)?;

    write_audit(
        tx.as_mut(),
        "events",
        actualizado.id,
        "editar",
        Some(&json!({
            "id_lapse": actualizado.id_lapse,
            "event_date": actualizado.event_date,
            "type_event": actualizado.type_event.as_deref().unwrap_or(""),
            "event_name": actualizado.event_name,
        })),
    )
    .await?;

    tx.commit().await?;
    Ok(actualizado)
}

pub async fn consultar(pool: &PgPool, id: Uuid) -> Result<Event, AppError> {
    queries::get_by_id(pool, id).await.map_err(a_not_found)
}

pub async fn listar(pool: &PgPool, lapse_id: Option<Uuid>) -> Result<Vec<Event>, AppError> {
    match lapse_id {
        Some(lid) => Ok(queries::listar_por_lapso(pool, lid).await?),
        None => Ok(queries::listar_all(pool).await?),
    }
}

/// Error de un día de práctica inválido (viene de `lapses::domain`): se
/// responde 400, igual que las validaciones de entrada de `events`.
fn dia_a_bad_request(error: crate::modules::lapses::domain::ErrorDominio) -> AppError {
    AppError::BadRequest(error.to_string())
}

/// Genera una práctica regular automática por cada `practice_day` del lapso
/// (RF-14, RF-15).
///
/// - Solo si el lapso no está `CLOSED` (RF-19); `PLANNED` y `ACTIVE` sí.
/// - Omite las fechas que ya tienen una práctica regular (AUTO o MANUAL, en
///   cualquier estado): «una regular por día, la cree quien la cree»
///   (`MEMORY.md`). Por eso es idempotente: una segunda llamada no crea nada.
/// - Los `AUTO` caen en el `practice_day` por construcción (solo se generan
///   ahí) y son `PRACTICA_REGULAR` por construcción (reglas 1 y 2 de la
///   base); los triggers quedan como defensa.
/// - Auditoría única por operación, con las fechas generadas y las omitidas.
pub async fn generar_practicas_auto(pool: &PgPool, id_lapse: Uuid) -> Result<Vec<Event>, AppError> {
    let mut tx = pool.begin().await?;

    let lapso = queries::obtener_lapso(tx.as_mut(), id_lapse)
        .await
        .map_err(a_not_found)?;
    let estado = EstadoLapso::parse(lapso.status.as_deref().unwrap_or("")).ok_or_else(|| {
        AppError::Conflict(format!("Estado de lapso desconocido: {:?}", lapso.status))
    })?;
    if !puede_generar_auto(&estado) {
        return Err(AppError::Conflict(
            "No se puede generar prácticas: el lapso está cerrado (CLOSED)".into(),
        ));
    }
    let dia = DiaSemana::try_from(lapso.practice_day.as_deref().unwrap_or(""))
        .map_err(dia_a_bad_request)?;

    let existentes = queries::listar_por_lapso(tx.as_mut(), id_lapse).await?;

    let mut generados: Vec<Event> = Vec::new();
    let mut omitidas: Vec<chrono::NaiveDate> = Vec::new();
    for fecha in fechas_de_practica(lapso.start_date, lapso.end_date, dia) {
        let ya_hay_regular = existentes.iter().any(|evento| {
            evento.event_date == fecha && evento.type_event.as_deref() == Some("PRACTICA_REGULAR")
        });
        if ya_hay_regular {
            omitidas.push(fecha);
            continue;
        }
        let evento = queries::crear_auto(tx.as_mut(), id_lapse, fecha, "Práctica regular").await?;
        generados.push(evento);
    }

    write_audit(
        tx.as_mut(),
        "lapses",
        id_lapse,
        "generar_practicas_auto",
        Some(&json!({
            "generadas": generados.len(),
            "fechas": generados.iter().map(|e| e.event_date).collect::<Vec<_>>(),
            "omitidas": omitidas,
        })),
    )
    .await?;

    tx.commit().await?;
    Ok(generados)
}

#[derive(Debug, Clone, Serialize)]
pub struct ResultadoReprogramacion {
    pub dia_anterior: String,
    pub dia_nuevo: String,
    pub movidos: usize,
    pub no_movidos: usize,
}

/// Cambia el día de práctica del lapso y reubica los eventos pendientes
/// (RF-16, RF-17, RF-18).
///
/// - Actualiza primero `lapses.practice_day` y después mueve fechas, para que
///   el trigger de la regla 1 (`check_event_date_within_lapse`) evalúe el
///   `event_date` contra el **nuevo** día de práctica.
/// - Solo se mueven los pendientes (SCHEDULED, sin asistencia, práctica
///   regular AUTO o MANUAL). Congelados (con asistencia), cancelados y
///   no-regulares se quedan (RF-16).
/// - Un evento cuyo destino está ocupado por otra regular, o al que no le
///   cabe una fecha del nuevo día en el lapso, se queda donde está y suma a
///   `no_movidos` (RF-17, RF-18): el calendario conserva «una regular por
///   día».
/// - Reprogamar con el mismo día es un no-op (200, `movidos = 0`) y **no**
///   deja entrada de auditoría.
pub async fn reprogramar_practice_day(
    pool: &PgPool,
    id_lapse: Uuid,
    nuevo_dia: &str,
) -> Result<ResultadoReprogramacion, AppError> {
    let nuevo = DiaSemana::try_from(nuevo_dia).map_err(dia_a_bad_request)?;

    let mut tx = pool.begin().await?;

    let lapso = queries::obtener_lapso(tx.as_mut(), id_lapse)
        .await
        .map_err(a_not_found)?;
    let estado = EstadoLapso::parse(lapso.status.as_deref().unwrap_or("")).ok_or_else(|| {
        AppError::Conflict(format!("Estado de lapso desconocido: {:?}", lapso.status))
    })?;
    if estado == EstadoLapso::Cerrado {
        return Err(AppError::Conflict(
            "No se puede reprogramar: el lapso está cerrado (CLOSED)".into(),
        ));
    }
    let actual = DiaSemana::try_from(lapso.practice_day.as_deref().unwrap_or(""))
        .map_err(dia_a_bad_request)?;
    if nuevo == actual {
        return Ok(ResultadoReprogramacion {
            dia_anterior: actual.as_str().to_string(),
            dia_nuevo: nuevo.as_str().to_string(),
            movidos: 0,
            no_movidos: 0,
        });
    }

    queries::set_practice_day(tx.as_mut(), id_lapse, nuevo.as_str())
        .await
        .map_err(a_not_found)?;

    let eventos = queries::listar_por_lapso(tx.as_mut(), id_lapse).await?;

    // Las fechas ya ocupadas por una práctica regular: un destino ocupado se
    // respeta y el evento pendiente se queda.
    let mut ocupadas: BTreeSet<chrono::NaiveDate> = eventos
        .iter()
        .filter(|evento| evento.type_event.as_deref() == Some("PRACTICA_REGULAR"))
        .map(|evento| evento.event_date)
        .collect();

    let mut reprogramados: Vec<(chrono::NaiveDate, chrono::NaiveDate)> = Vec::new();
    let mut no_movidos: usize = 0;

    for evento in &eventos {
        let tiene_asistencia = queries::tiene_asistencia(tx.as_mut(), evento.id).await?;
        if !puede_reprogramarse_evento(
            evento.status.as_deref(),
            evento.source.as_deref(),
            evento.type_event.as_deref(),
            tiene_asistencia,
        ) {
            continue;
        }

        let Some(destino) = proxima_fecha_practica(evento.event_date, lapso.end_date, nuevo) else {
            no_movidos += 1;
            continue;
        };
        if !ocupadas.insert(destino) {
            no_movidos += 1;
            continue;
        }

        queries::mover_fecha_evento(tx.as_mut(), evento.id, destino)
            .await
            .map_err(a_not_found)?;
        ocupadas.remove(&evento.event_date);
        reprogramados.push((evento.event_date, destino));
    }

    write_audit(
        tx.as_mut(),
        "lapses",
        id_lapse,
        "reprogramar_practice_day",
        Some(&json!({
            "dia_anterior": actual.as_str(),
            "dia_nuevo": nuevo.as_str(),
            "reprogramados": reprogramados
                .iter()
                .map(|(anterior, nuevo)| json!({ "anterior": anterior, "nuevo": nuevo }))
                .collect::<Vec<_>>(),
            "no_movidos": no_movidos,
        })),
    )
    .await?;

    tx.commit().await?;

    Ok(ResultadoReprogramacion {
        dia_anterior: actual.as_str().to_string(),
        dia_nuevo: nuevo.as_str().to_string(),
        movidos: reprogramados.len(),
        no_movidos,
    })
}
