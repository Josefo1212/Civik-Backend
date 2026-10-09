//! T7 — HTTP events.

mod common;

use axum::http::{Request, StatusCode};
use axum::Router;
use civik_backend::app::build_router;
use serde_json::json;
use tower::ServiceExt;
use axum::body::Body;
use uuid::Uuid;

type Resultado = Result<(), Box<dyn std::error::Error>>;

static CICLO_DE_VIDA: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn build_app() -> Router {
    build_router(common::pool().await.unwrap())
}

#[tokio::test]
async fn crear_evento_manual_ok() -> Resultado {
    let _lock = CICLO_DE_VIDA.lock().await;
    let app = build_app().await;
    let pool = common::pool().await?;

    // crear lapso
    let anio = 2055;
    let mut tx = pool.begin().await?;
    let lapso = civik_backend::modules::lapses::queries::crear(
        &mut tx,
        civik_backend::modules::lapses::domain::preparar_entrada(
            &anio.to_string(),
            "1",
            &format!("{anio}-09-01"),
            &format!("{anio}-12-20"),
            "THURSDAY",
        )
        .unwrap(),
    )
    .await?;
    tx.commit().await?;

    let req = Request::builder()
        .uri("/events")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "id_lapse": lapso.id,
                "event_date": format!("{anio}-09-05"),
                "type_event": "PRACTICA_EXTRA",
                "event_name": "Extra"
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let mut tx2 = pool.begin().await?;
    sqlx::query("ALTER TABLE events DISABLE TRIGGER ALL").execute(tx2.as_mut()).await.ok();
    sqlx::query("ALTER TABLE lapses DISABLE TRIGGER ALL").execute(tx2.as_mut()).await.ok();
    sqlx::query("DELETE FROM events WHERE id_lapse = $1").bind(lapso.id).execute(tx2.as_mut()).await.ok();
    sqlx::query("DELETE FROM lapses WHERE id = $1").bind(lapso.id).execute(tx2.as_mut()).await.ok();
    sqlx::query("ALTER TABLE events ENABLE TRIGGER ALL").execute(tx2.as_mut()).await.ok();
    sqlx::query("ALTER TABLE lapses ENABLE TRIGGER ALL").execute(tx2.as_mut()).await.ok();
    tx2.commit().await.ok();
    Ok(())
}

#[tokio::test]
async fn listar_eventos() -> Resultado {
    let _lock = CICLO_DE_VIDA.lock().await;
    let app = build_app().await;
    let pool = common::pool().await?;

    let anio = 2056;
    let mut tx = pool.begin().await?;
    let lapso = civik_backend::modules::lapses::queries::crear(
        &mut tx,
        civik_backend::modules::lapses::domain::preparar_entrada(
            &anio.to_string(),
            "1",
            &format!("{anio}-09-01"),
            &format!("{anio}-12-20"),
            "THURSDAY",
        )
        .unwrap(),
    )
    .await?;
    tx.commit().await?;

    let req = Request::builder()
        .uri("/events")
        .method("GET")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let mut tx2 = pool.begin().await?;
    sqlx::query("ALTER TABLE events DISABLE TRIGGER ALL").execute(tx2.as_mut()).await.ok();
    sqlx::query("ALTER TABLE lapses DISABLE TRIGGER ALL").execute(tx2.as_mut()).await.ok();
    sqlx::query("DELETE FROM events WHERE id_lapse = $1").bind(lapso.id).execute(tx2.as_mut()).await.ok();
    sqlx::query("DELETE FROM lapses WHERE id = $1").bind(lapso.id).execute(tx2.as_mut()).await.ok();
    sqlx::query("ALTER TABLE events ENABLE TRIGGER ALL").execute(tx2.as_mut()).await.ok();
    sqlx::query("ALTER TABLE lapses ENABLE TRIGGER ALL").execute(tx2.as_mut()).await.ok();
    tx2.commit().await.ok();
    Ok(())
}

#[tokio::test]
async fn consultar_evento_inexistente_404() -> Resultado {
    let _lock = CICLO_DE_VIDA.lock().await;
    let app = build_app().await;

    let req = Request::builder()
        .uri(format!("/events/{}", Uuid::new_v4()))
        .method("GET")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    Ok(())
}

#[tokio::test]
async fn crear_evento_fecha_fuera_de_rango_400_o_409() -> Resultado {
    let _lock = CICLO_DE_VIDA.lock().await;
    let app = build_app().await;
    let pool = common::pool().await?;

    let anio = 2060;
    let mut tx = pool.begin().await?;
    let lapso = civik_backend::modules::lapses::queries::crear(
        &mut tx,
        civik_backend::modules::lapses::domain::preparar_entrada(
            &anio.to_string(),
            "1",
            &format!("{anio}-09-01"),
            &format!("{anio}-12-20"),
            "THURSDAY",
        )
        .unwrap(),
    )
    .await?;
    tx.commit().await?;

    let req = Request::builder()
        .uri("/events")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "id_lapse": lapso.id,
                "event_date": format!("{anio}-08-01"),
                "type_event": "PRACTICA_EXTRA",
                "event_name": "Extra"
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(resp.status() == StatusCode::CONFLICT || resp.status() == StatusCode::BAD_REQUEST);

    let mut tx2 = pool.begin().await?;
    sqlx::query("ALTER TABLE events DISABLE TRIGGER ALL").execute(tx2.as_mut()).await.ok();
    sqlx::query("ALTER TABLE lapses DISABLE TRIGGER ALL").execute(tx2.as_mut()).await.ok();
    sqlx::query("DELETE FROM events WHERE id_lapse = $1").bind(lapso.id).execute(tx2.as_mut()).await.ok();
    sqlx::query("DELETE FROM lapses WHERE id = $1").bind(lapso.id).execute(tx2.as_mut()).await.ok();
    sqlx::query("ALTER TABLE events ENABLE TRIGGER ALL").execute(tx2.as_mut()).await.ok();
    sqlx::query("ALTER TABLE lapses ENABLE TRIGGER ALL").execute(tx2.as_mut()).await.ok();
    tx2.commit().await.ok();
    Ok(())
}
