use sqlx::PgPool;
use uuid::Uuid;

use crate::modules::events::queries as events_queries;
use crate::modules::students::queries as students_queries;
use crate::shared::audit::write_audit_log as write_audit;
use crate::shared::error::AppError;

use super::queries;

fn a_not_found(error: sqlx::Error) -> AppError {
    match error {
        sqlx::Error::RowNotFound => AppError::NotFound("Recurso no encontrado".into()),
        _ => AppError::Database(error),
    }
}

fn map_sqlx_err(error: sqlx::Error) -> AppError {
    use crate::shared::error::mapear_violacion;
    if let sqlx::Error::Database(db_err) = &error {
        if let Some(app_err) = mapear_violacion(db_err.code().as_deref(), db_err.constraint()) {
            return app_err;
        }
    }
    AppError::Database(error)
}

#[derive(serde::Deserialize, Debug)]
pub struct Item {
    pub id_student: Uuid,
    pub state: String,
}

#[derive(serde::Deserialize, Debug)]
pub struct RegistroLote {
    pub items: Vec<Item>,
}

#[derive(serde::Serialize, Debug)]
pub struct ResultadoRegistro {
    pub registrados: usize,
}

pub async fn registrar_listado(
    pool: &PgPool,
    id_event: Uuid,
    lote: RegistroLote,
) -> Result<ResultadoRegistro, AppError> {
    let mut tx = pool.begin().await?;
    let evento = events_queries::get_by_id(tx.as_mut(), id_event)
        .await
        .map_err(a_not_found)?;

    if evento.status.as_deref() == Some("CANCELLED") {
        return Err(AppError::Conflict(
            "No se puede registrar asistencia en un evento CANCELLED".into(),
        ));
    }

    let mut registrados = 0;
    for item in lote.items {
        // Verificar estudiante existe
        let _est = students_queries::consultar(tx.as_mut(), item.id_student).await;
        match _est {
            Ok(_) => {}
            Err(_) => return Err(AppError::NotFound("Estudiante no encontrado".into())),
        }

        match queries::insert(tx.as_mut(), id_event, item.id_student, &item.state).await {
            Ok(att) => {
                let _ = write_audit(
                    tx.as_mut(),
                    "attendance",
                    att.id_event,
                    "create",
                    Some(&serde_json::json!({"state": att.state})),
                )
                .await;
                registrados += 1;
            }
            Err(e) => return Err(map_sqlx_err(e)),
        }
    }
    tx.commit().await?;
    Ok(ResultadoRegistro { registrados })
}

pub async fn corregir_asistencia(
    pool: &PgPool,
    id_event: Uuid,
    id_student: Uuid,
    state: &str,
) -> Result<super::queries::Attendance, AppError> {
    let mut tx = pool.begin().await?;
    let evento = events_queries::get_by_id(tx.as_mut(), id_event)
        .await
        .map_err(a_not_found)?;
    if evento.status.as_deref() == Some("CANCELLED") {
        return Err(AppError::Conflict(
            "No se puede registrar asistencia en un evento CANCELLED".into(),
        ));
    }

    match queries::set_state(tx.as_mut(), id_event, id_student, state).await {
        Ok(att) => {
            let _ = write_audit(
                tx.as_mut(),
                "attendance",
                att.id_event,
                "update",
                Some(&serde_json::json!({"state": att.state})),
            )
            .await;
            tx.commit().await?;
            Ok(att)
        }
        Err(e) => {
            tx.rollback().await.ok();
            Err(map_sqlx_err(e))
        }
    }
}
