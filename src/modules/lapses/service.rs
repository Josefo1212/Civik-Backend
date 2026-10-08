//! Orquestación del módulo `lapses`: el service decide el flujo y es dueño de
//! la transacción.
//!
//! A diferencia de `sections` y `students` (CRUD directo), `lapses` tiene una
//! regla de negocio real que proteger (el ciclo `PLANNED → ACTIVE → CLOSED`)
//! y validaciones que cruzan otras tablas (RF-11: un movimiento de fechas no
//! puede dejar eventos fuera del lapso), así que la secuencia vive acá, no en
//! el handler: histórico descrito en el plan §4 y en `AGENTS.md` ("hexagonal
//! proporcional").
//!
//! Cada operación abre una transacción, lee/valida/escribe dentro de ella y la
//! confirma al final; si cualquier `?` falla a mitad, el `Transaction` se
//! descarta (rollback implícito) y una posible auditoría ya escrita se va con
//! él. Eso da la misma atomicidad que tenían las funciones PL/pgSQL que se
//! portaron al dominio de Rust (RNF-2).
//!
//! La regla de estado la valida el dominio (`domain::validar_transicion`) para
//! obtener el mensaje en español del `409`; la base la re-valida con sus
//! triggers como defensa (probados con SQL directo en conflictos_bd, T8).

use crate::modules::lapses::domain::{
    preparar_entrada, validar_borrado_permitido, validar_transicion, EstadoLapso, Operacion,
};
use crate::modules::lapses::queries::{self, Lapso};
use crate::shared::error::AppError;
use chrono::Utc;
use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;

/// El cuerpo del alta y de la edición, en la forma que manda el cliente.
///
/// Vive en `service` y no en `handler` (como `EntradaSeccion`) porque acá es
/// `service` el que aplica `domain::preparar_entrada`, el único punto de
/// validación (RF-2, RF-8): el handler solo deserializa y delega.
#[derive(Deserialize)]
pub struct EntradaLapso {
    pub school_year: String,
    pub lapse_number: String,
    pub start_date: String,
    pub end_date: String,
    pub practice_day: String,
}

/// Un estado de la base que no corresponda a ninguna variante del catálogo.
///
/// Es un caso inalcanzable con la base sana (la columna está restringida al
/// enum `lapse_status`), pero `EstadoLapso::parse` lo deja explícito como
/// `None` y el `deny` de `lib.rs` no permite un `unwrap`: se responde un 409
/// honesto en vez de dejar que el error suba como un 500 confuso.
fn estado_de(lapso: &Lapso) -> Result<EstadoLapso, AppError> {
    EstadoLapso::parse(&lapso.status).ok_or_else(|| {
        AppError::Conflict(format!(
            "El lapso está en un estado desconocido: «{}»",
            lapso.status
        ))
    })
}

/// Convierte los errores puros del dominio (transición de estado, borrado) en
/// un `409 Conflict` con su mensaje en español (RF-9, RF-14, RF-16, RF-18).
fn dominio_a_conflict(error: crate::modules::lapses::domain::ErrorDominio) -> AppError {
    AppError::Conflict(error.to_string())
}

/// Convierte los errores de la entrada (año, número, fechas, día) en un `400`
/// con su mensaje en español (RF-2).
fn dominio_a_bad_request(error: crate::modules::lapses::domain::ErrorDominio) -> AppError {
    AppError::BadRequest(error.to_string())
}

/// Una fila que no existe es un `404`, no un `500`: se traduce antes de que el
/// `?` lo envuelva como `Database` (RF-7).
fn a_not_found(error: sqlx::Error) -> AppError {
    match error {
        sqlx::Error::RowNotFound => AppError::NotFound("No se encontró el lapso".into()),
        otro => otro.into(),
    }
}

/// Alta de un lapso (RF-1 a RF-4, RF-19).
///
/// `preparar_entrada` valida y normaliza los cinco campos (RF-2); la
/// transacción garantiza que lapso y auditoría se confirman juntos. El `23505`
/// de `uq_lapses_year_number` y el `23P01` de `lapses_no_overlap` suben desde
/// `queries::crear` y `AppError::from` los traduce a `Conflict` (RF-3, RF-4).
pub async fn crear(pool: &PgPool, entrada: &EntradaLapso) -> Result<Lapso, AppError> {
    let datos = preparar_entrada(
        &entrada.school_year,
        &entrada.lapse_number,
        &entrada.start_date,
        &entrada.end_date,
        &entrada.practice_day,
    )
    .map_err(dominio_a_bad_request)?;

    let mut tx = pool.begin().await?;
    let lapso = queries::crear(&mut tx, datos).await?;
    tx.commit().await?;

    Ok(lapso)
}

/// Edición estructural de un lapso (RF-8 a RF-11, RF-19).
///
/// Secuencia del plan §4: leer → validar transición (no `CLOSED`, RF-9) →
/// validar fechas contra los eventos (RF-11) → editar. Las reglas de colisión
/// son las mismas del alta porque la edición usa el mismo `preparar_entrada`
/// (RF-8) y las mismas restricciones de la base (RF-10), solo que con el
/// estado del lapso re-validado primero.
pub async fn editar(
    pool: &PgPool,
    id: Uuid,
    entrada: &EntradaLapso,
) -> Result<Lapso, AppError> {
    let datos = preparar_entrada(
        &entrada.school_year,
        &entrada.lapse_number,
        &entrada.start_date,
        &entrada.end_date,
        &entrada.practice_day,
    )
    .map_err(dominio_a_bad_request)?;

    let mut tx = pool.begin().await?;

    let lapso = queries::get_by_id(tx.as_mut(), id).await.map_err(a_not_found)?;
    let estado = estado_de(&lapso)?;
    validar_transicion(&estado, Operacion::Editar).map_err(dominio_a_conflict)?;

    // RF-11: el trigger de la base lo rechaza con 23514; el service lo
    // anticipa para responder el 409 con un mensaje que nombra el problema.
    if queries::eventos_fuera_del_rango(tx.as_mut(), id, datos.start_date, datos.end_date).await? > 0
    {
        return Err(AppError::Conflict(
            "No se puede editar: las fechas nuevas dejarían eventos fuera del lapso".to_string(),
        ));
    }

    let actualizado = queries::editar(&mut tx, id, datos).await.map_err(a_not_found)?;
    tx.commit().await?;

    Ok(actualizado)
}

/// Activa un lapso (RF-12 a RF-14, RF-19).
///
/// Solo desde `PLANNED` (RF-14); el `23505` de `uq_lapses_single_active`
/// convierte el segundo activo en un `Conflict` (RF-13).
pub async fn activar(pool: &PgPool, id: Uuid) -> Result<Lapso, AppError> {
    let mut tx = pool.begin().await?;

    let lapso = queries::get_by_id(tx.as_mut(), id).await.map_err(a_not_found)?;
    let estado = estado_de(&lapso)?;
    validar_transicion(&estado, Operacion::Activar).map_err(dominio_a_conflict)?;

    let activo = queries::activar(&mut tx, id).await.map_err(a_not_found)?;
    tx.commit().await?;

    Ok(activo)
}

/// Cierra un lapso (RF-15, RF-16, RF-19).
///
/// Solo desde `ACTIVE` (RF-16); un `CLOSED` es terminal (RF-15) porque
/// `trg_lapses_closed_frozen` impide cualquier UPDATE posterior, incluida una
/// segunda transición.
pub async fn cerrar(pool: &PgPool, id: Uuid) -> Result<Lapso, AppError> {
    let mut tx = pool.begin().await?;

    let lapso = queries::get_by_id(tx.as_mut(), id).await.map_err(a_not_found)?;
    let estado = estado_de(&lapso)?;
    validar_transicion(&estado, Operacion::Cerrar).map_err(dominio_a_conflict)?;

    let cerrado = queries::cerrar(&mut tx, id).await.map_err(a_not_found)?;
    tx.commit().await?;

    Ok(cerrado)
}

/// Elimina físicamente el lapso (RF-17, RF-18, RF-19).
///
/// La única excepción constitucional a "nada se borra": vacío, no `CLOSED` y
/// no terminado. El service valida la transición (`no CLOSED`), la condición
/// temporal del guardarraíl (`validar_borrado_permitido`, con la fecha de hoy
/// inyectada para que el dominio siga siendo puro) y anticipa RF-18 con
/// `tiene_datos`: si el lapso tiene eventos u overrides responde el `409`
/// entendible. La auditoría se escribe en la misma transacción, antes del
/// borrado (`queries::eliminar`).
///
/// El pre-chequeo espeja las condiciones de `lapses_delete_guard` porque ese
/// trigger dispara su `23514` ANTES de que las FKs RESTRICT puedan respon
/// der: por DELETE, el código `23001` de `events_id_lapse_fkey` nunca se
/// alcanza (el guard evalúa las mismas filas primero y aborta). El guardarraíl
/// y las FKs siguen siendo la defensa si el service falla.
pub async fn eliminar(pool: &PgPool, id: Uuid) -> Result<(), AppError> {
    let mut tx = pool.begin().await?;

    let lapso = queries::get_by_id(tx.as_mut(), id).await.map_err(a_not_found)?;
    let estado = estado_de(&lapso)?;
    validar_transicion(&estado, Operacion::Eliminar).map_err(dominio_a_conflict)?;
    validar_borrado_permitido(lapso.end_date, Utc::now().date_naive())
        .map_err(dominio_a_conflict)?;

    if queries::tiene_datos(tx.as_mut(), id).await? {
        return Err(AppError::Conflict(
            "No se puede borrar: el lapso tiene eventos u overrides asociados".to_string(),
        ));
    }

    queries::eliminar(&mut tx, id).await.map_err(a_not_found)?;
    tx.commit().await?;

    Ok(())
}