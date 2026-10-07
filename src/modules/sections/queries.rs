use crate::modules::sections::domain::{DatosSeccion, Grado};
use crate::shared::audit;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::json;
use sqlx::{FromRow, PgExecutor, PgTransaction};
use uuid::Uuid;

/// Una fila de `sections`, en la forma exacta que devuelven las queries.
///
/// `grade_level`, `section` y `shift` llegan ya canónicos por `domain.rs`
/// (`1ro`/`a`/`MATUTINO`). `shift` es un `String` y no el enum de la base
/// (`shift_type`): sqlx no mapea enums propios en sus macros de compilación,
/// así que el catálogo lo garantiza el dominio y el enum de Postgres, y la
/// columna se lee como texto (ver `create.sql`).
#[derive(Debug, Clone, Serialize)]
pub struct Seccion {
    pub id: Uuid,
    pub grade_level: String,
    pub section: String,
    pub shift: String,
    pub active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Las seis columnas base de `RETURNING` en `create.sql` y `update.sql`.
///
/// Separada de `Seccion` a propósito: el `RETURNING` no incluye `shift` (el
/// enum y los casts se infieren como nulables en el macro de sqlx), así que la
/// fila sale de la base sin esa columna y `crear`/`editar` la rearman.
#[derive(Debug, FromRow)]
struct FilaBase {
    id: Uuid,
    grade_level: String,
    section: String,
    active: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

/// Una fila de `sections` con `shift` ya en texto (`get_by_id.sql` y
/// `set_active.sql`), para las lecturas donde Rust no conoce el turno.
///
/// `shift` sale de `shift::text AS shift` y sqlx infiere las expresiones
/// casteadas como nulables aunque la columna sea `NOT NULL`: por eso el campo
/// es `Option<String>`. En la práctica nunca llega `None` — el esquema lo
/// impide — y `en_seccion` lo normaliza a un `String` vacío por si acaso.
#[derive(Debug, FromRow)]
struct FilaConShift {
    id: Uuid,
    grade_level: String,
    section: String,
    shift: Option<String>,
    active: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl FilaConShift {
    fn en_seccion(self) -> Seccion {
        Seccion {
            id: self.id,
            grade_level: self.grade_level,
            section: self.section,
            shift: self.shift.unwrap_or_default(),
            active: self.active,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

/// Da de alta una sección ya normalizada y deja su entrada de auditoría.
///
/// Toma `&mut PgTransaction` y no un `PgExecutor` genérico a propósito: la
/// operación debe escribir la sección **y su auditoría** en el mismo
/// `BEGIN/COMMIT` (RNF-2), y `sqlx` no permite reutilizar un executor genérico
/// para dos llamadas — solo una transacción se puede pasar dos veces. El test
/// le pasa `&mut tx` de su transacción descartable, y el handler envolverá con
/// `pool.transaction()`.
///
/// El `23505` de `uq_sections_grade_section_shift` se propaga como
/// `sqlx::Error` y `AppError::from` lo traduce a `Conflict` (409); acá no se
/// decide sobre el duplicado, solo se deja subir.
pub async fn crear(tx: &mut PgTransaction<'_>, datos: DatosSeccion) -> sqlx::Result<Seccion> {
    // El RETURNING no trae `shift`: viene del enum shift_type y las expresiones
    // casteadas se infieren como nulables en el macro. Postgres ya validó el
    // literal contra el enum al insertar, y el valor es el que normalizó el
    // dominio, así que se rearma la fila con él.
    let base = sqlx::query_file_as!(
        FilaBase,
        "src/modules/sections/queries/create.sql",
        datos.grado.as_str(),
        &datos.seccion,
        datos.turno.as_str(),
    )
    .fetch_one(&mut **tx)
    .await?;

    let fila = Seccion {
        id: base.id,
        grade_level: base.grade_level,
        section: base.section,
        shift: datos.turno.as_str().to_string(),
        active: base.active,
        created_at: base.created_at,
        updated_at: base.updated_at,
    };

    let payload = json!({
        "grado": datos.grado.as_str(),
        "seccion": datos.seccion,
        "turno": datos.turno.as_str(),
    });
    audit::write_audit_log_tx(tx, "sections", fila.id, "alta", Some(&payload)).await?;

    Ok(fila)
}

/// Edita los datos de identidad de una sección y deja su auditoría en la misma
/// transacción (RF-27, RNF-2).
///
/// `datos` llega ya destinado por `domain::preparar_entrada`, el mismo punto
/// único de normalización que usa el alta: por eso la edición normaliza igual
/// que el alta (RF-14) y rechaza lo mismo (vacío / fuera de catálogo).
///
/// Si el `UPDATE` no toca ninguna fila, la sección no existe
/// (`sqlx::Error::RowNotFound`, que el handler traducirá a 404). El `23505`
/// sobre `uq_sections_grade_section_shift` sube tal cual y `AppError::from` lo
/// traduce a `Conflict` (RF-17).
pub async fn editar(
    tx: &mut PgTransaction<'_>,
    id: Uuid,
    datos: DatosSeccion,
) -> sqlx::Result<Seccion> {
    // El RETURNING no trae `shift` (mismo criterio que `crear`): la fila se
    // rearma con el turno ya normalizado.
    let base = sqlx::query_file_as!(
        FilaBase,
        "src/modules/sections/queries/update.sql",
        id,
        datos.grado.as_str(),
        &datos.seccion,
        datos.turno.as_str(),
    )
    .fetch_optional(&mut **tx)
    .await?;

    let Some(base) = base else {
        return Err(sqlx::Error::RowNotFound);
    };

    let fila = Seccion {
        id: base.id,
        grade_level: base.grade_level,
        section: base.section,
        shift: datos.turno.as_str().to_string(),
        active: base.active,
        created_at: base.created_at,
        updated_at: base.updated_at,
    };

    let payload = json!({
        "grado": datos.grado.as_str(),
        "seccion": datos.seccion,
        "turno": datos.turno.as_str(),
    });
    audit::write_audit_log_tx(tx, "sections", fila.id, "editar", Some(&payload)).await?;

    Ok(fila)
}

/// Una sección por su identificador, activa o dada de baja (RF-12).
///
/// Acepta un `PgExecutor` genérico porque hace una sola consulta y no lo reusa
/// para una segunda llamada — el criterio que en `crear` obliga a
/// `&mut PgTransaction` (el bloqueo de sqlx es reutilizar un executor para dos
/// llamadas, no tomarlo prestado una vez). Si no existe, `RowNotFound`.
pub async fn get_by_id<'e, E>(executor: E, id: Uuid) -> sqlx::Result<Seccion>
where
    E: PgExecutor<'e>,
{
    let base = sqlx::query_file_as!(
        FilaConShift,
        "src/modules/sections/queries/get_by_id.sql",
        id,
    )
    .fetch_optional(executor)
    .await?;

    let Some(base) = base else {
        return Err(sqlx::Error::RowNotFound);
    };

    Ok(base.en_seccion())
}

/// Marca una sección como activa o inactiva (RF-18/RF-20), de forma
/// idempotente (RF-19/RF-21).
///
/// `set_active.sql` solo actualiza si cambia el estado (mérito de
/// `IS DISTINCT FROM`); cuando no devuelve filas hay dos posibilidades que la
/// base no separa, y `get_by_id` las distingue: si la sección no existe,
/// devuelve `RowNotFound`; si existe, ya estaba en el estado pedido y se
/// devuelve tal cual, sin mover `updated_at`.
///
/// La auditoría (RF-27) se escribe únicamente cuando el estado cambió de
/// verdad: una llamada repetida "no modifica nada" (texto literal de RF-19 y
/// RF-21) y no debe dejar una entrada falsa en el historial.
pub async fn set_active(
    tx: &mut PgTransaction<'_>,
    id: Uuid,
    activo: bool,
) -> sqlx::Result<Seccion> {
    let base = sqlx::query_file_as!(
        FilaConShift,
        "src/modules/sections/queries/set_active.sql",
        id,
        activo,
    )
    .fetch_optional(&mut **tx)
    .await?;

    let Some(base) = base else {
        // El UPDATE no tocó ninguna fila: o la sección ya estaba en el estado
        // pedido o no existe. `get_by_id` deja que el llamador distinga.
        return get_by_id(&mut **tx, id).await;
    };

    let fila = base.en_seccion();
    let accion = if activo { "reactivar" } else { "baja" };
    let payload = json!({ "activo": activo });
    audit::write_audit_log_tx(tx, "sections", fila.id, accion, Some(&payload)).await?;

    Ok(fila)
}

/// Elimina físicamente una sección **solo si no tiene estudiantes** (RF-22 y
/// RF-23) y deja su auditoría en la misma transacción (RF-27, RNF-2).
///
/// La auditoría se escribe **antes** del borrado, para conservar los valores
/// de la fila que desaparece; si el `DELETE` falla o la transacción se aborta,
/// el registro se descarta junto con ella y no queda una "eliminación"
/// fantasma.
///
/// Si la sección no existe, `get_by_id` devuelve `RowNotFound`. El `23503` de
/// `students_id_section_fkey` sube desde el `DELETE` y `AppError::from` lo
/// traduce a `Conflict` — "la sección tiene estudiantes" — sin importar si el
/// estudiante está activo o inactivo. No hay filtro por `active`: una sección
/// dada de baja que nunca tuvo estudiantes también se elimina.
pub async fn eliminar(tx: &mut PgTransaction<'_>, id: Uuid) -> sqlx::Result<Seccion> {
    let fila = get_by_id(&mut **tx, id).await?;

    let payload = json!({
        "grado": fila.grade_level,
        "seccion": fila.section,
        "turno": fila.shift,
        "activo": fila.active,
    });
    audit::write_audit_log_tx(tx, "sections", fila.id, "eliminar", Some(&payload)).await?;

    // El RETURNING confirma que el borrado ocurrió de verdad; el 23503 se
    // propaga acá, nunca llega un `None` acompañado de fila viva.
    let base = sqlx::query_file_as!(FilaConShift, "src/modules/sections/queries/delete.sql", id,)
        .fetch_optional(&mut **tx)
        .await?;

    let Some(base) = base else {
        return Err(sqlx::Error::RowNotFound);
    };

    Ok(base.en_seccion())
}

/// Lista las secciones activas en el orden canónico de la spec (RF-9 a RF-11,
/// RF-24): `1ro`→`5to` y, dentro de cada grado, la subdivisión en orden
/// alfabético. `grado` filtra por uno solo (RF-10).
///
/// El array del `ORDER BY` de `list_active.sql` se construye acá desde
/// `Grado::TODOS` (una sola fuente de verdad para el orden); pasarlo como
/// `None` mantiene el orden canónico y deja fuera el filtro.
pub async fn listar_activas(
    pool: impl PgExecutor<'_>,
    grado: Option<Grado>,
) -> sqlx::Result<Vec<Seccion>> {
    let orden_canonico: Vec<String> = Grado::TODOS
        .iter()
        .map(|grado| grado.as_str().to_string())
        .collect();

    let filas = sqlx::query_file_as!(
        FilaConShift,
        "src/modules/sections/queries/list_active.sql",
        grado.map(|grado| grado.as_str()),
        &orden_canonico,
    )
    .fetch_all(pool)
    .await?;

    Ok(filas.into_iter().map(FilaConShift::en_seccion).collect())
}
