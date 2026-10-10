//! Casos de uso de `attendance` (hexagonal parcial): orquesta la transacción,
//! aplica las reglas puras de `domain` y escribe la auditoría. La secuencia es
//! de negocio; las invariantes duras siguen en los triggers de la base.

use std::collections::HashSet;

use serde_json::json;
use sqlx::{PgPool, PgTransaction};
use uuid::Uuid;

use crate::modules::events::queries as events_queries;
use crate::modules::students::queries as students_queries;
use crate::shared::audit::write_audit_log_tx;
use crate::shared::error::{mapear_violacion, AppError};

use super::domain::{
    puede_escribir_asistencia, validar_state, ErrorAsistencia, EventId, StudentId,
};
use super::queries::{self, Attendance, FilaPadron};

/// El cuerpo del alta en lote (RF-1): una entrada por par a registrar.
#[derive(serde::Deserialize, Debug)]
pub struct RegistroLote {
    pub items: Vec<Item>,
}

#[derive(serde::Deserialize, Debug)]
pub struct Item {
    pub id_student: Uuid,
    pub state: String,
}

/// Traduce el `RowNotFound` de la carga del evento a un `404` con mensaje.
fn a_not_found(error: sqlx::Error) -> AppError {
    match error {
        sqlx::Error::RowNotFound => AppError::NotFound("No se encontró el evento".into()),
        _ => AppError::Database(error),
    }
}

/// Traduce los errores de `sqlx` que traen una constraint conocida (FK, etc.)
/// a su `409`; el resto sube como error de infraestructura (`500`).
fn map_sqlx_err(error: sqlx::Error) -> AppError {
    if let sqlx::Error::Database(db_err) = &error {
        if let Some(app_err) = mapear_violacion(db_err.code().as_deref(), db_err.constraint()) {
            return app_err;
        }
    }
    AppError::Database(error)
}

/// Códigos HTTP de los errores de dominio: estado inválido es `400`; evento
/// cancelado, lapso cerrado y estudiante de baja son `409`.
fn desde_dominio(error: ErrorAsistencia) -> AppError {
    match error {
        ErrorAsistencia::EstadoInvalido(_) => AppError::BadRequest(error.to_string()),
        ErrorAsistencia::EventoCancelado
        | ErrorAsistencia::LapsoCerrado
        | ErrorAsistencia::EstudianteInactivo => AppError::Conflict(error.to_string()),
    }
}

/// Carga el estado del evento y el de su lapso dentro de una transacción.
///
/// El evento inexistente es `404`; el lapso siempre existe (FK), pero si la
/// lectura falla se trata igual: sin lapso no hay nada que validar.
async fn cargar_estados(
    tx: &mut PgTransaction<'_>,
    id_event: EventId,
) -> Result<(Option<String>, Option<String>), AppError> {
    let evento = events_queries::get_by_id(tx.as_mut(), id_event.0)
        .await
        .map_err(a_not_found)?;
    let lapso = events_queries::obtener_lapso(tx.as_mut(), evento.id_lapse)
        .await
        .map_err(a_not_found)?;

    Ok((evento.status, lapso.status))
}

/// Registra la asistencia de un evento en lote y devuelve el padrón actualizado
/// (RF-1 a RF-7, RF-14, RF-15).
///
/// Todo ocurre en **una transacción** (RF-7): si cualquier par falla, no se
/// escribe ninguno. Se valida primero el lote entero (estados, existencia y
/// estado de cada estudiante) y recién después se escribe, para que un error
/// no deje filas a medio guardar ni dependa del orden de la lista.
pub async fn registrar_listado(
    pool: &PgPool,
    id_event: EventId,
    lote: RegistroLote,
) -> Result<Vec<FilaPadron>, AppError> {
    let mut tx = pool.begin().await?;

    let (estado_evento, estado_lapso) = cargar_estados(&mut tx, id_event).await?;
    puede_escribir_asistencia(estado_evento.as_deref(), estado_lapso.as_deref())
        .map_err(desde_dominio)?;

    // Normaliza cada estado (RF-5) antes de escribir.
    let mut preparados: Vec<(StudentId, &'static str)> = Vec::with_capacity(lote.items.len());
    for item in &lote.items {
        let estado = validar_state(&item.state).map_err(desde_dominio)?;
        preparados.push((StudentId(item.id_student), estado.as_str()));
    }

    // Quiénes ya tienen fila: decide "creada" vs "actualizada" para la
    // auditoría y permite registrar a un estudiante de baja que ya tenía fila
    // (RF-6).
    let ya_registrados: HashSet<Uuid> = queries::rows_by_event(tx.as_mut(), id_event)
        .await
        .map_err(map_sqlx_err)?
        .into_iter()
        .map(|fila| fila.id_student)
        .collect();

    for (id_student, _) in &preparados {
        let estudiante = students_queries::consultar(tx.as_mut(), id_student.0)
            .await
            .map_err(|_| AppError::NotFound("No se encontró el estudiante".into()))?;
        if !estudiante.active && !ya_registrados.contains(&id_student.0) {
            return Err(desde_dominio(ErrorAsistencia::EstudianteInactivo));
        }
    }

    let mut creadas = 0usize;
    let mut actualizadas = 0usize;
    for (id_student, state) in &preparados {
        queries::upsert(tx.as_mut(), id_event, *id_student, state)
            .await
            .map_err(map_sqlx_err)?;
        if ya_registrados.contains(&id_student.0) {
            actualizadas += 1;
        } else {
            creadas += 1;
        }
    }

    // Una lista vacía es un no-op válido: sin filas escritas, sin auditoría.
    if !preparados.is_empty() {
        let payload = json!({ "creadas": creadas, "actualizadas": actualizadas });
        write_audit_log_tx(
            &mut tx,
            "attendance",
            id_event.0,
            "registrar_asistencia",
            Some(&payload),
        )
        .await?;
    }

    let padron = queries::padron(tx.as_mut(), id_event).await?;
    tx.commit().await?;
    Ok(padron)
}

/// El padrón de un evento (RF-13, RF-14): evento inexistente, `404`.
pub async fn listar_padron(pool: &PgPool, id_event: EventId) -> Result<Vec<FilaPadron>, AppError> {
    events_queries::get_by_id(pool, id_event.0)
        .await
        .map_err(a_not_found)?;
    let padron = queries::padron(pool, id_event).await?;
    Ok(padron)
}

/// Corrige el estado de un par (evento, estudiante) (RF-8 a RF-12, RF-15).
///
/// La corrección **no crea**: sin fila registrada responde `404` (RF-9). Si el
/// estado nuevo es igual al actual, responde la fila sin escribir y sin dejar
/// auditoría (RF-10). Un evento cancelado o un lapso cerrado se rechazan con
/// `409` antes de tocar la base, igual que en el alta.
pub async fn corregir_asistencia(
    pool: &PgPool,
    id_event: EventId,
    id_student: StudentId,
    state: &str,
) -> Result<Attendance, AppError> {
    let estado = validar_state(state).map_err(desde_dominio)?;

    let mut tx = pool.begin().await?;

    let (estado_evento, estado_lapso) = cargar_estados(&mut tx, id_event).await?;
    puede_escribir_asistencia(estado_evento.as_deref(), estado_lapso.as_deref())
        .map_err(desde_dominio)?;

    let Some(actual) = queries::get(tx.as_mut(), id_event, id_student)
        .await
        .map_err(map_sqlx_err)?
    else {
        return Err(AppError::NotFound(
            "No se encontró la asistencia del estudiante en el evento".into(),
        ));
    };

    if actual.state.as_deref() == Some(estado.as_str()) {
        // Idempotente: no hubo cambio real, no se escribe ni se audita.
        return Ok(actual);
    }

    let actualizado = queries::set_state(tx.as_mut(), id_event, id_student, estado.as_str())
        .await
        .map_err(map_sqlx_err)?;

    let payload = json!({
        "estado_anterior": actual.state,
        "estado_nuevo": actualizado.state,
    });
    write_audit_log_tx(
        &mut tx,
        "attendance",
        id_event.0,
        "corregir_asistencia",
        Some(&payload),
    )
    .await?;

    tx.commit().await?;
    Ok(actualizado)
}
