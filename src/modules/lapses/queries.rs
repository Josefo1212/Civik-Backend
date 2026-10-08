//! SQL del módulo `lapses`.
//!
//! Cada función es una operación contra `lapses` y, cuando escribe, deja su
//! auditoría **en la misma transacción** (RNF-2). Las que escriben toman
//! `&mut PgTransaction` por la misma razón que en `sections`: sqlx no permite
//! reutilizar un executor genérico para dos llamadas, y acá siempre hay dos
//! (la escritura + el registro de auditoría).
//!
//! El service (`service.rs`) es quien decide el flujo y valida el estado con
//! el dominio; este archivo ejecuta el SQL y solo clasifica "no existe"
//! (`RowNotFound`) contra "sin cambios" cuando una query no toca filas.

use crate::modules::lapses::domain::DatosLapso;
use crate::shared::audit;
use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use serde_json::json;
use sqlx::{FromRow, PgExecutor, PgTransaction};
use uuid::Uuid;

/// Un lapso, en la forma exacta que devuelven las queries.
///
/// `practice_day` y `status` son `String` y no enums: sqlx no mapea enums
/// propios en sus macros de compilación, así que se leen con `::text AS` y el
/// catálogo lo garantiza el dominio (`DiaSemana`, `EstadoLapso`) sobre el
/// enum de Postgres.
#[derive(Debug, Clone, Serialize)]
pub struct Lapso {
    pub id: Uuid,
    pub school_year: i32,
    pub lapse_number: i16,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub practice_day: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// La misma fila, antes de normalizar los enums.
///
/// `practice_day::text AS practice_day` y `status::text AS status` se infieren
/// como `Option<String>` en el macro (las expresiones casteadas de una columna
/// NOT NULL se tratan como nulables); `en_lapso` recupera el `String` con el
/// mismo criterio que `FilaConShift` de `sections`.
#[derive(Debug, FromRow)]
struct FilaLapso {
    id: Uuid,
    school_year: i32,
    lapse_number: i16,
    start_date: NaiveDate,
    end_date: NaiveDate,
    practice_day: Option<String>,
    status: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl FilaLapso {
    fn en_lapso(self) -> Lapso {
        Lapso {
            id: self.id,
            school_year: self.school_year,
            lapse_number: self.lapse_number,
            start_date: self.start_date,
            end_date: self.end_date,
            practice_day: self.practice_day.unwrap_or_default(),
            status: self.status.unwrap_or_default(),
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

/// Da de alta un lapso ya validado y deja su auditoría (RF-1, RF-19).
///
/// El 23505 de `uq_lapses_year_number` y el 23P01 de `lapses_no_overlap` se
/// propagan como `sqlx::Error` y `AppError::from` los traduce a `Conflict`
/// (RF-3, RF-4); acá no se decide sobre la colisión, solo se deja subir.
pub async fn crear(tx: &mut PgTransaction<'_>, datos: DatosLapso) -> sqlx::Result<Lapso> {
    let lapso = sqlx::query_file_as!(
        FilaLapso,
        "src/modules/lapses/queries/create.sql",
        datos.school_year,
        datos.lapse_number,
        datos.start_date,
        datos.end_date,
        datos.practice_day.as_str(),
    )
    .fetch_one(&mut **tx)
    .await?
    .en_lapso();

    let payload = payload_de(&lapso);
    audit::write_audit_log_tx(tx, "lapses", lapso.id, "alta", Some(&payload)).await?;

    Ok(lapso)
}

/// Un lapso por su identificador, en cualquier estado (RF-6); `RowNotFound`
/// si no existe (RF-7).
pub async fn get_by_id<'e, E>(executor: E, id: Uuid) -> sqlx::Result<Lapso>
where
    E: PgExecutor<'e>,
{
    let fila = sqlx::query_file_as!(
        FilaLapso,
        "src/modules/lapses/queries/get_by_id.sql",
        id,
    )
    .fetch_optional(executor)
    .await?;

    let Some(fila) = fila else {
        return Err(sqlx::Error::RowNotFound);
    };

    Ok(fila.en_lapso())
}

/// Todos los lapsos en el orden canónico (RF-5): año descendente, número
/// ascendente. Sin filtros (plan §8.6).
pub async fn listar<'e, E>(executor: E) -> sqlx::Result<Vec<Lapso>>
where
    E: PgExecutor<'e>,
{
    let filas = sqlx::query_file_as!(FilaLapso, "src/modules/lapses/queries/list.sql")
        .fetch_all(executor)
        .await?;

    Ok(filas.into_iter().map(FilaLapso::en_lapso).collect())
}

/// Edita la estructura de un lapso (RF-8, RF-10) y deja su auditoría (RF-19).
///
/// `datos` llega destinado por `domain::preparar_entrada`, el mismo punto
/// único del alta: la edición valida y normaliza exactamente igual (RF-8). El
/// service ya validó que el lapso no está `CLOSED` (RF-9); el trigger
/// `trg_lapses_closed_frozen` queda como defensa. El 23505/23P01 siguen
/// trayendo el 409 de la colisión (RF-10).
pub async fn editar(
    tx: &mut PgTransaction<'_>,
    id: Uuid,
    datos: DatosLapso,
) -> sqlx::Result<Lapso> {
    let fila = sqlx::query_file_as!(
        FilaLapso,
        "src/modules/lapses/queries/update.sql",
        id,
        datos.school_year,
        datos.lapse_number,
        datos.start_date,
        datos.end_date,
        datos.practice_day.as_str(),
    )
    .fetch_optional(&mut **tx)
    .await?;

    let Some(fila) = fila else {
        return Err(sqlx::Error::RowNotFound);
    };
    let lapso = fila.en_lapso();

    let payload = payload_de(&lapso);
    audit::write_audit_log_tx(tx, "lapses", lapso.id, "editar", Some(&payload)).await?;

    Ok(lapso)
}

/// Activa un lapso (RF-12, RF-13, RF-14).
pub async fn activar(tx: &mut PgTransaction<'_>, id: Uuid) -> sqlx::Result<Lapso> {
    cambiar_estado(tx, id, "ACTIVE", "PLANNED", "activar").await
}

/// Cierra un lapso (RF-15, RF-16).
pub async fn cerrar(tx: &mut PgTransaction<'_>, id: Uuid) -> sqlx::Result<Lapso> {
    cambiar_estado(tx, id, "CLOSED", "ACTIVE", "cerrar").await
}

/// Implementación compartida de activación y cierre (RF-12 a RF-16).
///
/// `set_status.sql` solo actualiza si el estado actual es el esperado (assert);
/// cuando no devuelve filas hay dos posibilidades que la base no separa, y
/// `get_by_id` las distingue: si el lapso no existe, `RowNotFound`; si existe,
/// ya estaba en un estado que no permite la operación y se devuelve tal cual,
/// sin mover `updated_at` ni escribir una auditoría falsa — es el service el
/// que, viendo el estado real, produce el 409 entendible (el `23505` de
/// `uq_lapses_single_active` también trae su propio 409; RF-13).
async fn cambiar_estado(
    tx: &mut PgTransaction<'_>,
    id: Uuid,
    nuevo: &str,
    esperado: &str,
    accion: &str,
) -> sqlx::Result<Lapso> {
    let fila = sqlx::query_file_as!(
        FilaLapso,
        "src/modules/lapses/queries/set_status.sql",
        id,
        nuevo,
        esperado,
    )
    .fetch_optional(&mut **tx)
    .await?;

    let Some(fila) = fila else {
        return get_by_id(&mut **tx, id).await;
    };
    let lapso = fila.en_lapso();

    let payload = json!({ "estado": lapso.status });
    audit::write_audit_log_tx(tx, "lapses", lapso.id, accion, Some(&payload)).await?;

    Ok(lapso)
}

/// Elimina físicamente un lapso (RF-17, RF-18) y deja su auditoría (RF-19).
///
/// El service ya validó "no CLOSED y no terminado" (para 409 entendibles); la
/// auditoría se escribe **antes** del borrado para conservar los valores de la
/// fila que desaparece (mismo criterio que `sections::eliminar`). El 23001 de
/// `events_id_lapse_fkey`/`grade_overrides_id_lapse_fkey` sube desde el DELETE
/// y `AppError::from` lo traduce a "el lapso tiene eventos/overrides"
/// (RF-18).
pub async fn eliminar(tx: &mut PgTransaction<'_>, id: Uuid) -> sqlx::Result<Lapso> {
    let lapso = get_by_id(&mut **tx, id).await?;

    let mut payload = payload_de(&lapso);
    payload["estado"] = json!(lapso.status);
    audit::write_audit_log_tx(tx, "lapses", lapso.id, "eliminar", Some(&payload)).await?;

    let fila = sqlx::query_file_as!(FilaLapso, "src/modules/lapses/queries/delete.sql", id)
        .fetch_optional(&mut **tx)
        .await?;

    let Some(fila) = fila else {
        return Err(sqlx::Error::RowNotFound);
    };

    Ok(fila.en_lapso())
}

/// Si el lapso tiene algún dato asociado (eventos u overrides) que impida
/// borrarlo (RF-18).
///
/// Es el pre-chequeo del service sobre `has_data.sql`: espeja las condiciones
/// de `lapses_delete_guard` para responder un `409` entendible antes de que el
/// trigger lance su `23514`. La FK `events_id_lapse_fkey` (RESTRICT) queda
/// como red extra, pero por DELETE no se alcanza: el guard evalúa las mismas
/// filas primero.
pub async fn tiene_datos<'e, E>(executor: E, id_lapse: Uuid) -> sqlx::Result<bool>
where
    E: PgExecutor<'e>,
{
    let fila = sqlx::query_file!("src/modules/lapses/queries/has_data.sql", id_lapse)
        .fetch_one(executor)
        .await?;

    Ok(fila.tiene_datos.unwrap_or(false))
}

/// Cuántos eventos de `id_lapse` caerían fuera de `[inicio, fin]` (RF-11).
/// Es el pre-chequeo del service sobre `events_outside.sql`: si el resultado
/// es mayor que cero, el service rechaza el movimiento de fechas con un 409
/// entendible antes de que la base lance el `23514` de su trigger.
pub async fn eventos_fuera_del_rango<'e, E>(
    executor: E,
    id_lapse: Uuid,
    inicio: NaiveDate,
    fin: NaiveDate,
) -> sqlx::Result<i64>
where
    E: PgExecutor<'e>,
{
    let fila = sqlx::query_file!(
        "src/modules/lapses/queries/events_outside.sql",
        id_lapse,
        inicio,
        fin,
    )
    .fetch_one(executor)
    .await?;

    Ok(fila.cuantos.unwrap_or(0))
}

/// El payload de auditoría común al alta y a la edición, en claves en español
/// (igual que `sections`).
fn payload_de(lapso: &Lapso) -> serde_json::Value {
    json!({
        "anio_escolar": lapso.school_year,
        "numero_lapso": lapso.lapse_number,
        "inicio": lapso.start_date,
        "fin": lapso.end_date,
        "dia_practica": lapso.practice_day,
    })
}