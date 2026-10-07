//! Bordes HTTP del módulo `students`, probados con `tower::ServiceExt::oneshot`
//! sobre el mismo `Router` que sirve producción (plan 003 §8.5): sin abrir
//! puerto.
//!
//! Lo pertinente no es repetir los casos de negocio de `tests/students.rs`
//! (transacción/rollback), sino comprobar que la superficie HTTP convierte
//! cada situación en el código correcto y con la forma de JSON acordada
//! (`snake_case`, `null` en teléfono ausente). Estos tests **commitean**, así
//! que cada uno crea lo suyo con prefijos UUID únicos y borra al final con
//! `DELETE` directo (RF-19 no expone borrado por la API — la higiene de la
//! prueba no es un comportamiento de la API).

mod common;

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use civik_backend::app::build_router;
use serde_json::{json, Value};
use tower::ServiceExt;
use uuid::Uuid;

type Resultado = Result<(), Box<dyn std::error::Error>>;

fn prefijo_unico() -> String {
    Uuid::new_v4().simple().to_string()[..8].to_string()
}

async fn crear_seccion(app: &axum::Router, seccion: &str) -> Resultado {
    let respuesta = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/sections")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "grade_level": "1ro",
                        "section": seccion,
                        "shift": "MATUTINO",
                    })
                    .to_string(),
                ))?,
        )
        .await?;
    assert_eq!(respuesta.status(), StatusCode::CREATED);
    Ok(())
}

async fn limpiar_estudiante(id: Uuid) -> Resultado {
    let pool = common::pool().await?;
    sqlx::query("DELETE FROM students WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await?;
    Ok(())
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
async fn alta_201_normaliza_y_devuelve_snake_case() -> Resultado {
    let pool = common::pool().await?;
    let app = build_router(pool.clone());
    let seccion = prefijo_unico();
    crear_seccion(&app, &seccion).await?;
    let id_seccion: Uuid = sqlx::query_scalar("SELECT id FROM sections WHERE section = $1")
        .bind(&seccion)
        .fetch_one(&pool)
        .await?;

    let respuesta = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/sections/{id_seccion}/students"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "apellido": "  García   Rojas ",
                        "nombre": "Ana  María",
                        "telefono": "04121"
                    })
                    .to_string(),
                ))?,
        )
        .await?;

    assert_eq!(respuesta.status(), StatusCode::CREATED, "RF-1");
    let bytes = to_bytes(respuesta.into_body(), usize::MAX).await?;
    let json: Value = serde_json::from_slice(&bytes)?;

    assert_eq!(json["id_section"], id_seccion.to_string());
    assert_eq!(json["last_name"], "García Rojas", "RF-5: colapsado");
    assert_eq!(json["first_name"], "Ana María");
    assert_eq!(json["phone"], "04121");
    assert_eq!(json["active"], true);
    assert!(json["created_at"].is_string());
    assert!(json["updated_at"].is_string());

    limpiar_estudiante(Uuid::parse_str(json["id"].as_str().unwrap())?).await?;
    limpiar_seccion(id_seccion).await?;
    Ok(())
}

#[tokio::test]
async fn alta_sin_telefono_devuelve_null_en_phone() -> Resultado {
    let pool = common::pool().await?;
    let app = build_router(pool.clone());
    let seccion = prefijo_unico();
    crear_seccion(&app, &seccion).await?;
    let id_seccion: Uuid = sqlx::query_scalar("SELECT id FROM sections WHERE section = $1")
        .bind(&seccion)
        .fetch_one(&pool)
        .await?;

    let respuesta = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/sections/{id_seccion}/students"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"apellido": "García", "nombre": "Ana"}).to_string(),
                ))?,
        )
        .await?;

    assert_eq!(respuesta.status(), StatusCode::CREATED);
    let bytes = to_bytes(respuesta.into_body(), usize::MAX).await?;
    let json: Value = serde_json::from_slice(&bytes)?;
    assert_eq!(json["phone"], Value::Null);

    limpiar_estudiante(Uuid::parse_str(json["id"].as_str().unwrap())?).await?;
    limpiar_seccion(id_seccion).await?;
    Ok(())
}

#[tokio::test]
async fn alta_en_seccion_inactiva_responde_409_y_la_inexistente_404() -> Resultado {
    let pool = common::pool().await?;
    let app = build_router(pool.clone());
    let seccion = prefijo_unico();
    crear_seccion(&app, &seccion).await?;
    let id_seccion: Uuid = sqlx::query_scalar("SELECT id FROM sections WHERE section = $1")
        .bind(&seccion)
        .fetch_one(&pool)
        .await?;

    sqlx::query("UPDATE sections SET active = false WHERE id = $1")
        .bind(id_seccion)
        .execute(&pool)
        .await?;

    let inactiva = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/sections/{id_seccion}/students"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"apellido": "García", "nombre": "Ana"}).to_string(),
                ))?,
        )
        .await?;
    assert_eq!(inactiva.status(), StatusCode::CONFLICT, "RF-4");

    let inexistente = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/sections/{}/students", Uuid::new_v4()))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"apellido": "García", "nombre": "Ana"}).to_string(),
                ))?,
        )
        .await?;
    assert_eq!(inexistente.status(), StatusCode::NOT_FOUND, "RF-3");

    limpiar_seccion(id_seccion).await?;
    Ok(())
}

#[tokio::test]
async fn alta_con_campos_vacios_responde_400() -> Resultado {
    let pool = common::pool().await?;
    let app = build_router(pool);

    let respuesta = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/sections/{}/students", Uuid::new_v4()))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"apellido": "  ", "nombre": "Ana"}).to_string(),
                ))?,
        )
        .await?;
    assert_eq!(respuesta.status(), StatusCode::BAD_REQUEST, "RF-2");
    let bytes = to_bytes(respuesta.into_body(), usize::MAX).await?;
    let json: Value = serde_json::from_slice(&bytes)?;
    assert!(
        json["error"].as_str().unwrap_or("").contains("apellido"),
        "el mensaje explica qué falta: {json}"
    );

    Ok(())
}

#[tokio::test]
async fn listado_200_con_filtro_y_400_con_parametro_malo() -> Resultado {
    let pool = common::pool().await?;
    let app = build_router(pool.clone());
    let seccion = prefijo_unico();
    crear_seccion(&app, &seccion).await?;
    let id_seccion: Uuid = sqlx::query_scalar("SELECT id FROM sections WHERE section = $1")
        .bind(&seccion)
        .fetch_one(&pool)
        .await?;

    let cuerpo = || Body::from(json!({"apellido": "García", "nombre": "Ana"}).to_string());
    let alta = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/sections/{id_seccion}/students"))
                .header("content-type", "application/json")
                .body(cuerpo())?,
        )
        .await?;
    assert_eq!(alta.status(), StatusCode::CREATED);
    let id = Uuid::parse_str(
        serde_json::from_slice::<Value>(&to_bytes(alta.into_body(), usize::MAX).await?)?["id"]
            .as_str()
            .unwrap(),
    )?;
    sqlx::query("UPDATE students SET active = false WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await?;

    // Sin filtro solo trae activos → la lista de este test queda vacía.
    let sin_filtro = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/sections/{id_seccion}/students"))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(sin_filtro.status(), StatusCode::OK, "RF-6");
    let bytes = to_bytes(sin_filtro.into_body(), usize::MAX).await?;
    let lista: Value = serde_json::from_slice(&bytes)?;
    let tabla: Vec<Value> = lista
        .as_array()
        .unwrap()
        .iter()
        .filter(|fila| fila["id"] == id.to_string())
        .cloned()
        .collect();
    assert!(tabla.is_empty(), "RF-6: el inactivo se omite");

    // Con la bandera lo devuelve marcado.
    let con_filtro = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/sections/{id_seccion}/students?incluir_inactivas=true"
                ))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(con_filtro.status(), StatusCode::OK, "RF-7");
    let bytes = to_bytes(con_filtro.into_body(), usize::MAX).await?;
    let lista: Value = serde_json::from_slice(&bytes)?;
    let incluido = lista
        .as_array()
        .unwrap()
        .iter()
        .find(|fila| fila["id"] == id.to_string())
        .expect("la bandera debe incluirlo");
    assert_eq!(incluido["active"], false, "RF-7: marcado como inactivo");

    // El parámetro mal escrito es un 400, no un 500.
    let malo = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/sections/{id_seccion}/students?incluir_inactivas=si"
                ))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(malo.status(), StatusCode::BAD_REQUEST, "parámetro inválido");

    limpiar_estudiante(id).await?;
    limpiar_seccion(id_seccion).await?;
    Ok(())
}

#[tokio::test]
async fn consulta_200_y_404_para_inexistente() -> Resultado {
    let pool = common::pool().await?;
    let app = build_router(pool.clone());
    let seccion = prefijo_unico();
    crear_seccion(&app, &seccion).await?;
    let id_seccion: Uuid = sqlx::query_scalar("SELECT id FROM sections WHERE section = $1")
        .bind(&seccion)
        .fetch_one(&pool)
        .await?;

    let alta = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/sections/{id_seccion}/students"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"apellido": "García", "nombre": "Ana"}).to_string(),
                ))?,
        )
        .await?;
    let id = Uuid::parse_str(
        serde_json::from_slice::<Value>(&to_bytes(alta.into_body(), usize::MAX).await?)?["id"]
            .as_str()
            .unwrap(),
    )?;

    let consulta = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/students/{id}"))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(consulta.status(), StatusCode::OK, "RF-8");
    let bytes = to_bytes(consulta.into_body(), usize::MAX).await?;
    let json: Value = serde_json::from_slice(&bytes)?;
    assert_eq!(json["id_section"], id_seccion.to_string(), "RF-8");
    assert_eq!(json["first_name"], "Ana");

    let inexistente = app
        .oneshot(
            Request::builder()
                .uri(format!("/students/{}", Uuid::new_v4()))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(inexistente.status(), StatusCode::NOT_FOUND, "RF-9");

    limpiar_estudiante(id).await?;
    limpiar_seccion(id_seccion).await?;
    Ok(())
}

#[tokio::test]
async fn edicion_200_cambia_datos_y_mueve_de_seccion() -> Resultado {
    let pool = common::pool().await?;
    let app = build_router(pool.clone());
    let seccion_origen = prefijo_unico();
    let seccion_destino = format!("d{}", prefijo_unico());
    crear_seccion(&app, &seccion_origen).await?;
    crear_seccion(&app, &seccion_destino).await?;
    let origen: Uuid = sqlx::query_scalar("SELECT id FROM sections WHERE section = $1")
        .bind(&seccion_origen)
        .fetch_one(&pool)
        .await?;
    let destino: Uuid = sqlx::query_scalar("SELECT id FROM sections WHERE section = $1")
        .bind(&seccion_destino)
        .fetch_one(&pool)
        .await?;

    let alta = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/sections/{origen}/students"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"apellido": "García", "nombre": "Ana"}).to_string(),
                ))?,
        )
        .await?;
    let id = Uuid::parse_str(
        serde_json::from_slice::<Value>(&to_bytes(alta.into_body(), usize::MAX).await?)?["id"]
            .as_str()
            .unwrap(),
    )?;

    // La edición normaliza (RF-10), mueve a la sección activa (RF-12) y el
    // primer término de RF-13 se cumple: sin asistencia, el cambio pasa.
    let edicion = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/students/{id}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "id_seccion": destino,
                        "apellido": "  Rojas  ",
                        "nombre": "María",
                        "telefono": "04121"
                    })
                    .to_string(),
                ))?,
        )
        .await?;
    assert_eq!(edicion.status(), StatusCode::OK, "RF-12");
    let bytes = to_bytes(edicion.into_body(), usize::MAX).await?;
    let json: Value = serde_json::from_slice(&bytes)?;
    assert_eq!(json["id_section"], destino.to_string());
    assert_eq!(json["last_name"], "Rojas");
    assert_eq!(json["phone"], "04121");

    limpiar_estudiante(id).await?;
    limpiar_seccion(origen).await?;
    limpiar_seccion(destino).await?;
    Ok(())
}

#[tokio::test]
async fn edicion_a_seccion_inactiva_responde_409_y_editar_inexistente_404() -> Resultado {
    let pool = common::pool().await?;
    let app = build_router(pool.clone());
    let seccion = prefijo_unico();
    crear_seccion(&app, &seccion).await?;
    let id_seccion: Uuid = sqlx::query_scalar("SELECT id FROM sections WHERE section = $1")
        .bind(&seccion)
        .fetch_one(&pool)
        .await?;

    let alta = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/sections/{id_seccion}/students"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"apellido": "García", "nombre": "Ana"}).to_string(),
                ))?,
        )
        .await?;
    let id = Uuid::parse_str(
        serde_json::from_slice::<Value>(&to_bytes(alta.into_body(), usize::MAX).await?)?["id"]
            .as_str()
            .unwrap(),
    )?;

    let a_inactiva = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO sections (id, grade_level, section, shift, active) \
         VALUES ($1, '2do', 'b', 'MATUTINO', false)",
    )
    .bind(a_inactiva)
    .execute(&pool)
    .await?;

    let mover = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/students/{id}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "id_seccion": a_inactiva,
                        "apellido": "García",
                        "nombre": "Ana"
                    })
                    .to_string(),
                ))?,
        )
        .await?;
    assert_eq!(
        mover.status(),
        StatusCode::CONFLICT,
        "RF-12: sección inactiva"
    );

    let editar_inexistente = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/students/{}", Uuid::new_v4()))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "id_seccion": id_seccion,
                        "apellido": "García",
                        "nombre": "Ana"
                    })
                    .to_string(),
                ))?,
        )
        .await?;
    assert_eq!(
        editar_inexistente.status(),
        StatusCode::NOT_FOUND,
        "RF-11 edición"
    );

    limpiar_estudiante(id).await?;
    limpiar_seccion(id_seccion).await?;
    sqlx::query("DELETE FROM sections WHERE id = $1")
        .bind(a_inactiva)
        .execute(&pool)
        .await?;
    Ok(())
}

#[tokio::test]
async fn baja_y_reactivacion_200_idempotentes_y_404_para_inexistente() -> Resultado {
    let pool = common::pool().await?;
    let app = build_router(pool.clone());
    let seccion = prefijo_unico();
    crear_seccion(&app, &seccion).await?;
    let id_seccion: Uuid = sqlx::query_scalar("SELECT id FROM sections WHERE section = $1")
        .bind(&seccion)
        .fetch_one(&pool)
        .await?;

    let alta = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/sections/{id_seccion}/students"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"apellido": "García", "nombre": "Ana"}).to_string(),
                ))?,
        )
        .await?;
    let id = Uuid::parse_str(
        serde_json::from_slice::<Value>(&to_bytes(alta.into_body(), usize::MAX).await?)?["id"]
            .as_str()
            .unwrap(),
    )?;

    for _ in 0..2 {
        let baja = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/students/{id}/baja"))
                    .body(Body::empty())?,
            )
            .await?;
        assert_eq!(baja.status(), StatusCode::OK, "RF-14/RF-15");
        let bytes = to_bytes(baja.into_body(), usize::MAX).await?;
        let json: Value = serde_json::from_slice(&bytes)?;
        assert_eq!(json["active"], false);
    }

    for _ in 0..2 {
        let reactivar = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/students/{id}/reactivar"))
                    .body(Body::empty())?,
            )
            .await?;
        assert_eq!(reactivar.status(), StatusCode::OK, "RF-16/RF-17");
        let bytes = to_bytes(reactivar.into_body(), usize::MAX).await?;
        let json: Value = serde_json::from_slice(&bytes)?;
        assert_eq!(json["active"], true);
    }

    let inexistente = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/students/{}/baja", Uuid::new_v4()))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(inexistente.status(), StatusCode::NOT_FOUND);

    limpiar_estudiante(id).await?;
    limpiar_seccion(id_seccion).await?;
    Ok(())
}
