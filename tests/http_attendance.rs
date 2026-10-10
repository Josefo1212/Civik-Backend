use axum::body::Body;
use axum::http::{self, Method, Request, StatusCode};
use civik_backend::app::build_router;
use civik_backend::modules::events::queries as events_queries;
use civik_backend::modules::lapses::queries as lapses_queries;
use civik_backend::modules::sections::queries as sections_queries;
use civik_backend::modules::students::queries as students_queries;
use common::pool;
use tower::ServiceExt;

mod common;

type Res = Result<(), Box<dyn std::error::Error>>;

/// Purga el residuo que una corrida interrumpida haya podido dejar: este test
/// HTTP hace `commit` de verdad (el handler usa el pool, no la transacción del
/// test), así que un fallo a mitad deja filas vivas que rompen la corrida
/// siguiente. Se usa un año y una sección exclusivos de este archivo para que
/// el `DELETE` no toque datos de otros tests.
async fn limpiar_residuo(pool: &sqlx::PgPool) -> Res {
    let mut tx = pool.begin().await?;
    sqlx::query("ALTER TABLE attendance DISABLE TRIGGER ALL")
        .execute(tx.as_mut())
        .await?;
    sqlx::query("ALTER TABLE events DISABLE TRIGGER ALL")
        .execute(tx.as_mut())
        .await?;
    sqlx::query("ALTER TABLE students DISABLE TRIGGER ALL")
        .execute(tx.as_mut())
        .await?;
    sqlx::query("ALTER TABLE sections DISABLE TRIGGER ALL")
        .execute(tx.as_mut())
        .await?;
    sqlx::query("ALTER TABLE lapses DISABLE TRIGGER ALL")
        .execute(tx.as_mut())
        .await?;
    sqlx::query("DELETE FROM attendance WHERE id_event IN (SELECT id FROM events WHERE id_lapse IN (SELECT id FROM lapses WHERE school_year = 2099))")
        .execute(tx.as_mut())
        .await?;
    sqlx::query(
        "DELETE FROM events WHERE id_lapse IN (SELECT id FROM lapses WHERE school_year = 2099)",
    )
    .execute(tx.as_mut())
    .await?;
    sqlx::query(
        "DELETE FROM students WHERE id_section IN (SELECT id FROM sections WHERE section = 'HAtt')",
    )
    .execute(tx.as_mut())
    .await?;
    sqlx::query("DELETE FROM sections WHERE section = 'HAtt'")
        .execute(tx.as_mut())
        .await?;
    sqlx::query("DELETE FROM lapses WHERE school_year = 2099")
        .execute(tx.as_mut())
        .await?;
    sqlx::query("ALTER TABLE attendance ENABLE TRIGGER ALL")
        .execute(tx.as_mut())
        .await?;
    sqlx::query("ALTER TABLE events ENABLE TRIGGER ALL")
        .execute(tx.as_mut())
        .await?;
    sqlx::query("ALTER TABLE students ENABLE TRIGGER ALL")
        .execute(tx.as_mut())
        .await?;
    sqlx::query("ALTER TABLE sections ENABLE TRIGGER ALL")
        .execute(tx.as_mut())
        .await?;
    sqlx::query("ALTER TABLE lapses ENABLE TRIGGER ALL")
        .execute(tx.as_mut())
        .await?;
    tx.commit().await?;
    Ok(())
}

#[tokio::test]
async fn registrar_listado_y_corregir_http() -> Res {
    let pool = pool().await?;
    limpiar_residuo(&pool).await?;
    let mut tx = pool.begin().await?;
    let lap = lapses_queries::crear(
        &mut tx,
        civik_backend::modules::lapses::domain::preparar_entrada(
            "2099",
            "1",
            "2099-09-01",
            "2099-12-20",
            "THURSDAY",
        )?,
    )
    .await?;
    let sec = sections_queries::crear(
        &mut tx,
        civik_backend::modules::sections::domain::DatosSeccion {
            grado: civik_backend::modules::sections::domain::Grado::Primero,
            seccion: "HAtt".into(),
            turno: civik_backend::modules::sections::domain::Turno::Matutino,
        },
    )
    .await?;
    let est = students_queries::crear(
        &mut tx,
        civik_backend::modules::students::domain::DatosEstudiante {
            id_seccion: sec.id,
            nombre: "HAtt".into(),
            apellido: "B".into(),
            telefono: None,
        },
    )
    .await?;
    let ev = events_queries::crear_manual(
        tx.as_mut(),
        lap.id,
        chrono::NaiveDate::from_ymd_opt(2099, 9, 11).unwrap(),
        "PRACTICA_REGULAR",
        "C",
    )
    .await?;
    tx.commit().await?;

    let router = build_router(pool.clone());
    let body =
        serde_json::json!({"items":[{"id_student": est.id, "state":"PRESENTE"}]}).to_string();
    let req = Request::builder()
        .method(Method::POST)
        .uri(format!("/events/{}/attendance", ev.id))
        .header(http::header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))?;
    let res = router.clone().oneshot(req).await?;
    assert_eq!(res.status(), StatusCode::CREATED);

    let body = serde_json::json!({"state":"AUSENTE"}).to_string();
    let req = Request::builder()
        .method(Method::PATCH)
        .uri(format!("/attendance/{}/{}", ev.id, est.id))
        .header(http::header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))?;
    let res = router.oneshot(req).await?;
    assert_eq!(res.status(), StatusCode::OK);

    let mut tx = pool.begin().await?;
    let _ = sqlx::query("DELETE FROM attendance WHERE id_event=$1")
        .bind(ev.id)
        .execute(tx.as_mut())
        .await;
    let _ = sqlx::query("DELETE FROM events WHERE id=$1")
        .bind(ev.id)
        .execute(tx.as_mut())
        .await;
    let _ = sqlx::query("DELETE FROM students WHERE id=$1")
        .bind(est.id)
        .execute(tx.as_mut())
        .await;
    let _ = sqlx::query("DELETE FROM sections WHERE id=$1")
        .bind(sec.id)
        .execute(tx.as_mut())
        .await;
    let _ = sqlx::query("DELETE FROM lapses WHERE id=$1")
        .bind(lap.id)
        .execute(tx.as_mut())
        .await;
    tx.commit().await?;
    Ok(())
}
