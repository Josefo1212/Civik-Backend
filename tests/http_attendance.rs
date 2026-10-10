//! Bordes HTTP de `attendance` con `oneshot`.
//!
//! El handler usa el pool y **confirma** de verdad (no la transacción del
//! test), así que este archivo trabaja con un año y una sección exclusivos y
//! purga su residuo al arrancar: un fallo a mitad deja filas vivas que la
//! corrida siguiente necesita limpiar. Es un único test secuencial —no varios
//! en paralelo— justamente porque los tests de un archivo corren en hilos
//! concurrentes y compartirían este mismo estado confirmado.

use axum::body::Body;
use axum::http::{self, Method, Request, StatusCode};
use axum::Router;
use chrono::Datelike;
use civik_backend::app::build_router;
use civik_backend::modules::events::queries as events_queries;
use civik_backend::modules::lapses::queries as lapses_queries;
use civik_backend::modules::sections::queries as sections_queries;
use civik_backend::modules::students::queries as students_queries;
use common::pool;
use tower::ServiceExt;
use uuid::Uuid;

mod common;

type Res = Result<(), Box<dyn std::error::Error>>;

const ANIO: i32 = 2099;
const SECCION: &str = "HAtt";

/// Purga el residuo de una corrida interrumpida, acotado al año y la sección
/// de este archivo para no tocar datos de otros tests. Los triggers de fila se
/// desactivan porque `attendance` no se puede borrar (regla 10) y su `DELETE`
/// en cascada desde `events` dispararía el guardarraíl.
async fn limpiar_residuo(pool: &sqlx::PgPool) -> Res {
    let mut tx = pool.begin().await?;
    for tabla in ["attendance", "events", "students", "sections", "lapses"] {
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
    for tabla in ["attendance", "events", "students", "sections", "lapses"] {
        sqlx::query(&format!("ALTER TABLE {tabla} ENABLE TRIGGER ALL"))
            .execute(tx.as_mut())
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

fn primer_jueves(anio: i32, mes: u32) -> chrono::NaiveDate {
    let mut dia = chrono::NaiveDate::from_ymd_opt(anio, mes, 1).expect("fecha válida");
    while dia.weekday() != chrono::Weekday::Thu {
        dia = dia.succ_opt().expect("hay jueves en el mes");
    }
    dia
}

async fn llamar(router: &Router, req: Request<Body>) -> StatusCode {
    router
        .clone()
        .oneshot(req)
        .await
        .expect("la petición no debe fallar en el transporte")
        .status()
}

fn pedir(method: Method, uri: String, body: Option<serde_json::Value>) -> Request<Body> {
    let builder = Request::builder().method(method).uri(uri);
    match body {
        Some(valor) => builder
            .header(http::header::CONTENT_TYPE, "application/json")
            .body(Body::from(valor.to_string()))
            .expect("request válido"),
        None => builder.body(Body::empty()).expect("request válido"),
    }
}

#[tokio::test]
async fn flujo_http_de_asistencia() -> Res {
    let pool = pool().await?;
    limpiar_residuo(&pool).await?;

    // --- Montaje confirmado ---
    let mut tx = pool.begin().await?;
    let lapso1 = lapses_queries::crear(
        &mut tx,
        civik_backend::modules::lapses::domain::preparar_entrada(
            &ANIO.to_string(),
            "1",
            &format!("{ANIO}-09-01"),
            &format!("{ANIO}-12-20"),
            "THURSDAY",
        )?,
    )
    .await?;
    let lapso2 = lapses_queries::crear(
        &mut tx,
        civik_backend::modules::lapses::domain::preparar_entrada(
            &ANIO.to_string(),
            "2",
            &format!("{ANIO}-02-01"),
            &format!("{ANIO}-06-30"),
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
            nombre: "HAtt".into(),
            apellido: "Alfa".into(),
            telefono: None,
        },
    )
    .await?;
    let s2 = students_queries::crear(
        &mut tx,
        civik_backend::modules::students::domain::DatosEstudiante {
            id_seccion: sec.id,
            nombre: "HAtt".into(),
            apellido: "Baja".into(),
            telefono: None,
        },
    )
    .await?;

    let evento1 = events_queries::crear_manual(
        tx.as_mut(),
        lapso1.id,
        primer_jueves(ANIO, 9),
        "PRACTICA_REGULAR",
        "Clase 1",
    )
    .await?;
    let evento2 = events_queries::crear_manual(
        tx.as_mut(),
        lapso1.id,
        primer_jueves(ANIO, 9) + chrono::Duration::weeks(1),
        "PRACTICA_REGULAR",
        "Clase 2",
    )
    .await?;
    let evento3 = events_queries::crear_manual(
        tx.as_mut(),
        lapso2.id,
        primer_jueves(ANIO, 2),
        "PRACTICA_REGULAR",
        "Clase 3",
    )
    .await?;

    // Cancelar evento2 (sin asistencia: se permite) y cerrar lapso2.
    sqlx::query(
        "UPDATE events SET status = 'CANCELLED', cancelled_reason = 'prueba' WHERE id = $1",
    )
    .bind(evento2.id)
    .execute(tx.as_mut())
    .await?;
    students_queries::set_active(&mut tx, s2.id, false).await?;
    tx.commit().await?;

    // Activar y cerrar lapso2 (fuera de la transacción de montaje).
    let mut tx = pool.begin().await?;
    lapses_queries::activar(&mut tx, lapso2.id).await?;
    lapses_queries::cerrar(&mut tx, lapso2.id).await?;
    tx.commit().await?;

    let router = build_router(pool.clone());

    // 1. POST válido → 200 con el padrón.
    let res = router
        .clone()
        .oneshot(pedir(
            Method::POST,
            format!("/events/{}/attendance", evento1.id),
            Some(serde_json::json!({ "items": [{ "id_student": s1.id, "state": "PRESENTE" }] })),
        ))
        .await?;
    assert_eq!(res.status(), StatusCode::OK);

    // 2. GET padrón → 200.
    assert_eq!(
        llamar(
            &router,
            pedir(
                Method::GET,
                format!("/events/{}/attendance", evento1.id),
                None
            )
        )
        .await,
        StatusCode::OK
    );

    // 3. PATCH válido → 200.
    assert_eq!(
        llamar(
            &router,
            pedir(
                Method::PATCH,
                format!("/events/{}/attendance/{}", evento1.id, s1.id),
                Some(serde_json::json!({ "state": "AUSENTE" }))
            )
        )
        .await,
        StatusCode::OK
    );

    // 4. PATCH repetido (mismo estado) → 200 idempotente.
    assert_eq!(
        llamar(
            &router,
            pedir(
                Method::PATCH,
                format!("/events/{}/attendance/{}", evento1.id, s1.id),
                Some(serde_json::json!({ "state": "AUSENTE" }))
            )
        )
        .await,
        StatusCode::OK
    );

    // 5. GET de evento inexistente → 404.
    assert_eq!(
        llamar(
            &router,
            pedir(
                Method::GET,
                format!("/events/{}/attendance", Uuid::new_v4()),
                None
            )
        )
        .await,
        StatusCode::NOT_FOUND
    );

    // 6. POST a evento inexistente → 404.
    assert_eq!(
        llamar(
            &router,
            pedir(
                Method::POST,
                format!("/events/{}/attendance", Uuid::new_v4()),
                Some(serde_json::json!({ "items": [] }))
            )
        )
        .await,
        StatusCode::NOT_FOUND
    );

    // 7. POST con estado inválido → 400.
    assert_eq!(
        llamar(
            &router,
            pedir(
                Method::POST,
                format!("/events/{}/attendance", evento1.id),
                Some(
                    serde_json::json!({ "items": [{ "id_student": s1.id, "state": "presente" }] })
                )
            )
        )
        .await,
        StatusCode::BAD_REQUEST
    );

    // 8. PATCH con estado inválido → 400.
    assert_eq!(
        llamar(
            &router,
            pedir(
                Method::PATCH,
                format!("/events/{}/attendance/{}", evento1.id, s1.id),
                Some(serde_json::json!({ "state": "otro" }))
            )
        )
        .await,
        StatusCode::BAD_REQUEST
    );

    // 9. POST con estudiante inexistente → 404.
    assert_eq!(
        llamar(
            &router,
            pedir(
                Method::POST,
                format!("/events/{}/attendance", evento1.id),
                Some(
                    serde_json::json!({ "items": [{ "id_student": Uuid::new_v4(), "state": "PRESENTE" }] })
                )
            )
        )
        .await,
        StatusCode::NOT_FOUND
    );

    // 10. POST en evento cancelado → 409.
    assert_eq!(
        llamar(
            &router,
            pedir(
                Method::POST,
                format!("/events/{}/attendance", evento2.id),
                Some(
                    serde_json::json!({ "items": [{ "id_student": s1.id, "state": "PRESENTE" }] })
                )
            )
        )
        .await,
        StatusCode::CONFLICT
    );

    // 11. POST en lapso cerrado → 409.
    assert_eq!(
        llamar(
            &router,
            pedir(
                Method::POST,
                format!("/events/{}/attendance", evento3.id),
                Some(
                    serde_json::json!({ "items": [{ "id_student": s1.id, "state": "PRESENTE" }] })
                )
            )
        )
        .await,
        StatusCode::CONFLICT
    );

    // 12. POST a estudiante de baja sin fila → 409.
    assert_eq!(
        llamar(
            &router,
            pedir(
                Method::POST,
                format!("/events/{}/attendance", evento1.id),
                Some(
                    serde_json::json!({ "items": [{ "id_student": s2.id, "state": "PRESENTE" }] })
                )
            )
        )
        .await,
        StatusCode::CONFLICT
    );

    // 13. DELETE no existe → 405.
    assert_eq!(
        llamar(
            &router,
            pedir(
                Method::DELETE,
                format!("/events/{}/attendance", evento1.id),
                None
            )
        )
        .await,
        StatusCode::METHOD_NOT_ALLOWED
    );

    limpiar_residuo(&pool).await?;
    Ok(())
}
