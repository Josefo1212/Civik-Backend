use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Error de base de datos: {0}")]
    Database(sqlx::Error),

    // El mensaje distingue qué recurso no se encontró (sección, estudiante):
    // un `404` mudo no le dice al instructor qué corregir (RNF-4).
    #[error("{0}")]
    NotFound(String),

    #[error("Solicitud inválida: {0}")]
    BadRequest(String),

    #[error("{0}")]
    Conflict(String),
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_msg) = match &self {
            AppError::Database(_) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
            AppError::NotFound(_) => (StatusCode::NOT_FOUND, self.to_string()),
            AppError::BadRequest(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            AppError::Conflict(_) => (StatusCode::CONFLICT, self.to_string()),
        };

        let body = Json(ErrorResponse { error: error_msg });

        (status, body).into_response()
    }
}

/// Traduce un código de error de Postgres **junto con su restricción** a un
/// `409 Conflict`.
///
/// El código solo no alcanza: la init tiene cinco restricciones UNIQUE (tras
/// retirar `uq_students_national_id`) y varias clave foránea, y el mismo
/// `23505` significa "combinación duplicada" sobre
/// `uq_sections_grade_section_shift` pero "id repetido" sobre `sections_pkey`.
/// Mapear solo el código devolvería un 409 con un mensaje falso.
///
/// La clave foránea de `students` aparece en el mapa con **dos códigos**: un
/// `DELETE` que toca una FK `ON DELETE RESTRICT` emite `23001`
/// (restrict_violation), mientras que un `INSERT` con el padre inexistente
/// emite `23503` (foreign_key_violation). El borrado de secciones (RF-23 de
/// la 002) cae en el primer caso; se conserva el segundo por si alguna vez una
/// FK cambia a `NO ACTION`.
///
/// `pub(crate)` y no `pub` a propósito: el mapeo es un detalle de la capa de
/// código de Postgres, y solo los tests de ejecución (que generan el error
/// real) y los unit tests de este mismo archivo necesitan verlo.
///
/// Devuelve `None` para todo lo demás, y `From<sqlx::Error>` deja el error
/// como `Database` (500): los errores de infraestructura no se traducen.
pub(crate) fn mapear_violacion(
    codigo: Option<&str>,
    restriccion: Option<&str>,
) -> Option<AppError> {
    match (codigo, restriccion) {
        (Some("23505"), Some("uq_sections_grade_section_shift")) => Some(AppError::Conflict(
            "Esa combinación de grado, subdivisión y turno ya está en uso".to_string(),
        )),
        (Some("23001") | Some("23503"), Some("students_id_section_fkey")) => Some(
            AppError::Conflict("La sección tiene estudiantes".to_string()),
        ),
        _ => None,
    }
}

impl From<sqlx::Error> for AppError {
    fn from(error: sqlx::Error) -> Self {
        let conflicto = match &error {
            sqlx::Error::Database(base) => {
                let codigo = base.code().map(|codigo| codigo.into_owned());
                mapear_violacion(codigo.as_deref(), base.constraint())
            }
            _ => None,
        };

        conflicto.unwrap_or(AppError::Database(error))
    }
}

// Los tests de unidad pueden usar `unwrap()`: el `deny` de `lib.rs` protege
// el código de producción, y en un assert un panic es el fallo esperado.
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    // --- El código de Postgres sin su restricción no basta ---
    //
    // La init tiene cinco UNIQUE y varias FK: el mismo `23505` significa
    // "combinación duplicada" sobre `uq_sections_grade_section_shift` y
    // "id repetido" sobre `sections_pkey`. Cruzar solo el código devolvería
    // 409 con un mensaje falso.

    #[test]
    fn duplicado_de_seccion_es_conflict() {
        let error =
            mapear_violacion(Some("23505"), Some("uq_sections_grade_section_shift")).unwrap();

        assert!(matches!(error, AppError::Conflict(_)));
        let mensaje = error.to_string();
        assert!(mensaje.contains("uso"), "mensaje: {mensaje}");
    }

    #[test]
    fn duplicado_de_llave_primaria_no_es_conflict() {
        assert!(mapear_violacion(Some("23505"), Some("sections_pkey")).is_none());
        assert!(mapear_violacion(Some("23505"), Some("uq_events_auto_one_per_day")).is_none());
    }

    #[test]
    fn borrar_seccion_con_estudiantes_es_conflict() {
        // `23001` (RESTRICT sobre DELETE) es el código real del borrado;
        // `23503` (NO ACTION) el de un INSERT con la sección inexistente.
        for codigo in ["23001", "23503"] {
            let error = mapear_violacion(Some(codigo), Some("students_id_section_fkey")).unwrap();
            assert!(matches!(error, AppError::Conflict(_)));
            let mensaje = error.to_string();
            assert!(mensaje.contains("estudiantes"), "mensaje: {mensaje}");
        }
    }

    #[test]
    fn otra_clave_foranea_no_es_conflict() {
        assert!(mapear_violacion(Some("23503"), Some("attendance_id_event_fkey")).is_none());
        assert!(mapear_violacion(Some("23503"), Some("attendance_id_student_fkey")).is_none());
    }

    #[test]
    fn codigos_de_otra_naturaleza_no_mapean() {
        assert!(mapear_violacion(Some("23502"), Some("uq_sections_grade_section_shift")).is_none());
        assert!(mapear_violacion(None, Some("uq_sections_grade_section_shift")).is_none());
        assert!(mapear_violacion(Some("23505"), None).is_none());
        assert!(mapear_violacion(None, None).is_none());
    }

    // --- Los mensajes están en español y no exponen detalle técnico ---

    #[test]
    fn el_not_found_dice_que_recurso_falto() {
        let error = AppError::NotFound("No se encontró el estudiante".into());
        let mensaje = error.to_string();

        assert!(
            mensaje.contains("estudiante"),
            "el 404 debe nombrar el recurso: {mensaje}"
        );
        assert_eq!(error.into_response().status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn los_mensajes_de_conflict_estan_en_espanol() {
        let duplicado =
            mapear_violacion(Some("23505"), Some("uq_sections_grade_section_shift")).unwrap();
        let con_estudiantes =
            mapear_violacion(Some("23503"), Some("students_id_section_fkey")).unwrap();

        assert!(
            !duplicado.to_string().contains("23505"),
            "expone código: {duplicado}"
        );
        assert!(
            !con_estudiantes.to_string().contains("23503"),
            "expone código: {con_estudiantes}"
        );
    }

    // --- Códigos de estado (plan §7) ---

    #[test]
    fn conflict_responde_409() {
        let respuesta = AppError::Conflict("ya está en uso".into()).into_response();

        assert_eq!(respuesta.status(), StatusCode::CONFLICT);
    }

    #[test]
    fn los_demas_errores_conservan_su_codigo() {
        assert_eq!(
            AppError::NotFound("No se encontró la sección".into())
                .into_response()
                .status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            AppError::BadRequest("falta el grado".into())
                .into_response()
                .status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            AppError::Database(sqlx::Error::PoolTimedOut)
                .into_response()
                .status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}
