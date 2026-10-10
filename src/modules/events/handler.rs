//! Handlers HTTP del módulo `events` (CRUD manual).

use crate::modules::events::queries::Event;
use crate::modules::events::service::{
    self, EntradaCancelar, EntradaCrearManual, EntradaEditarManual,
};
use crate::shared::error::AppError;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct ListarQuery {
    pub lapse_id: Option<Uuid>,
}

/// `POST /events` — crear evento manual (RF-1)
pub async fn crear(
    State(pool): State<PgPool>,
    Json(entrada): Json<EntradaCrearManual>,
) -> Result<(StatusCode, Json<Event>), AppError> {
    let evento = service::crear_manual(&pool, &entrada).await?;
    Ok((StatusCode::CREATED, Json(evento)))
}

/// `GET /events` — listar (RF-6)
pub async fn listar(
    State(pool): State<PgPool>,
    Query(q): Query<ListarQuery>,
) -> Result<Json<Vec<Event>>, AppError> {
    let eventos = service::listar(&pool, q.lapse_id).await?;
    Ok(Json(eventos))
}

/// `GET /events/{id}` — consultar (RF-7)
pub async fn consultar(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> Result<Json<Event>, AppError> {
    let evento = service::consultar(&pool, id).await?;
    Ok(Json(evento))
}

/// `PATCH /events/{id}` — editar manual (RF-10)
pub async fn editar(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
    Json(entrada): Json<EntradaEditarManual>,
) -> Result<Json<Event>, AppError> {
    let evento = service::editar_manual(&pool, id, &entrada).await?;
    Ok(Json(evento))
}

/// `POST /events/{id}/cancel` — cancelar con motivo (RF-8)
pub async fn cancelar(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
    Json(entrada): Json<EntradaCancelar>,
) -> Result<Json<Event>, AppError> {
    let evento = service::cancelar(&pool, id, &entrada.motivo).await?;
    Ok(Json(evento))
}

#[derive(Deserialize)]
pub struct EntradaReprogramar {
    pub nuevo_dia: String,
}

/// `POST /lapses/{id}/generar-practicas` — generar prácticas automáticas (RF-14)
pub async fn generar_auto(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> Result<(StatusCode, Json<Vec<Event>>), AppError> {
    let eventos = service::generar_practicas_auto(&pool, id).await?;
    Ok((StatusCode::OK, Json(eventos)))
}

/// `POST /lapses/{id}/reprogram-practice-day` — cambiar día de práctica (RF-16)
pub async fn reprogramar(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
    Json(entrada): Json<EntradaReprogramar>,
) -> Result<Json<service::ResultadoReprogramacion>, AppError> {
    let resultado = service::reprogramar_practice_day(&pool, id, &entrada.nuevo_dia).await?;
    Ok(Json(resultado))
}
