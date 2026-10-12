//! Lectura de filtros de query compartidos entre módulos.
//!
//! `incluir_inactivas` lo usan el padrón por sección, el listado global de
//! estudiantes y el listado de secciones. El parámetro llega como texto y acá
//! hay un único punto que lo convierte a `bool`: sin esto, cada handler
//! repetiría el mismo `match` y podrían divergir los valores aceptados o el
//! mensaje de error.

use crate::shared::error::AppError;

/// Convierte el valor de `incluir_inactivas` a `bool`.
///
/// Ausente o `false` → `false` (solo activas); `true` → `true`. Cualquier otro
/// valor es un `400` con un mensaje en español que nombra el parámetro y lo
/// recibido (RF-3, RF-6 de la spec 011).
pub fn leer_incluir_inactivas(valor: Option<&str>) -> Result<bool, AppError> {
    match valor {
        None | Some("false") => Ok(false),
        Some("true") => Ok(true),
        Some(otro) => Err(AppError::BadRequest(format!(
            "incluir_inactivas debe ser «true» o «false», llegó «{otro}»"
        ))),
    }
}
