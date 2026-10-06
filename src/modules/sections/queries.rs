use crate::modules::sections::domain::DatosSeccion;
use crate::shared::audit;
use chrono::{DateTime, Utc};
use serde_json::json;
use sqlx::{FromRow, PgTransaction};
use uuid::Uuid;

/// Una fila de `sections`, en la forma exacta que devuelven las queries.
///
/// `grade_level`, `section` y `shift` llegan ya canónicos por `domain.rs`
/// (`1ro`/`a`/`MATUTINO`). `shift` es un `String` y no el enum de la base
/// (`shift_type`): sqlx no mapea enums propios en sus macros de compilación,
/// así que el catálogo lo garantiza el dominio y el enum de Postgres, y la
/// columna se lee como texto (ver `create.sql`).
#[derive(Debug, Clone)]
pub struct Seccion {
    pub id: Uuid,
    pub grade_level: String,
    pub section: String,
    pub shift: String,
    pub active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Las seis columnas base de `RETURNING` en `create.sql`.
///
/// Separada de `Seccion` a propósito: el `RETURNING` no incluye `shift` (el
/// enum y los casts se infieren como nulables en el macro de sqlx), así que la
/// fila sale de la base sin esa columna y `crear` la rearma.
#[derive(Debug, FromRow)]
struct FilaBase {
    id: Uuid,
    grade_level: String,
    section: String,
    active: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
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