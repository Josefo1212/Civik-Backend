//! Orquestación del módulo `events` (CRUD manual).

use crate::modules::events::domain::{
    self, es_practica_regular_manual, preparar_creacion_manual, preparar_edicion_manual,
    puede_cancelarse, puede_editarse, DatosCrearManual, DatosEditarManual, EstadoEvento,
};
use crate::modules::events::queries::{self, Event};
use crate::shared::audit::write_audit_log as write_audit;
use crate::shared::error::AppError;
use chrono::Datelike;
use serde::Deserialize;
use serde_json::json;
use sqlx::PgPool;
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
    let datos: DatosCrearManual =
        preparar_creacion_manual(&entrada.event_date, &entrada.type_event, &entrada.event_name)
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
            return Err(dominio_a_conflict(domain::ErrorDominio::PracticaRegularFueraDeDia {
                dia_practica: lapso.practice_day.as_deref().unwrap_or("MONDAY").to_string(),
                fecha: datos.event_date,
                dia_fecha: datos.event_date.weekday().to_string(),
            }));
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
        return Err(AppError::BadRequest("El motivo no puede estar vacío".into()));
    }

    let mut tx = pool.begin().await?;

    let evento = queries::get_by_id(tx.as_mut(), id).await.map_err(a_not_found)?;
    let estado = EstadoEvento::parse(evento.status.as_deref().unwrap_or("")).ok_or_else(|| {
        AppError::Conflict(format!("Estado de evento desconocido: {:?}", evento.status))
    })?;
    let tiene_asistencia = queries::tiene_asistencia(tx.as_mut(), id).await?;
    puede_cancelarse(estado, tiene_asistencia).map_err(dominio_a_conflict)?;

    let actualizado = queries::cancelar(tx.as_mut(), id, motivo).await.map_err(a_not_found)?;

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
    let datos: DatosEditarManual =
        preparar_edicion_manual(&entrada.event_date, &entrada.type_event, &entrada.event_name)
            .map_err(dominio_a_bad_request)?;

    let mut tx = pool.begin().await?;

    let evento = queries::get_by_id(tx.as_mut(), id).await.map_err(a_not_found)?;
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
            return Err(dominio_a_conflict(domain::ErrorDominio::PracticaRegularFueraDeDia {
                dia_practica: lapso.practice_day.as_deref().unwrap_or("MONDAY").to_string(),
                fecha: datos.event_date,
                dia_fecha: datos.event_date.weekday().to_string(),
            }));
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
