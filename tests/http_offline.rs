//! Borde HTTP de la sincronización offline, con `oneshot` (RF-6, RF-9, RF-10).
//!
//! Como en `http_grades.rs`: el handler usa el pool y **confirma** de verdad,
//! así que el archivo trabaja con un año y una sección exclusivos y purga su
//! residuo al arrancar y al terminar. Un único test secuencial.

mod common;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use axum::Router;
use civik_backend::app::build_router;
use civik_backend::modules::attendance::domain::{EventId, StudentId};
use civik_backend::modules::attendance::queries as att_queries;
use civik_backend::modules::events::queries as events_queries;
use civik_backend::modules::lapses::queries as lapses_queries;
use civik_backend::modules::sections::queries as sections_queries;
use civik_backend::modules::students::queries as students_queries;
use common::pool;
use tower::ServiceExt;
use uuid::Uuid;

type Res = Result<(), Box<dyn std::error::Error>>;

const ANIO: i32 = 2016;
const SECCION: &str = "WOf1";

/// Purga el residuo de este archivo, acotado a su año y sección. Los triggers
/// se desactivan porque `attendance` no se borra (regla 10).
async fn limpiar_residuo(pool: &sqlx::PgPool) -> Res {
    let mut tx = pool.begin().await?;
    for tabla in [
        "attendance",
        "grade_overrides",
        "events",
        "students",
        "sections",
        "lapses",
    ] {
        sqlx::query(&format!("ALTER TABLE {tabla} DISABLE TRIGGER ALL"))
            .execute(tx.as_mut())
            .await?;
    }
    sqlx::query(
        "DELETE FROM attendance WHERE id_event IN (
            SELECT id FROM events WHERE id_lapse IN (
                SELECT id FROM lapses WHERE school_year = $1))",
    )
    .bind(ANIO)
    .execute(tx.as_mut())
    .await?;
    sqlx::query(
        "DELETE FROM grade_overrides WHERE id_lapse IN (
            SELECT id FROM lapses WHERE school_year = $1)",
    )
    .bind(ANIO)
    .execute(tx.as_mut())
    .await?;
    sqlx::query(
        "DELETE FROM events WHERE id_lapse IN (SELECT id FROM lapses WHERE school_year = $1)",
    )
    .bind(ANIO)
    .execute(tx.as_mut())
    .await?;
    sqlx::query(
        "DELETE FROM students WHERE id_section IN (SELECT id FROM sections WHERE section = $1)",
    )
    .bind(SECCION)
    .execute(tx.as_mut())
    .await?;
    sqlx::query("DELETE FROM sections WHERE section = $1")
        .bind(SECCION)
        .execute(tx.as_mut())
        .await?;
    sqlx::query("DELETE FROM lapses WHERE school_year = $1")
        .bind(ANIO)
        .execute(tx.as_mut())
        .await?;
    for tabla in [
        "attendance",
        "grade_overrides",
        "events",
        "students",
        "sections",
        "lapses",
    ] {
        sqlx::query(&format!("ALTER TABLE {tabla} ENABLE TRIGGER ALL"))
            .execute(tx.as_mut())
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

fn pedir_json(method: Method, uri: String, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .expect("request válido")
}

async fn enviar(router: &Router, req: Request<Body>) -> axum::response::Response {
    router
        .clone()
        .oneshot(req)
        .await
        .expect("la petición no debe fallar en el transporte")
}

async fn json(res: axum::response::Response) -> serde_json::Value {
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .expect("cuerpo legible");
    serde_json::from_slice(&bytes).expect("cuerpo JSON")
}

#[tokio::test]
async fn flujo_http_de_sincronizacion() -> Res {
    let pool = pool().await?;
    limpiar_residuo(&pool).await?;

    // --- Montaje confirmado (lapso DRAFT: no bloquea a otros con un ACTIVE) ---
    let mut tx = pool.begin().await?;
    let lapso = lapses_queries::crear(
        &mut tx,
        civik_backend::modules::lapses::domain::preparar_entrada(
            "2016",
            "1",
            "2016-01-05",
            "2016-12-16",
            "THURSDAY",
        )?,
    )
    .await?;
    let sec = sections_queries::crear(
        &mut tx,
        civik_backend::modules::sections::domain::DatosSeccion {
            grado: civik_backend::modules::sections::domain::Grado::Primero,
            seccion: SECCION.into(),
            turno: civik_backend::modules::sections::domain::Turno::Matutino,
        },
    )
    .await?;
    let s_nuevo = students_queries::crear(
        &mut tx,
        civik_backend::modules::students::domain::DatosEstudiante {
            id_seccion: sec.id,
            nombre: "WOf".into(),
            apellido: "Alfa".into(),
            telefono: None,
        },
    )
    .await?;
    let s_conf = students_queries::crear(
        &mut tx,
        civik_backend::modules::students::domain::DatosEstudiante {
            id_seccion: sec.id,
            nombre: "WOf".into(),
            apellido: "Beta".into(),
            telefono: None,
        },
    )
    .await?;

    let ev = events_queries::crear_manual(
        tx.as_mut(),
        lapso.id,
        chrono::NaiveDate::from_ymd_opt(2016, 6, 15).expect("fecha válida"),
        "PRACTICA_EXTRA",
        "Clase",
    )
    .await?;
    // El servidor ya tiene la versión de `s_conf`.
    att_queries::upsert(
        tx.as_mut(),
        EventId(ev.id),
        StudentId(s_conf.id),
        "PRESENTE",
    )
    .await?;

    // Un segundo evento, sin asistencia, para probar el 409 por cancelado.
    let ev_cancel = events_queries::crear_manual(
        tx.as_mut(),
        lapso.id,
        chrono::NaiveDate::from_ymd_opt(2016, 6, 16).expect("fecha válida"),
        "PRACTICA_EXTRA",
        "Suspendida",
    )
    .await?;
    events_queries::cancelar(tx.as_mut(), ev_cancel.id, "se cayó la clase").await?;
    tx.commit().await?;

    let router = build_router(pool.clone());
    let ruta = format!("/events/{}/attendance/sync", ev.id);
    let viejo = (chrono::Utc::now() - chrono::Duration::hours(1)).to_rfc3339();

    // 1. Lote mixto → 200 con la clasificación por ítem.
    let cuerpo = serde_json::json!({
        "items": [
            { "id_student": s_nuevo.id, "state": "AUSENTE" },
            { "id_student": s_conf.id, "state": "AUSENTE", "cambio_en": viejo },
        ]
    });
    let res = enviar(&router, pedir_json(Method::POST, ruta.clone(), cuerpo)).await;
    assert_eq!(res.status(), StatusCode::OK);

    let items = json(res).await;
    let items = items.as_array().expect("arreglo de resultados");
    assert_eq!(items.len(), 2);

    let de = |id: Uuid| {
        items
            .iter()
            .find(|i| i["id_student"] == serde_json::json!(id))
            .expect("el ítem está en la respuesta")
    };
    assert_eq!(de(s_nuevo.id)["resultado"], "creado");
    let conflicto = de(s_conf.id);
    assert_eq!(conflicto["resultado"], "conflicto");
    assert_eq!(conflicto["state"], "PRESENTE");
    assert!(
        conflicto["updated_at"].is_string(),
        "el conflicto devuelve la versión del servidor"
    );

    // 2. Evento inexistente → 404.
    let res = enviar(
        &router,
        pedir_json(
            Method::POST,
            format!("/events/{}/attendance/sync", Uuid::new_v4()),
            serde_json::json!({
                "items": [{ "id_student": s_nuevo.id, "state": "AUSENTE" }]
            }),
        ),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    // 3. Estado inválido → 400.
    let res = enviar(
        &router,
        pedir_json(
            Method::POST,
            ruta.clone(),
            serde_json::json!({
                "items": [{ "id_student": s_nuevo.id, "state": "TAL_VEZ" }]
            }),
        ),
    )
    .await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    // 4. Evento cancelado → 409.
    let res = enviar(
        &router,
        pedir_json(
            Method::POST,
            format!("/events/{}/attendance/sync", ev_cancel.id),
            serde_json::json!({
                "items": [{ "id_student": s_nuevo.id, "state": "AUSENTE" }]
            }),
        ),
    )
    .await;
    assert_eq!(res.status(), StatusCode::CONFLICT);

    limpiar_residuo(&pool).await?;
    Ok(())
}
