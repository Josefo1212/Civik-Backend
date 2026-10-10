//! T7 — HTTP events.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use civik_backend::app::build_router;
use serde_json::json;
use tower::ServiceExt;
use uuid::Uuid;

type Resultado = Result<(), Box<dyn std::error::Error>>;

static CICLO_DE_VIDA: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn build_app() -> Router {
    build_router(common::pool().await.unwrap())
}

async fn crear_lapso(pool: &sqlx::PgPool, anio: i32) -> Result<Uuid, Box<dyn std::error::Error>> {
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
    Ok(lapso.id)
}

async fn limpiar_lapso(
    pool: &sqlx::PgPool,
    id_lapse: Uuid,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut tx = pool.begin().await?;
    sqlx::query("ALTER TABLE events DISABLE TRIGGER ALL")
        .execute(tx.as_mut())
        .await
        .ok();
    sqlx::query("ALTER TABLE lapses DISABLE TRIGGER ALL")
        .execute(tx.as_mut())
        .await
        .ok();
    sqlx::query("DELETE FROM events WHERE id_lapse = $1")
        .bind(id_lapse)
        .execute(tx.as_mut())
        .await
        .ok();
    sqlx::query("DELETE FROM lapses WHERE id = $1")
        .bind(id_lapse)
        .execute(tx.as_mut())
        .await
        .ok();
    sqlx::query("ALTER TABLE events ENABLE TRIGGER ALL")
        .execute(tx.as_mut())
        .await
        .ok();
    sqlx::query("ALTER TABLE lapses ENABLE TRIGGER ALL")
        .execute(tx.as_mut())
        .await
        .ok();
    tx.commit().await.ok();
    Ok(())
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
    sqlx::query("ALTER TABLE events DISABLE TRIGGER ALL")
        .execute(tx2.as_mut())
        .await
        .ok();
    sqlx::query("ALTER TABLE lapses DISABLE TRIGGER ALL")
        .execute(tx2.as_mut())
        .await
        .ok();
    sqlx::query("DELETE FROM events WHERE id_lapse = $1")
        .bind(lapso.id)
        .execute(tx2.as_mut())
        .await
        .ok();
    sqlx::query("DELETE FROM lapses WHERE id = $1")
        .bind(lapso.id)
        .execute(tx2.as_mut())
        .await
        .ok();
    sqlx::query("ALTER TABLE events ENABLE TRIGGER ALL")
        .execute(tx2.as_mut())
        .await
        .ok();
    sqlx::query("ALTER TABLE lapses ENABLE TRIGGER ALL")
        .execute(tx2.as_mut())
        .await
        .ok();
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
    sqlx::query("ALTER TABLE events DISABLE TRIGGER ALL")
        .execute(tx2.as_mut())
        .await
        .ok();
    sqlx::query("ALTER TABLE lapses DISABLE TRIGGER ALL")
        .execute(tx2.as_mut())
        .await
        .ok();
    sqlx::query("DELETE FROM events WHERE id_lapse = $1")
        .bind(lapso.id)
        .execute(tx2.as_mut())
        .await
        .ok();
    sqlx::query("DELETE FROM lapses WHERE id = $1")
        .bind(lapso.id)
        .execute(tx2.as_mut())
        .await
        .ok();
    sqlx::query("ALTER TABLE events ENABLE TRIGGER ALL")
        .execute(tx2.as_mut())
        .await
        .ok();
    sqlx::query("ALTER TABLE lapses ENABLE TRIGGER ALL")
        .execute(tx2.as_mut())
        .await
        .ok();
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

    limpiar_lapso(&pool, lapso.id).await
}

#[tokio::test]
async fn generar_practicas_responde_200_con_lista() -> Resultado {
    let _lock = CICLO_DE_VIDA.lock().await;
    let app = build_app().await;
    let pool = common::pool().await?;

    let anio = 2080;
    let id_lapse = crear_lapso(&pool, anio).await?;

    let req = Request::builder()
        .uri(format!("/lapses/{id_lapse}/generar-practicas"))
        .method("POST")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await?;
    let lista: Vec<serde_json::Value> = serde_json::from_slice(&bytes)?;
    assert!(
        !lista.is_empty(),
        "debe crear una práctica por día de práctica"
    );
    assert_eq!(
        lista[0]["source"], "AUTO",
        "la práctica generada debe ser AUTO"
    );

    limpiar_lapso(&pool, id_lapse).await
}

#[tokio::test]
async fn reprogramar_practice_day_responde_200() -> Resultado {
    let _lock = CICLO_DE_VIDA.lock().await;
    let app = build_app().await;
    let pool = common::pool().await?;

    let anio = 2081;
    let id_lapse = crear_lapso(&pool, anio).await?;

    let req = Request::builder()
        .uri(format!("/lapses/{id_lapse}/reprogram-practice-day"))
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(json!({ "nuevo_dia": "MONDAY" }).to_string()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await?;
    let resultado: serde_json::Value = serde_json::from_slice(&bytes)?;
    assert_eq!(resultado["dia_anterior"], "THURSDAY");
    assert_eq!(resultado["dia_nuevo"], "MONDAY");

    limpiar_lapso(&pool, id_lapse).await
}

#[tokio::test]
async fn reprogramar_dia_invalido_responde_400() -> Resultado {
    let _lock = CICLO_DE_VIDA.lock().await;
    let app = build_app().await;

    let req = Request::builder()
        .uri(format!("/lapses/{}/reprogram-practice-day", Uuid::new_v4()))
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(json!({ "nuevo_dia": "MARCIANO" }).to_string()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    Ok(())
}

#[tokio::test]
async fn generar_y_reprogramar_lapso_inexistente_responde_404() -> Resultado {
    let _lock = CICLO_DE_VIDA.lock().await;
    let app = build_app().await;

    let id = Uuid::new_v4();
    let req = Request::builder()
        .uri(format!("/lapses/{id}/generar-practicas"))
        .method("POST")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    let req = Request::builder()
        .uri(format!("/lapses/{id}/reprogram-practice-day"))
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(json!({ "nuevo_dia": "MONDAY" }).to_string()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    Ok(())
}

#[tokio::test]
async fn generar_y_reprogramar_lapso_cerrado_responde_409() -> Resultado {
    let _lock = CICLO_DE_VIDA.lock().await;
    let app = build_app().await;
    let pool = common::pool().await?;

    let anio = 2082;
    let id_lapse = crear_lapso(&pool, anio).await?;
    let mut tx = pool.begin().await?;
    sqlx::query("UPDATE lapses SET status = 'CLOSED' WHERE id = $1")
        .bind(id_lapse)
        .execute(tx.as_mut())
        .await?;
    tx.commit().await?;

    let req = Request::builder()
        .uri(format!("/lapses/{id_lapse}/generar-practicas"))
        .method("POST")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);

    let req = Request::builder()
        .uri(format!("/lapses/{id_lapse}/reprogram-practice-day"))
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(json!({ "nuevo_dia": "MONDAY" }).to_string()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);

    limpiar_lapso(&pool, id_lapse).await
}
