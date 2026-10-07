//! El `_health` no puede mentir: con la base en pie responde 200, y si la
//! consulta falla responde 503 (antes descartaba el error con `let _ =` y
//! siempre devolvía 200).
//!
//! El caso sin base se construye con un pool perezoso (`connect_lazy`, no
//! establece conexión hasta el primer uso) apuntando a un puerto que nadie
//! escucha: la adquisición falla rápido por connection refused.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use civik_backend::app::build_router;
use sqlx::postgres::PgPoolOptions;
use std::time::Duration;
use tower::ServiceExt;

type Resultado = Result<(), Box<dyn std::error::Error>>;

#[tokio::test]
async fn health_con_la_base_en_pie_responde_200() -> Resultado {
    let pool = common::pool().await?;
    let app = build_router(pool);

    let respuesta = app
        .oneshot(Request::builder().uri("/_health").body(Body::empty())?)
        .await?;

    assert_eq!(respuesta.status(), StatusCode::OK);
    Ok(())
}

#[tokio::test]
async fn health_sin_base_responde_503() -> Resultado {
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_secs(3))
        .connect_lazy("postgres://civik:civik@127.0.0.1:1/civik_dev")?;

    let app = build_router(pool);

    let respuesta = app
        .oneshot(Request::builder().uri("/_health").body(Body::empty())?)
        .await?;

    assert_eq!(respuesta.status(), StatusCode::SERVICE_UNAVAILABLE);
    Ok(())
}
