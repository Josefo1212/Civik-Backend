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
/// El código solo no alcanza: la base tiene varias restricciones UNIQUE y
/// FKs, y el *mismo* `23505` significa "combinación duplicada" sobre
/// `uq_sections_grade_section_shift`, "lapso repetido" sobre
/// `uq_lapses_year_number` y "segundo lapso activo" sobre el índice parcial
/// `uq_lapses_single_active`. El solape de lapsos ni siquiera es `23505`: un
/// índice `EXCLUDE` emite `23P01` (exclusion_violation). Mapear solo el
/// código devolvería un 409 con un mensaje falso, o un 500 donde hay un
/// 409 esperado.
///
/// La clave foránea de `students` aparece en el mapa con **dos códigos**: un
/// `DELETE` que toca una FK `ON DELETE RESTRICT` emite `23001`
/// (restrict_violation), mientras que un `INSERT` con el padre inexistente
/// emite `23503` (foreign_key_violation). El borrado de secciones (RF-23 de
/// la 002) cae en el primer caso; se conserva el segundo por si alguna vez una
/// FK cambia a `NO ACTION`. Lo mismo aplica a las dos FKs de `lapses`
/// (`events` y `grade_overrides`).
///
/// Los triggers de la init que rechazan con `23514` (check_violation) **no**
/// se mapean: no llevan nombre de constraint y son la defensa final que solo
/// se alcanza si el servidor falla antes; se prueban con SQL directo en
/// `tests/conflictos_bd.rs`, no en la API.
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
        (Some("23505"), Some("uq_lapses_year_number")) => Some(AppError::Conflict(
            "Ya existe un lapso para ese año escolar y número".to_string(),
        )),
        (Some("23P01"), Some("lapses_no_overlap")) => Some(AppError::Conflict(
            "Las fechas se solapan con las de otro lapso".to_string(),
        )),
        (Some("23505"), Some("uq_lapses_single_active")) => Some(AppError::Conflict(
            "Ya hay otro lapso activo: el sistema solo admite uno".to_string(),
        )),
        (Some("23505"), Some("uq_events_auto_one_per_day")) => Some(AppError::Conflict(
            "Ya existe una práctica regular en esa fecha".to_string(),
        )),
        (Some("23001") | Some("23503"), Some("events_id_lapse_fkey")) => Some(AppError::Conflict(
            "El lapso tiene eventos asociados".to_string(),
        )),
        (Some("23001") | Some("23503"), Some("grade_overrides_id_lapse_fkey")) => Some(
            AppError::Conflict("El lapso tiene overrides de nota".to_string()),
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
    // La init tiene varias UNIQUE, un índice parcial y un EXCLUDE: el mismo
    // `23505` significa "combinación duplicada" sobre
    // `uq_sections_grade_section_shift`, "lapso repetido" sobre
    // `uq_lapses_year_number` y "segundo lapso activo" sobre el índice
    // parcial `uq_lapses_single_active`; el solape ni siquiera es `23505`
    // sino `23P01`. Cruzar solo el código devolvería 409 con un mensaje
    // falso.

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
        assert!(mapear_violacion(Some("23505"), Some("events_pkey")).is_none());
    }

    #[test]
    fn colision_de_practica_auto_es_conflict() {
        // La generación/reprogramación previene la colisión en el service,
        // pero si una se filtra, el índice único `uq_events_auto_one_per_day`
        // (23505) debe traducirse a 409, no a 500.
        let error = mapear_violacion(Some("23505"), Some("uq_events_auto_one_per_day")).unwrap();

        assert!(matches!(error, AppError::Conflict(_)));
        let mensaje = error.to_string();
        assert!(mensaje.contains("fecha"), "mensaje: {mensaje}");
    }

    #[test]
    fn duplicado_de_lapso_es_conflict() {
        let error = mapear_violacion(Some("23505"), Some("uq_lapses_year_number")).unwrap();

        assert!(matches!(error, AppError::Conflict(_)));
        let mensaje = error.to_string();
        assert!(mensaje.contains("año escolar"), "mensaje: {mensaje}");
    }

    #[test]
    fn el_solape_de_lapso_es_conflict_con_23p01() {
        // Un índice EXCLUDE violado emite 23P01 (exclusion_violation), no
        // 23505: mapear solo el último convertiría el solape en 500.
        let error = mapear_violacion(Some("23P01"), Some("lapses_no_overlap")).unwrap();

        assert!(matches!(error, AppError::Conflict(_)));
        let mensaje = error.to_string();
        assert!(mensaje.contains("solapan"), "mensaje: {mensaje}");
    }

    #[test]
    fn el_solape_sin_su_restriccion_no_mapea() {
        assert!(mapear_violacion(Some("23P01"), None).is_none());
        assert!(
            mapear_violacion(Some("23P01"), Some("uq_lapses_single_active")).is_none(),
            "el 23P01 necesita su EXCLUDE: otro constraint no alcanza"
        );
    }

    #[test]
    fn el_segundo_lapso_activo_es_conflict() {
        let error = mapear_violacion(Some("23505"), Some("uq_lapses_single_active")).unwrap();

        assert!(matches!(error, AppError::Conflict(_)));
        let mensaje = error.to_string();
        assert!(mensaje.contains("activo"), "mensaje: {mensaje}");
    }

    #[test]
    fn borrar_un_lapso_con_eventos_es_conflict() {
        for codigo in ["23001", "23503"] {
            let error = mapear_violacion(Some(codigo), Some("events_id_lapse_fkey")).unwrap();

            assert!(matches!(error, AppError::Conflict(_)));
            let mensaje = error.to_string();
            assert!(mensaje.contains("eventos"), "mensaje: {mensaje}");
        }
    }

    #[test]
    fn borrar_un_lapso_con_overrides_es_conflict() {
        let error = mapear_violacion(Some("23001"), Some("grade_overrides_id_lapse_fkey")).unwrap();

        assert!(matches!(error, AppError::Conflict(_)));
        let mensaje = error.to_string();
        assert!(mensaje.contains("override"), "mensaje: {mensaje}");
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
