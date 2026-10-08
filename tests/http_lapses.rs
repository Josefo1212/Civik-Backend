//! Bordes HTTP del módulo `lapses`, probados con `tower::ServiceExt::oneshot`
//! sobre el mismo `Router` que sirve producción: sin abrir puerto.
//!
//! A diferencia de `lapses.rs` (T6, capas `domain`+`queries`+triggers dentro de
//! una transacción descartable), el handler hace `commit` de verdad: cada test
//! reclama un **año escolar exclusivo**, con fechas dentro de ese año y fin muy
//! futuro (≥2050), y borra su fila al final con `DELETE` — que nunca cae en el
//! `lapses_delete_guard` porque el lapso nunca se cierra (un `CLOSED` es
//! residuo indeletable; por eso el feliz de `cerrar` y los 409 sobre `CLOSED`
//! se prueban solo en T6, plan §8.4).
//!
//! El plan pedía derivar el `school_year` de un UUID, pero el mod-universo de
//! ~20 tests sufre el problema del cumpleaños (≈2 colisiones esperadas por
//! corrida, que acabarían en 409 aleatorios): un contador atómico reserva cada
//! corrida un año distinto y determinista. Los tramos de fechas viven cada uno
//! ENTERO dentro de su año exclusivo, así que los rangos de tests distintos
//! nunca se solapan (el EXCLUDE `lapses_no_overlap` es global y los tests
//! corren en paralelo).

mod common;

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use civik_backend::app::build_router;
use serde_json::Value;
use std::sync::atomic::{AtomicI32, Ordering};
use tower::ServiceExt;
use uuid::Uuid;

type Resultado = Result<(), Box<dyn std::error::Error>>;

/// Serializa los tests que activan o cierran lapsos.
///
/// La regla "un solo ACTIVE" (`uq_lapses_single_active`) es GLOBAL y estos
/// tests corren en paralelo: dos tests que activen al mismo tiempo pueden
/// patearse entre sí con un 409 espurio. El `Mutex` garantiza que en todo
/// momento solo uno toca transiciones de estado, y cada uno borra su lapso
/// (que al no ser CLOSED, el guardarraíl deja borrar) antes de soltarlo.
static CICLO_DE_VIDA: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Purga el residuo que una corrida interrumpida de ESTE binary haya podido
/// dejar. Los tests HTTP de `lapses` solo crean filas con `school_year` ≥ 2050
/// y jamás las cierran ni les ligan datos, así que todo lo que sobrepase 2050
/// es descartable con el guardarraíl en paz (declarado con `OnceCell`: el
/// primer test espera mientras limpia; los demás ya pasan de largo).
static RESIDUO_LIMPIADO: tokio::sync::OnceCell<()> = tokio::sync::OnceCell::const_new();

async fn limpiar_residuo_previo(pool: &sqlx::PgPool) -> Resultado {
    RESIDUO_LIMPIADO
        .get_or_try_init(|| async {
            sqlx::query("DELETE FROM lapses WHERE school_year >= 2050")
                .execute(pool)
                .await?;
            Ok::<(), sqlx::Error>(())
        })
        .await?;
    Ok(())
}

/// Reparte años escolares exclusivos por corrida, desde 2050 (siempre futuro:
/// `end_date` ≥ 2050 para que la limpieza por `DELETE` nunca se bloquee).
static SIGUIENTE_ANIO: AtomicI32 = AtomicI32::new(2050);

fn anio_escolar_unico() -> i32 {
    SIGUIENTE_ANIO.fetch_add(1, Ordering::Relaxed)
}

/// El cuerpo de alta/edición: el año exclusivo del test en texto y sus fechas.
fn cuerpo_lapso(anio: i32, numero: &str) -> Value {
    serde_json::json!({
        "school_year": anio.to_string(),
        "lapse_number": numero,
        "start_date": format!("{anio}-02-01"),
        "end_date": format!("{anio}-12-20"),
        "practice_day": "THURSDAY",
    })
}

/// `POST /lapses` con `cuerpo`, y devuelve el id creado (el test ya verificó el
/// `201` y la normalización en el test dedicado; acá solo se arma el dato).
async fn crear_lapso(app: Router, anio: i32) -> Result<Uuid, Box<dyn std::error::Error>> {
    let respuesta = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/lapses")
                .header("content-type", "application/json")
                .body(Body::from(cuerpo_lapso(anio, "1").to_string()))?,
        )
        .await?;
    assert_eq!(respuesta.status(), StatusCode::CREATED, "RF-1: el alta funciona");
    let bytes = to_bytes(respuesta.into_body(), usize::MAX).await?;
    let json: Value = serde_json::from_slice(&bytes)?;
    Ok(Uuid::parse_str(json["id"].as_str().unwrap())?)
}

/// Borra una fila que este test creó (raw SQL, como en `http_sections.rs`): la
/// limpieza no debe fabricar auditoría; el `DELETE` dedicado de RF-17 se prueba
/// en su propio test por HTTP.
async fn limpiar_lapso(pool: &sqlx::PgPool, id: Uuid) -> Resultado {
    sqlx::query("DELETE FROM lapses WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

// --- Alta: RF-1, RF-2, RF-3, RF-4 ---

#[tokio::test]
async fn alta_201_normaliza_y_nace_en_planeado() -> Resultado {
    let pool = common::pool().await?;
    limpiar_residuo_previo(&pool).await?;
    let app = build_router(pool.clone());
    let anio = anio_escolar_unico();

    let cuerpo = serde_json::json!({
        "school_year": format!(" {anio} "),
        "lapse_number": " 1 ",
        "start_date": " 2050-02-01 ".replace("2050", &anio.to_string()),
        "end_date": " 2050-12-20 ".replace("2050", &anio.to_string()),
        "practice_day": " thursday ",
    });
    let respuesta = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/lapses")
                .header("content-type", "application/json")
                .body(Body::from(cuerpo.to_string()))?,
        )
        .await?;

    assert_eq!(respuesta.status(), StatusCode::CREATED, "RF-1: 201");
    let bytes = to_bytes(respuesta.into_body(), usize::MAX).await?;
    let json: Value = serde_json::from_slice(&bytes)?;

    // RF-2: recortado, año y número numéricos, día en mayúsculas del catálogo.
    assert_eq!(json["school_year"], anio);
    assert_eq!(json["lapse_number"], 1);
    assert_eq!(json["practice_day"], "THURSDAY");
    assert_eq!(json["status"], "PLANNED", "RF-1: nace planeado");
    assert_eq!(json["start_date"], format!("{anio}-02-01"));
    assert_eq!(json["end_date"], format!("{anio}-12-20"));

    let id = Uuid::parse_str(json["id"].as_str().unwrap())?;
    limpiar_lapso(&pool, id).await?;
    Ok(())
}

#[tokio::test]
async fn alta_con_anio_fuera_de_rango_responde_400() -> Resultado {
    let pool = common::pool().await?;
    limpiar_residuo_previo(&pool).await?;
    let app = build_router(pool);

    for malo in ["1999", "2101", "dos mil"] {
        let mut cuerpo = cuerpo_lapso(anio_escolar_unico(), "1");
        cuerpo["school_year"] = Value::from(malo);
        let respuesta = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/lapses")
                    .header("content-type", "application/json")
                    .body(Body::from(cuerpo.to_string()))?,
            )
            .await?;
        assert_eq!(respuesta.status(), StatusCode::BAD_REQUEST, "año «{malo}» (RF-2)");
        let bytes = to_bytes(respuesta.into_body(), usize::MAX).await?;
        let json: Value = serde_json::from_slice(&bytes)?;
        assert!(
            json["error"].as_str().unwrap_or("").contains("año escolar"),
            "el mensaje nombra el campo: {json}"
        );
    }

    Ok(())
}

#[tokio::test]
async fn alta_con_numero_fuera_de_rango_responde_400() -> Resultado {
    let pool = common::pool().await?;
    limpiar_residuo_previo(&pool).await?;
    let app = build_router(pool);

    for malo in ["0", "4", "dos"] {
        let mut cuerpo = cuerpo_lapso(anio_escolar_unico(), "1");
        cuerpo["lapse_number"] = Value::from(malo);
        let respuesta = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/lapses")
                    .header("content-type", "application/json")
                    .body(Body::from(cuerpo.to_string()))?,
            )
            .await?;
        assert_eq!(respuesta.status(), StatusCode::BAD_REQUEST, "número «{malo}» (RF-2)");
        let bytes = to_bytes(respuesta.into_body(), usize::MAX).await?;
        let json: Value = serde_json::from_slice(&bytes)?;
        assert!(
            json["error"].as_str().unwrap_or("").contains("número de lapso"),
            "el mensaje nombra el campo: {json}"
        );
    }

    Ok(())
}

#[tokio::test]
async fn alta_con_fechas_invertidas_responde_400() -> Resultado {
    let pool = common::pool().await?;
    limpiar_residuo_previo(&pool).await?;
    let app = build_router(pool);

    let mut cuerpo = cuerpo_lapso(anio_escolar_unico(), "1");
    let anio_original = cuerpo["school_year"].as_str().unwrap().to_string();
    cuerpo["start_date"] = Value::from(format!("{anio_original}-12-20"));
    cuerpo["end_date"] = Value::from(format!("{anio_original}-02-01"));

    let respuesta = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/lapses")
                .header("content-type", "application/json")
                .body(Body::from(cuerpo.to_string()))?,
        )
        .await?;
    assert_eq!(respuesta.status(), StatusCode::BAD_REQUEST, "RF-2: fin antes de inicio");
    let bytes = to_bytes(respuesta.into_body(), usize::MAX).await?;
    let json: Value = serde_json::from_slice(&bytes)?;
    assert!(
        json["error"].as_str().unwrap_or("").contains("fecha de fin"),
        "el mensaje nombra el campo: {json}"
    );

    Ok(())
}

#[tokio::test]
async fn alta_con_dia_invalido_responde_400_y_enumera_los_dias() -> Resultado {
    let pool = common::pool().await?;
    limpiar_residuo_previo(&pool).await?;
    let app = build_router(pool);

    let mut cuerpo = cuerpo_lapso(anio_escolar_unico(), "1");
    cuerpo["practice_day"] = Value::from("LUNES");

    let respuesta = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/lapses")
                .header("content-type", "application/json")
                .body(Body::from(cuerpo.to_string()))?,
        )
        .await?;
    assert_eq!(respuesta.status(), StatusCode::BAD_REQUEST, "RF-2: día fuera del catálogo");
    let bytes = to_bytes(respuesta.into_body(), usize::MAX).await?;
    let json: Value = serde_json::from_slice(&bytes)?;
    let error = json["error"].as_str().unwrap_or("");
    assert!(error.contains("día de práctica"), "mensaje: {error}");
    assert!(error.contains("MONDAY"), "el mensaje enumera los días: {error}");

    Ok(())
}

#[tokio::test]
async fn alta_con_campos_faltantes_responde_400() -> Resultado {
    let pool = common::pool().await?;
    limpiar_residuo_previo(&pool).await?;
    let app = build_router(pool);

    // Sin `school_year`: el extractor `Json` de axum lo rechaza. La 0.8
    // responde 422 (cuerpo malformado), no 400; los 400 de RF-2 (año, número,
    // fechas, día) salen del dominio, no del extractor.
    let cuerpo = serde_json::json!({
        "lapse_number": "1",
        "start_date": "2050-02-01",
        "end_date": "2050-12-20",
        "practice_day": "THURSDAY",
    });
    let respuesta = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/lapses")
                .header("content-type", "application/json")
                .body(Body::from(cuerpo.to_string()))?,
        )
        .await?;
    assert_eq!(respuesta.status(), StatusCode::UNPROCESSABLE_ENTITY, "axum 0.8: 422");

    Ok(())
}

#[tokio::test]
async fn alta_duplicada_en_mismo_anio_y_numero_responde_409() -> Resultado {
    let pool = common::pool().await?;
    limpiar_residuo_previo(&pool).await?;
    let app = build_router(pool.clone());
    let anio = anio_escolar_unico();

    let primero = crear_lapso(app.clone(), anio).await?;

    let respuesta = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/lapses")
                .header("content-type", "application/json")
                .body(Body::from(cuerpo_lapso(anio, "1").to_string()))?,
        )
        .await?;
    assert_eq!(respuesta.status(), StatusCode::CONFLICT, "RF-3: (año, número) repetido");
    let bytes = to_bytes(respuesta.into_body(), usize::MAX).await?;
    let json: Value = serde_json::from_slice(&bytes)?;
    assert!(
        json["error"].as_str().unwrap_or("").contains("año escolar"),
        "el mensaje explica el conflicto: {json}"
    );

    limpiar_lapso(&pool, primero).await?;
    Ok(())
}

#[tokio::test]
async fn alta_solapada_responde_409() -> Resultado {
    let pool = common::pool().await?;
    limpiar_residuo_previo(&pool).await?;
    let app = build_router(pool.clone());
    let anio = anio_escolar_unico();

    let primero = crear_lapso(app.clone(), anio).await?;

    // Solape parcial con el anterior, dentro del mismo año y del mismo test: el
    // EXCLUDE `daterange '[]' WITH &&` lo detecta aunque el número difiera.
    let mut cuerpo = cuerpo_lapso(anio, "2");
    cuerpo["start_date"] = Value::from(format!("{anio}-06-01"));

    let respuesta = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/lapses")
                .header("content-type", "application/json")
                .body(Body::from(cuerpo.to_string()))?,
        )
        .await?;
    assert_eq!(respuesta.status(), StatusCode::CONFLICT, "RF-4: fechas solapadas");
    let bytes = to_bytes(respuesta.into_body(), usize::MAX).await?;
    let json: Value = serde_json::from_slice(&bytes)?;
    assert!(
        json["error"].as_str().unwrap_or("").contains("solapan"),
        "el mensaje explica el conflicto: {json}"
    );

    limpiar_lapso(&pool, primero).await?;
    Ok(())
}

// --- Lecturas: RF-5, RF-6, RF-7 ---

#[tokio::test]
async fn listado_200_incluye_el_lapso_creado() -> Resultado {
    let pool = common::pool().await?;
    limpiar_residuo_previo(&pool).await?;
    let app = build_router(pool.clone());
    let anio = anio_escolar_unico();

    let id = crear_lapso(app.clone(), anio).await?;

    // RF-5: la lectura devuelve el arreglo completo, con la fila recién creada.
    let respuesta = app
        .oneshot(Request::builder().uri("/lapses").body(Body::empty())?)
        .await?;
    assert_eq!(respuesta.status(), StatusCode::OK, "RF-5: listado 200");
    let bytes = to_bytes(respuesta.into_body(), usize::MAX).await?;
    let lista: Value = serde_json::from_slice(&bytes)?;
    let ids_creados: Vec<Uuid> = lista
        .as_array()
        .unwrap()
        .iter()
        .map(|l| Uuid::parse_str(l["id"].as_str().unwrap()).unwrap())
        .collect();
    assert!(ids_creados.contains(&id), "el lapso recién creado aparece");

    limpiar_lapso(&pool, id).await?;
    Ok(())
}

#[tokio::test]
async fn consulta_200_devuelve_el_lapso_normalizado() -> Resultado {
    let pool = common::pool().await?;
    limpiar_residuo_previo(&pool).await?;
    let app = build_router(pool.clone());
    let anio = anio_escolar_unico();

    let id = crear_lapso(app.clone(), anio).await?;

    let respuesta = app
        .oneshot(
            Request::builder()
                .uri(format!("/lapses/{id}"))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(respuesta.status(), StatusCode::OK, "RF-6: consulta por id");
    let bytes = to_bytes(respuesta.into_body(), usize::MAX).await?;
    let json: Value = serde_json::from_slice(&bytes)?;
    assert_eq!(json["school_year"], anio);
    assert_eq!(json["status"], "PLANNED");

    limpiar_lapso(&pool, id).await?;
    Ok(())
}

#[tokio::test]
async fn consulta_inexistente_responde_404() -> Resultado {
    let pool = common::pool().await?;
    limpiar_residuo_previo(&pool).await?;
    let app = build_router(pool);

    let respuesta = app
        .oneshot(
            Request::builder()
                .uri(format!("/lapses/{}", Uuid::new_v4()))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(respuesta.status(), StatusCode::NOT_FOUND, "RF-7: 404 al consultar");

    Ok(())
}

// --- Edición: RF-8, RF-10, RF-11 (los 409 sobre CLOSED viven en T6) ---

#[tokio::test]
async fn edicion_200_normaliza_y_mantiene_el_estado() -> Resultado {
    let pool = common::pool().await?;
    limpiar_residuo_previo(&pool).await?;
    let app = build_router(pool.clone());
    let anio = anio_escolar_unico();

    let id = crear_lapso(app.clone(), anio).await?;

    let respuesta = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/lapses/{id}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "school_year": " 2048 ",
                        "lapse_number": " 2 ",
                        "start_date": format!(" {anio}-02-01 "),
                        "end_date": format!(" {anio}-12-20 "),
                        "practice_day": " friday ",
                    })
                    .to_string(),
                ))?,
        )
        .await?;
    assert_eq!(respuesta.status(), StatusCode::OK, "RF-8: edición estructural 200");
    let bytes = to_bytes(respuesta.into_body(), usize::MAX).await?;
    let json: Value = serde_json::from_slice(&bytes)?;
    assert_eq!(json["school_year"], 2048, "RF-8: la edición valida igual que el alta");
    assert_eq!(json["lapse_number"], 2);
    assert_eq!(json["practice_day"], "FRIDAY");
    assert_eq!(json["status"], "PLANNED", "RF-8: no toca el estado");

    limpiar_lapso(&pool, id).await?;
    Ok(())
}

#[tokio::test]
async fn edicion_hacia_un_anio_y_numero_ajenos_responde_409() -> Resultado {
    let pool = common::pool().await?;
    limpiar_residuo_previo(&pool).await?;
    let app = build_router(pool.clone());

    let anio_a = anio_escolar_unico();
    let anio_b = anio_escolar_unico();
    let id_a = crear_lapso(app.clone(), anio_a).await?;
    // B nace como (anio_b, 2): el par que el PATCH de A intentará robar.
    let id_b = {
        let respuesta = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/lapses")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        cuerpo_lapso(anio_b, "2").to_string(),
                    ))?,
            )
            .await?;
        assert_eq!(respuesta.status(), StatusCode::CREATED);
        let bytes = to_bytes(respuesta.into_body(), usize::MAX).await?;
        let json: Value = serde_json::from_slice(&bytes)?;
        Uuid::parse_str(json["id"].as_str().unwrap())?
    };

    // Edito A hacia el (año, número) que ya ocupa B, sin tocar sus fechas: el
    // índice único `uq_lapses_year_number` responde 23505 → 409.
    let respuesta = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/lapses/{id_a}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "school_year": anio_b.to_string(),
                        "lapse_number": "2",
                        "start_date": format!("{anio_a}-02-01"),
                        "end_date": format!("{anio_a}-12-20"),
                        "practice_day": "THURSDAY",
                    })
                    .to_string(),
                ))?,
        )
        .await?;
    assert_eq!(respuesta.status(), StatusCode::CONFLICT, "RF-10: combinación ajena");
    let bytes = to_bytes(respuesta.into_body(), usize::MAX).await?;
    let json: Value = serde_json::from_slice(&bytes)?;
    assert!(
        json["error"].as_str().unwrap_or("").contains("año escolar"),
        "el mensaje explica el conflicto: {json}"
    );

    limpiar_lapso(&pool, id_a).await?;
    limpiar_lapso(&pool, id_b).await?;
    Ok(())
}

#[tokio::test]
async fn edicion_inexistente_responde_404() -> Resultado {
    let pool = common::pool().await?;
    limpiar_residuo_previo(&pool).await?;
    let app = build_router(pool);

    let respuesta = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/lapses/{}", Uuid::new_v4()))
                .header("content-type", "application/json")
                .body(Body::from(cuerpo_lapso(anio_escolar_unico(), "1").to_string()))?,
        )
        .await?;
    assert_eq!(respuesta.status(), StatusCode::NOT_FOUND);

    Ok(())
}

// --- Ciclo de vida: RF-12, RF-13, RF-14, RF-16 (el cierre feliz y los 409
// sobre CLOSED viven en T6) ---

#[tokio::test]
async fn activacion_200_pasa_a_activo() -> Resultado {
    let pool = common::pool().await?;
    limpiar_residuo_previo(&pool).await?;
    let app = build_router(pool.clone());
    let anio = anio_escolar_unico();
    let _guard = CICLO_DE_VIDA.lock().await;

    let id = crear_lapso(app.clone(), anio).await?;

    let respuesta = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/lapses/{id}/activar"))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(respuesta.status(), StatusCode::OK, "RF-12: activar 200");
    let bytes = to_bytes(respuesta.into_body(), usize::MAX).await?;
    let json: Value = serde_json::from_slice(&bytes)?;
    assert_eq!(json["status"], "ACTIVE", "RF-12: pasa a activo");

    limpiar_lapso(&pool, id).await?;
    Ok(())
}

#[tokio::test]
async fn activar_un_segundo_lapso_con_otro_activo_responde_409() -> Resultado {
    let pool = common::pool().await?;
    limpiar_residuo_previo(&pool).await?;
    let app = build_router(pool.clone());
    let _guard = CICLO_DE_VIDA.lock().await;

    let anio_a = anio_escolar_unico();
    let anio_b = anio_escolar_unico();
    let id_a = crear_lapso(app.clone(), anio_a).await?;
    let id_b = crear_lapso(app.clone(), anio_b).await?;

    let activado = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/lapses/{id_a}/activar"))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(activado.status(), StatusCode::OK, "RF-12: el primero se activa");

    let respuesta = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/lapses/{id_b}/activar"))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(respuesta.status(), StatusCode::CONFLICT, "RF-13: un solo activo");
    let bytes = to_bytes(respuesta.into_body(), usize::MAX).await?;
    let json: Value = serde_json::from_slice(&bytes)?;
    assert!(
        json["error"].as_str().unwrap_or("").contains("admite uno"),
        "el mensaje explica el conflicto: {json}"
    );

    limpiar_lapso(&pool, id_a).await?;
    limpiar_lapso(&pool, id_b).await?;
    Ok(())
}

#[tokio::test]
async fn activar_un_lapso_ya_activo_responde_409() -> Resultado {
    let pool = common::pool().await?;
    limpiar_residuo_previo(&pool).await?;
    let app = build_router(pool.clone());
    let anio = anio_escolar_unico();
    let _guard = CICLO_DE_VIDA.lock().await;

    let id = crear_lapso(app.clone(), anio).await?;
    let activado = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/lapses/{id}/activar"))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(activado.status(), StatusCode::OK);

    // RF-14: segunda activación sobre un ACTIVE → 409 del dominio, no 200.
    let respuesta = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/lapses/{id}/activar"))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(respuesta.status(), StatusCode::CONFLICT, "RF-14: activar un activo");
    let bytes = to_bytes(respuesta.into_body(), usize::MAX).await?;
    let json: Value = serde_json::from_slice(&bytes)?;
    assert!(
        json["error"].as_str().unwrap_or("").contains("PLANNED"),
        "el mensaje dice qué estado espera: {json}"
    );

    limpiar_lapso(&pool, id).await?;
    Ok(())
}

#[tokio::test]
async fn cerrar_un_lapso_planeado_responde_409() -> Resultado {
    let pool = common::pool().await?;
    limpiar_residuo_previo(&pool).await?;
    let app = build_router(pool.clone());
    let anio = anio_escolar_unico();
    let _guard = CICLO_DE_VIDA.lock().await;

    let id = crear_lapso(app.clone(), anio).await?;

    // RF-16: cerrar solo desde ACTIVE; un PLANNED da 409 del dominio.
    let respuesta = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/lapses/{id}/cerrar"))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(respuesta.status(), StatusCode::CONFLICT, "RF-16: cerrar un planeado");
    let bytes = to_bytes(respuesta.into_body(), usize::MAX).await?;
    let json: Value = serde_json::from_slice(&bytes)?;
    assert!(
        json["error"].as_str().unwrap_or("").contains("ACTIVE"),
        "el mensaje dice qué estado espera: {json}"
    );

    limpiar_lapso(&pool, id).await?;
    Ok(())
}

#[tokio::test]
async fn activar_inexistente_responde_404() -> Resultado {
    let pool = common::pool().await?;
    limpiar_residuo_previo(&pool).await?;
    let app = build_router(pool);

    let respuesta = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/lapses/{}/activar", Uuid::new_v4()))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(respuesta.status(), StatusCode::NOT_FOUND);

    Ok(())
}

// --- Borrado: RF-17 (los rechazos de RF-18 viven en T6) ---

#[tokio::test]
async fn eliminacion_204_sin_cuerpo_y_el_lapso_desaparece() -> Resultado {
    let pool = common::pool().await?;
    limpiar_residuo_previo(&pool).await?;
    let app = build_router(pool.clone());
    let anio = anio_escolar_unico();

    let id = crear_lapso(app.clone(), anio).await?;

    let respuesta = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/lapses/{id}"))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(respuesta.status(), StatusCode::NO_CONTENT, "RF-17: 204 sin cuerpo");
    let bytes = to_bytes(respuesta.into_body(), usize::MAX).await?;
    assert!(bytes.is_empty(), "el 204 no debe tener cuerpo");

    let quedan: i64 = sqlx::query_scalar("SELECT count(*) FROM lapses WHERE id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await?;
    assert_eq!(quedan, 0, "la eliminación es física (RF-17)");

    Ok(())
}

#[tokio::test]
async fn eliminacion_inexistente_responde_404() -> Resultado {
    let pool = common::pool().await?;
    limpiar_residuo_previo(&pool).await?;
    let app = build_router(pool);

    let respuesta = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/lapses/{}", Uuid::new_v4()))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(respuesta.status(), StatusCode::NOT_FOUND);

    Ok(())
}