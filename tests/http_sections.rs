//! Bordes HTTP del módulo `sections`, probados con `tower::ServiceExt::oneshot`
//! sobre el mismo `Router` que sirve producción (plan §8.6): sin abrir puerto.
//!
//! Difieren de los tests de `sections.rs` en una cosa: el handler hace
//! `commit` de verdad, así que cada test crea sus filas con una subdivisión
//! única (un trozo de UUID, `varchar(10)`) y las borra al final con `DELETE`
//! directo — como las secciones no llegan a tener estudiantes, RF-22 permite
//! ese borrado. Si un test falla a mitad y queda la fila, no estorba a otro:
//! la subdivisión única hace que la combinación nunca esté en uso.

mod common;

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use civik_backend::app::build_router;
use serde_json::Value;
use tower::ServiceExt;
use uuid::Uuid;

type Resultado = Result<(), Box<dyn std::error::Error>>;

/// Una subdivisión única para este test, tan corta que cabe en `varchar(10)`.
fn subdivision_unica() -> String {
    Uuid::new_v4().simple().to_string()[..8].to_string()
}

async fn limpiar_seccion(id: Uuid) -> Resultado {
    let pool = common::pool().await?;
    sqlx::query("DELETE FROM students WHERE id_section = $1")
        .bind(id)
        .execute(&pool)
        .await?;
    sqlx::query("DELETE FROM sections WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await?;
    Ok(())
}

#[tokio::test]
async fn alta_201_devuelve_la_seccion_normalizada_en_snake_case() -> Resultado {
    let pool = common::pool().await?;
    let app = build_router(pool.clone());
    let seccion = subdivision_unica();

    let body = Body::from(
        serde_json::json!({
            "grade_level": " 1RO ",
            "section": format!(" {seccion} "),
            "shift": "matutino",
        })
        .to_string(),
    );
    let respuesta = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/sections")
                .header("content-type", "application/json")
                .body(body)?,
        )
        .await?;

    assert_eq!(respuesta.status(), StatusCode::CREATED);

    let bytes = to_bytes(respuesta.into_body(), usize::MAX).await?;
    let json: Value = serde_json::from_slice(&bytes)?;

    // RF-2/RF-3/RF-4: recortado, minúsculas para grado y subdivisión,
    // mayúsculas para el turno.
    assert_eq!(json["grade_level"], "1ro");
    assert_eq!(json["section"], seccion);
    assert_eq!(json["shift"], "MATUTINO");
    assert_eq!(json["active"], true);
    assert!(json["created_at"].is_string());
    assert!(json["updated_at"].is_string());

    let id = Uuid::parse_str(json["id"].as_str().unwrap())?;
    limpiar_seccion(id).await?;
    Ok(())
}

#[tokio::test]
async fn alta_duplicada_responde_409() -> Resultado {
    let pool = common::pool().await?;
    let app = build_router(pool.clone());
    let seccion = subdivision_unica();

    let cuerpo = || {
        serde_json::json!({
            "grade_level": "1ro",
            "section": seccion,
            "shift": "MATUTINO",
        })
        .to_string()
    };

    let primera = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/sections")
                .header("content-type", "application/json")
                .body(Body::from(cuerpo()))?,
        )
        .await?;
    assert_eq!(
        primera.status(),
        StatusCode::CREATED,
        "RF-1: el alta funciona"
    );
    let id = {
        let bytes = to_bytes(primera.into_body(), usize::MAX).await?;
        let json: Value = serde_json::from_slice(&bytes)?;
        Uuid::parse_str(json["id"].as_str().unwrap())?
    };

    let segunda = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/sections")
                .header("content-type", "application/json")
                .body(Body::from(cuerpo()))?,
        )
        .await?;
    assert_eq!(
        segunda.status(),
        StatusCode::CONFLICT,
        "RF-8: combinación duplicada"
    );
    let bytes = to_bytes(segunda.into_body(), usize::MAX).await?;
    let json: Value = serde_json::from_slice(&bytes)?;
    assert!(
        json["error"].as_str().unwrap_or("").contains("en uso"),
        "el mensaje explica el conflicto: {json}"
    );

    limpiar_seccion(id).await?;
    Ok(())
}

#[tokio::test]
async fn alta_invalida_responde_400_y_enumera_los_grados() -> Resultado {
    let pool = common::pool().await?;
    let app = build_router(pool);

    let respuesta = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/sections")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "grade_level": "9no",
                        "section": "a",
                        "shift": "MATUTINO",
                    })
                    .to_string(),
                ))?,
        )
        .await?;

    assert_eq!(
        respuesta.status(),
        StatusCode::BAD_REQUEST,
        "RF-5/RF-6/RF-7"
    );
    let bytes = to_bytes(respuesta.into_body(), usize::MAX).await?;
    let json: Value = serde_json::from_slice(&bytes)?;
    let error = json["error"].as_str().unwrap_or("");
    assert!(
        error.contains("1ro"),
        "el mensaje enumera los grados: {error}"
    );

    Ok(())
}

#[tokio::test]
async fn listado_200_ordena_y_filtra_por_grado() -> Resultado {
    let pool = common::pool().await?;
    let app = build_router(pool.clone());

    // Se crean por POST y se borran al final; el listado solo lee lo que está
    // activo (RF-9/RF-24) en el orden canónico (RF-11).
    let mut ids = Vec::new();
    for (grado, sub) in [("3ro", "z"), ("1ro", "b"), ("1ro", "a"), ("5to", "y")] {
        let sub = format!("{sub}{}", subdivision_unica());
        let respuesta = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/sections")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "grade_level": grado,
                            "section": sub,
                            "shift": "MATUTINO",
                        })
                        .to_string(),
                    ))?,
            )
            .await?;
        assert_eq!(respuesta.status(), StatusCode::CREATED);
        let bytes = to_bytes(respuesta.into_body(), usize::MAX).await?;
        let json: Value = serde_json::from_slice(&bytes)?;
        ids.push((Uuid::parse_str(json["id"].as_str().unwrap())?, sub));
    }

    let respuesta = app
        .clone()
        .oneshot(Request::builder().uri("/sections").body(Body::empty())?)
        .await?;
    assert_eq!(respuesta.status(), StatusCode::OK);
    let bytes = to_bytes(respuesta.into_body(), usize::MAX).await?;
    let lista: Value = serde_json::from_slice(&bytes)?;

    // El listado puede contener filas de corridas previas que quedaron si un
    // test anterior falló antes de su limpieza: las aserciones de orden usan
    // POSICIONES relativas de lo que este test creó, no el arreglo completo.
    let posiciones: std::collections::HashMap<Uuid, usize> = lista
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(i, s)| (Uuid::parse_str(s["id"].as_str().unwrap()).unwrap(), i))
        .collect();
    let ids_creadas: Vec<Uuid> = ids.iter().map(|(id, _)| *id).collect();
    let p = |id: Uuid| posiciones[&id];

    // ids[0]="3ro:z…", ids[1]="1ro:b…", ids[2]="1ro:a…", ids[3]="5to:y…".
    assert!(
        p(ids_creadas[2]) < p(ids_creadas[1]),
        "RF-11: dentro del grado la subdivisión va alfabética (a antes que b)"
    );
    assert!(
        p(ids_creadas[1]) < p(ids_creadas[0]),
        "RF-11: 1ro va antes que 3ro"
    );
    assert!(
        p(ids_creadas[0]) < p(ids_creadas[3]),
        "RF-11: 3ro va antes que 5to"
    );

    // RF-10: el filtro solo deja pasar las del grado pedido.
    let respuesta = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/sections?grado=1ro")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(respuesta.status(), StatusCode::OK);
    let bytes = to_bytes(respuesta.into_body(), usize::MAX).await?;
    let lista: Value = serde_json::from_slice(&bytes)?;
    let uno_ro: Vec<Uuid> = lista
        .as_array()
        .unwrap()
        .iter()
        .map(|s| Uuid::parse_str(s["id"].as_str().unwrap()).unwrap())
        .collect();
    assert!(
        uno_ro.contains(&ids_creadas[1]) && uno_ro.contains(&ids_creadas[2]),
        "RF-10: el filtro incluye las de ese grado"
    );
    assert!(
        !uno_ro.contains(&ids_creadas[0]) && !uno_ro.contains(&ids_creadas[3]),
        "RF-10/RF-24: el filtro excluye los otros grados"
    );

    for (id, _) in ids {
        limpiar_seccion(id).await?;
    }
    Ok(())
}

#[tokio::test]
async fn listado_con_grado_no_admitido_responde_400() -> Resultado {
    let pool = common::pool().await?;
    let app = build_router(pool);

    let respuesta = app
        .oneshot(
            Request::builder()
                .uri("/sections?grado=9no")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(
        respuesta.status(),
        StatusCode::BAD_REQUEST,
        "grado fuera del catálogo"
    );

    Ok(())
}

#[tokio::test]
async fn consulta_200_devuelve_la_seccion_en_snake_case() -> Resultado {
    let pool = common::pool().await?;
    let app = build_router(pool.clone());
    let seccion = subdivision_unica();

    let cuerpo = || {
        serde_json::json!({
            "grade_level": "1ro",
            "section": seccion,
            "shift": "MATUTINO",
        })
        .to_string()
    };
    let creada = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/sections")
                .header("content-type", "application/json")
                .body(Body::from(cuerpo()))?,
        )
        .await?;
    let id = {
        let bytes = to_bytes(creada.into_body(), usize::MAX).await?;
        let json: Value = serde_json::from_slice(&bytes)?;
        Uuid::parse_str(json["id"].as_str().unwrap())?
    };

    let respuesta = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/sections/{id}"))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(respuesta.status(), StatusCode::OK, "RF-12");
    let bytes = to_bytes(respuesta.into_body(), usize::MAX).await?;
    let json: Value = serde_json::from_slice(&bytes)?;
    assert_eq!(json["grade_level"], "1ro");
    assert_eq!(json["section"], seccion);
    assert_eq!(json["shift"], "MATUTINO");
    assert!(json["active"].is_boolean());

    limpiar_seccion(id).await?;
    Ok(())
}

#[tokio::test]
async fn consulta_inexistente_responde_404() -> Resultado {
    let pool = common::pool().await?;
    let app = build_router(pool);

    let respuesta = app
        .oneshot(
            Request::builder()
                .uri(format!("/sections/{}", Uuid::new_v4()))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(respuesta.status(), StatusCode::NOT_FOUND, "RF-13");

    Ok(())
}

#[tokio::test]
async fn edicion_200_normaliza_y_es_valor_legible() -> Resultado {
    let pool = common::pool().await?;
    let app = build_router(pool.clone());
    let seccion = subdivision_unica();

    let creada = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/sections")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "grade_level": "1ro",
                        "section": seccion,
                        "shift": "MATUTINO",
                    })
                    .to_string(),
                ))?,
        )
        .await?;
    let id = {
        let bytes = to_bytes(creada.into_body(), usize::MAX).await?;
        let json: Value = serde_json::from_slice(&bytes)?;
        Uuid::parse_str(json["id"].as_str().unwrap())?
    };

    let edicion = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/sections/{id}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "grade_level": " 2DO ",
                        "section": seccion,
                        "shift": "vespertino",
                    })
                    .to_string(),
                ))?,
        )
        .await?;
    assert_eq!(edicion.status(), StatusCode::OK, "RF-14");
    let bytes = to_bytes(edicion.into_body(), usize::MAX).await?;
    let json: Value = serde_json::from_slice(&bytes)?;
    assert_eq!(json["grade_level"], "2do");
    assert_eq!(json["shift"], "VESPERTINO");

    limpiar_seccion(id).await?;
    Ok(())
}

#[tokio::test]
async fn edicion_a_una_combinacion_ajena_responde_409() -> Resultado {
    let pool = common::pool().await?;
    let app = build_router(pool.clone());

    let creadas: Vec<(Uuid, String)> = {
        let mut v = Vec::new();
        for _ in 0..2 {
            let sub = subdivision_unica();
            let respuesta = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/sections")
                        .header("content-type", "application/json")
                        .body(Body::from(
                            serde_json::json!({
                                "grade_level": "1ro",
                                "section": sub.clone(),
                                "shift": "MATUTINO",
                            })
                            .to_string(),
                        ))?,
                )
                .await?;
            assert_eq!(respuesta.status(), StatusCode::CREATED);
            let bytes = to_bytes(respuesta.into_body(), usize::MAX).await?;
            let json: Value = serde_json::from_slice(&bytes)?;
            v.push((Uuid::parse_str(json["id"].as_str().unwrap())?, sub));
        }
        v
    };

    // Edito la primera para que tome la combinación de la segunda → 409.
    let respuesta = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/sections/{}", creadas[0].0))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "grade_level": "1ro",
                        "section": creadas[1].1,
                        "shift": "MATUTINO",
                    })
                    .to_string(),
                ))?,
        )
        .await?;
    assert_eq!(respuesta.status(), StatusCode::CONFLICT, "RF-17");

    for (id, _) in creadas {
        limpiar_seccion(id).await?;
    }
    Ok(())
}

#[tokio::test]
async fn editar_inexistente_responde_404() -> Resultado {
    let pool = common::pool().await?;
    let app = build_router(pool);

    let respuesta = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/sections/{}", Uuid::new_v4()))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "grade_level": "1ro",
                        "section": "a",
                        "shift": "MATUTINO",
                    })
                    .to_string(),
                ))?,
        )
        .await?;
    assert_eq!(respuesta.status(), StatusCode::NOT_FOUND);

    Ok(())
}

#[tokio::test]
async fn baja_y_reactivacion_200_idempotentes() -> Resultado {
    let pool = common::pool().await?;
    let app = build_router(pool.clone());
    let seccion = subdivision_unica();

    let creada = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/sections")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "grade_level": "1ro",
                        "section": seccion,
                        "shift": "MATUTINO",
                    })
                    .to_string(),
                ))?,
        )
        .await?;
    let id = {
        let bytes = to_bytes(creada.into_body(), usize::MAX).await?;
        let json: Value = serde_json::from_slice(&bytes)?;
        Uuid::parse_str(json["id"].as_str().unwrap())?
    };

    // RF-18 + RF-19: la segunda baja también responde 200 y no modifica nada.
    for _ in 0..2 {
        let baja = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/sections/{id}/baja"))
                    .body(Body::empty())?,
            )
            .await?;
        assert_eq!(baja.status(), StatusCode::OK, "RF-18/RF-19");
        let bytes = to_bytes(baja.into_body(), usize::MAX).await?;
        let json: Value = serde_json::from_slice(&bytes)?;
        assert_eq!(json["active"], false);
    }

    // RF-20 + RF-21: la segunda reactivación también 200.
    for _ in 0..2 {
        let reactivar = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/sections/{id}/reactivar"))
                    .body(Body::empty())?,
            )
            .await?;
        assert_eq!(reactivar.status(), StatusCode::OK, "RF-20/RF-21");
        let bytes = to_bytes(reactivar.into_body(), usize::MAX).await?;
        let json: Value = serde_json::from_slice(&bytes)?;
        assert_eq!(json["active"], true);
    }

    limpiar_seccion(id).await?;
    Ok(())
}

#[tokio::test]
async fn baja_de_inexistente_responde_404() -> Resultado {
    let pool = common::pool().await?;
    let app = build_router(pool);

    let respuesta = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/sections/{}/baja", Uuid::new_v4()))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(respuesta.status(), StatusCode::NOT_FOUND);

    Ok(())
}

#[tokio::test]
async fn eliminacion_204_sin_cuerpo_y_la_seccion_desaparece() -> Resultado {
    let pool = common::pool().await?;
    let app = build_router(pool.clone());
    let seccion = subdivision_unica();

    let creada = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/sections")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "grade_level": "1ro",
                        "section": seccion,
                        "shift": "MATUTINO",
                    })
                    .to_string(),
                ))?,
        )
        .await?;
    let id = {
        let bytes = to_bytes(creada.into_body(), usize::MAX).await?;
        let json: Value = serde_json::from_slice(&bytes)?;
        Uuid::parse_str(json["id"].as_str().unwrap())?
    };

    let borrada = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/sections/{id}"))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(
        borrada.status(),
        StatusCode::NO_CONTENT,
        "RF-22: 204 sin cuerpo"
    );
    let bytes = to_bytes(borrada.into_body(), usize::MAX).await?;
    assert!(bytes.is_empty(), "el 204 no debe tener cuerpo");

    let quedan: i64 = sqlx::query_scalar("SELECT count(*) FROM sections WHERE id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await?;
    assert_eq!(quedan, 0, "la eliminación es física (RF-22)");

    Ok(())
}

#[tokio::test]
async fn eliminacion_de_seccion_con_estudiantes_responde_409() -> Resultado {
    let pool = common::pool().await?;
    let app = build_router(pool.clone());
    let seccion = subdivision_unica();

    let creada = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/sections")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "grade_level": "1ro",
                        "section": seccion,
                        "shift": "MATUTINO",
                    })
                    .to_string(),
                ))?,
        )
        .await?;
    let id = {
        let bytes = to_bytes(creada.into_body(), usize::MAX).await?;
        let json: Value = serde_json::from_slice(&bytes)?;
        Uuid::parse_str(json["id"].as_str().unwrap())?
    };

    sqlx::query(
        "INSERT INTO students (id_section, first_name, last_name) VALUES ($1, 'Ana', 'Pérez')",
    )
    .bind(id)
    .execute(&pool)
    .await?;

    let borrada = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/sections/{id}"))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(
        borrada.status(),
        StatusCode::CONFLICT,
        "RF-23: sección con estudiantes"
    );
    let bytes = to_bytes(borrada.into_body(), usize::MAX).await?;
    let json: Value = serde_json::from_slice(&bytes)?;
    assert!(
        json["error"].as_str().unwrap_or("").contains("estudiantes"),
        "el mensaje nombra el problema: {json}"
    );

    sqlx::query("DELETE FROM students WHERE id_section = $1")
        .bind(id)
        .execute(&pool)
        .await?;
    limpiar_seccion(id).await?;
    Ok(())
}
