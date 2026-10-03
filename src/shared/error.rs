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
    Database(#[from] sqlx::Error),

    #[error("Recurso no encontrado")]
    NotFound,

    #[error("Solicitud inválida: {0}")]
    BadRequest(String),
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_msg) = match &self {
            AppError::Database(_) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
            AppError::NotFound => (StatusCode::NOT_FOUND, self.to_string()),
            AppError::BadRequest(_) => (StatusCode::BAD_REQUEST, self.to_string()),
        };

        let body = Json(ErrorResponse {
            error: error_msg,
        });

        (status, body).into_response()
    }
}
