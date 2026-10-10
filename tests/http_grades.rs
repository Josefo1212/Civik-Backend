//! Borde HTTP de `grades` con `oneshot` (RNF-8).
//!
//! Igual que `http_attendance.rs`: el handler usa el pool y **confirma** de
//! verdad, así que el archivo trabaja con un año y una sección exclusivos y
//! purga su residuo al arrancar y al terminar. Un único test secuencial.

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
use rust_decimal::Decimal;
use tower::ServiceExt;
use uuid::Uuid;

mod common;

type Res = Result<(), Box<dyn std::error::Error>>;

const ANIO: i32 = 2001;
const SECCION: &str = "HGra";

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

fn pedir(method: Method, uri: String) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .body(Body::empty())
        .expect("request válido")
}

async fn llamar(router: &Router, req: Request<Body>) -> StatusCode {
    router
        .clone()
        .oneshot(req)
        .await
        .expect("la petición no debe fallar en el transporte")
        .status()
}

async fn json(res: axum::response::Response) -> serde_json::Value {
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .expect("cuerpo legible");
    serde_json::from_slice(&bytes).expect("cuerpo JSON")
}

#[tokio::test]
async fn flujo_http_de_notas() -> Res {
    let pool = pool().await?;
    limpiar_residuo(&pool).await?;

    // --- Montaje confirmado (lapso DRAFT: no bloquea ningún otro test con un
    //     lapso ACTIVE global) ---
    let mut tx = pool.begin().await?;
    let lapso = lapses_queries::crear(
        &mut tx,
        civik_backend::modules::lapses::domain::preparar_entrada(
            "2001",
            "1",
            "2001-01-05",
            "2001-12-16",
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
    let s1 = students_queries::crear(
        &mut tx,
        civik_backend::modules::students::domain::DatosEstudiante {
            id_seccion: sec.id,
            nombre: "HGra".into(),
            apellido: "Alfa".into(),
            telefono: None,
        },
    )
    .await?;
    let s2 = students_queries::crear(
        &mut tx,
        civik_backend::modules::students::domain::DatosEstudiante {
            id_seccion: sec.id,
            nombre: "HGra".into(),
            apellido: "Beta".into(),
            telefono: None,
        },
    )
    .await?;

    // Evento pasado (elegible con el lapso abierto) con una falta y un override.
    let ev = events_queries::crear_manual(
        tx.as_mut(),
        lapso.id,
        chrono::NaiveDate::from_ymd_opt(2001, 6, 15).expect("fecha válida"),
        "PRACTICA_REGULAR",
        "Clase",
    )
    .await?;
    att_queries::upsert(tx.as_mut(), EventId(ev.id), StudentId(s1.id), "AUSENTE").await?;
    att_queries::upsert(tx.as_mut(), EventId(ev.id), StudentId(s2.id), "PRESENTE").await?;
    sqlx::query("INSERT INTO grade_overrides (id_student, id_lapse, value) VALUES ($1, $2, $3)")
        .bind(s2.id)
        .bind(lapso.id)
        .bind(Decimal::new(1875, 2))
        .execute(tx.as_mut())
        .await?;
    tx.commit().await?;

    let router = build_router(pool.clone());
    let ruta = format!("/lapses/{}/sections/{}/grades", lapso.id, sec.id);

    // 1. GET válido → 200 con la nota calculada y el override.
    let res = router
        .clone()
        .oneshot(pedir(Method::GET, ruta.clone()))
        .await?;
    assert_eq!(res.status(), StatusCode::OK);
    let filas = json(res).await;
    let filas = filas.as_array().expect("arreglo de notas");
    assert_eq!(filas.len(), 2);

    let nota_de = |id: Uuid| {
        filas
            .iter()
            .find(|f| f["id_student"] == serde_json::json!(id))
            .and_then(|f| f["nota"].as_str().map(str::to_owned))
    };
    assert_eq!(nota_de(s1.id).as_deref(), Some("19.0"));
    assert_eq!(nota_de(s2.id).as_deref(), Some("18.75"));

    // 2. GET con lapso inexistente → 404.
    assert_eq!(
        llamar(
            &router,
            pedir(
                Method::GET,
                format!("/lapses/{}/sections/{}/grades", Uuid::new_v4(), sec.id)
            )
        )
        .await,
        StatusCode::NOT_FOUND
    );

    // 3. GET con sección inexistente → 404.
    assert_eq!(
        llamar(
            &router,
            pedir(
                Method::GET,
                format!("/lapses/{}/sections/{}/grades", lapso.id, Uuid::new_v4())
            )
        )
        .await,
        StatusCode::NOT_FOUND
    );

    // 4. Método no permitido → 405.
    assert_eq!(
        llamar(&router, pedir(Method::POST, ruta)).await,
        StatusCode::METHOD_NOT_ALLOWED
    );

    limpiar_residuo(&pool).await?;
    Ok(())
}
