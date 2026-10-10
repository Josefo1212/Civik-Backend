//! Acceso a datos de `grades`: una función por `.sql`, verificado en
//! compilación (constitución nº 1). El adaptador (`adapter_pg.rs`) delega el
//! SQL acá; tener las funciones sobre un `PgExecutor` genérico permite además
//! probarlas dentro de la transacción descartable de los tests (patrón
//! transacción/rollback), cosa que un adaptador atado al pool no permitiría.
//!
//! Los agregados (`COUNT`) los infiere nulables el macro, así que se leen con
//! `query_file!` y se desenvuelven con `unwrap_or(0)`: el `FILTER` nunca
//! devuelve `NULL`.

use sqlx::PgExecutor;
use uuid::Uuid;

use super::ports::{AlumnoSeccion, AusentesEstudiante};

/// ¿Existe el lapso? (RF-10).
pub async fn existe_lapso<'e, E>(executor: E, id_lapse: Uuid) -> sqlx::Result<bool>
where
    E: PgExecutor<'e>,
{
    let fila = sqlx::query_file!("src/modules/grades/queries/existe_lapso.sql", id_lapse)
        .fetch_one(executor)
        .await?;
    Ok(fila.existe.unwrap_or(false))
}

/// ¿Existe la sección? (RF-11).
pub async fn existe_seccion<'e, E>(executor: E, id_section: Uuid) -> sqlx::Result<bool>
where
    E: PgExecutor<'e>,
{
    let fila = sqlx::query_file!("src/modules/grades/queries/existe_seccion.sql", id_section)
        .fetch_one(executor)
        .await?;
    Ok(fila.existe.unwrap_or(false))
}

/// N: cuántos eventos elegibles tiene el lapso (RF-3, RF-4).
pub async fn n_eventos_elegibles<'e, E>(executor: E, id_lapse: Uuid) -> sqlx::Result<i64>
where
    E: PgExecutor<'e>,
{
    let fila = sqlx::query_file!(
        "src/modules/grades/queries/n_eventos_elegibles.sql",
        id_lapse
    )
    .fetch_one(executor)
    .await?;
    Ok(fila.total.unwrap_or(0))
}

/// Los ausentes por estudiante y por tipo (RNF-4).
pub async fn ausentes_por_estudiante<'e, E>(
    executor: E,
    id_lapse: Uuid,
) -> sqlx::Result<Vec<AusentesEstudiante>>
where
    E: PgExecutor<'e>,
{
    let filas = sqlx::query_file!(
        "src/modules/grades/queries/ausentes_por_estudiante.sql",
        id_lapse
    )
    .fetch_all(executor)
    .await?;

    Ok(filas
        .into_iter()
        .map(|fila| AusentesEstudiante {
            id_student: fila.id_student,
            ausentes_regular: fila.ausentes_regular.unwrap_or(0),
            ausentes_extra: fila.ausentes_extra.unwrap_or(0),
            ausentes_presentacion: fila.ausentes_presentacion.unwrap_or(0),
        })
        .collect())
}

/// El roster de la sección en el lapso, con override y orden canónico (RF-9).
pub async fn roster<'e, E>(
    executor: E,
    id_lapse: Uuid,
    id_section: Uuid,
) -> sqlx::Result<Vec<AlumnoSeccion>>
where
    E: PgExecutor<'e>,
{
    let filas = sqlx::query_file!(
        "src/modules/grades/queries/roster.sql",
        id_lapse,
        id_section
    )
    .fetch_all(executor)
    .await?;

    Ok(filas
        .into_iter()
        .map(|fila| AlumnoSeccion {
            id_student: fila.id_student,
            first_name: fila.first_name,
            last_name: fila.last_name,
            override_value: fila.override_value,
        })
        .collect())
}
