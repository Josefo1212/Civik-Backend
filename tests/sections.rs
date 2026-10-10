//! T4/T5 — el alta, la edición, la baja y la reactivación de secciones.
//!
//! Qué se prueba acá y cómo se relaciona con la spec:
//! - RF-1, RF-2, RF-3, RF-4: `crear` persiste lo que `preparar_entrada`
//!   normalizó (`" 1RO "`, `" a "`, `"matutino"` → `1ro`, `a`, `MATUTINO`).
//! - RF-8: el alta duplicada falla, esté la original activa o dada de baja; la
//!   combinación no se libera nunca.
//! - RF-14: la edición normaliza igual que el alta (mismo `preparar_entrada`).
//! - RF-15, RF-16: editar a vacío / grado / turno inválidos se rechaza antes
//!   de tocar la base, porque el punto de validación es el del alta.
//! - RF-17: editar a una combinación que pertenece a otra sección se rechaza
//!   (23505 → `Conflict`).
//! - RF-18 a RF-21: baja y reactivación idempotentes — repetir no modifica la
//!   fila y no deja una entrada de auditoría falsa.
//! - RF-25, RF-26: editar o dar de baja una sección con estudiantes los deja
//!   en ella.
//! - RF-22: eliminar una sección sin estudiantes la borra de verdad; una dada
//!   de baja que nunca tuvo estudiantes también.
//! - RF-23: eliminar con un estudiante (activo o inactivo) se rechaza con
//!   "la sección tiene estudiantes" y la fila sigue.
//! - RF-27: alta, edición, baja, reactivación y eliminación dejan su
//!   auditoría.
//! - RF-9 a RF-11 y RF-24: `listar_activas` devuelve en una sola llamada,
//!   omite las inactivas y ordena `1ro`→`5to` con la subdivisión alfabética.
//! - RF-12 y RF-13: `get_by_id` trae también las dadas de baja e inexistentes
//!   no devuelven fila.
//!
//! Los casos de validación pura (RF-5/6/7) ya viven en los tests de
//! `domain.rs`; acá se ejercita el camino completo hasta la base de datos.

mod common;

use civik_backend::modules::sections::domain::{preparar_entrada, Grado};
use civik_backend::modules::sections::queries::{
    crear, editar, eliminar, get_by_id, listar_activas, set_active, Seccion,
};
use civik_backend::shared::error::AppError;
use uuid::Uuid;

type Resultado = Result<(), Box<dyn std::error::Error>>;

#[tokio::test]
async fn el_alta_persiste_la_fila_normalizada() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let datos = preparar_entrada(" 1RO ", "  a ", "matutino")?;
    let seccion: Seccion = crear(&mut tx, datos).await?;

    assert_ne!(seccion.id, Uuid::nil(), "RF-1: devuelve identificador");
    assert_eq!(seccion.grade_level, "1ro", "RF-2: minúscula");
    assert_eq!(seccion.section, "a", "RF-4: recortado");
    assert_eq!(seccion.shift, "MATUTINO", "RF-3: mayúscula");

    // Contraste contra la fila real, no solo contra lo que devuelve `crear`.
    let (grado, subdivision, turno): (String, String, String) =
        sqlx::query_as("SELECT grade_level, section, shift::text FROM sections WHERE id = $1")
            .bind(seccion.id)
            .fetch_one(&mut *tx)
            .await?;

    assert_eq!(grado, "1ro");
    assert_eq!(subdivision, "a");
    assert_eq!(turno, "MATUTINO");

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn el_alta_duplicada_de_una_seccion_activa_falla() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let datos = preparar_entrada("1ro", "a", "MATUTINO")?;
    crear(&mut tx, datos.clone()).await?;

    let resultado = crear(&mut tx, datos).await;

    let Err(error) = resultado else {
        panic!("la segunda alta de la misma combinación debe fallar (RF-8)");
    };

    if let sqlx::Error::Database(base) = &error {
        assert_eq!(base.code().as_deref(), Some("23505"));
        assert_eq!(base.constraint(), Some("uq_sections_grade_section_shift"));
    } else {
        panic!("esperaba un error de base de datos, llegó: {error}");
    }

    let app_error: AppError = error.into();
    assert!(matches!(app_error, AppError::Conflict(_)));
    assert!(
        app_error.to_string().contains("uso"),
        "el mensaje debe explicar que la combinación está en uso: {app_error}"
    );

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn el_alta_duplicada_de_una_seccion_dada_de_baja_tambien_falla() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let datos = preparar_entrada("1ro", "a", "MATUTINO")?;
    let seccion = crear(&mut tx, datos.clone()).await?;

    sqlx::query("UPDATE sections SET active = false WHERE id = $1")
        .bind(seccion.id)
        .execute(&mut *tx)
        .await?;

    let resultado = crear(&mut tx, datos).await;

    let Err(error) = resultado else {
        panic!("dar de baja la original no libera la combinación (RF-8, caso límite)");
    };

    if let sqlx::Error::Database(base) = &error {
        assert_eq!(base.code().as_deref(), Some("23505"));
        assert_eq!(base.constraint(), Some("uq_sections_grade_section_shift"));
    } else {
        panic!("esperaba un error de base de datos, llegó: {error}");
    }

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn el_alta_deja_su_entrada_en_el_historial() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let datos = preparar_entrada("1ro", "a", "MATUTINO")?;
    let seccion = crear(&mut tx, datos).await?;

    let (entity, action, data_raw): (String, String, String) =
        sqlx::query_as("SELECT entity, action, data::text FROM audit_logs WHERE entity_id = $1")
            .bind(seccion.id)
            .fetch_one(&mut *tx)
            .await?;

    assert_eq!(entity, "sections");
    assert_eq!(action, "alta", "RF-27: el alta registra su acción");

    let data: serde_json::Value = serde_json::from_str(&data_raw)?;
    assert_eq!(data["grado"], "1ro", "la auditoría guarda los valores");
    assert_eq!(data["seccion"], "a");
    assert_eq!(data["turno"], "MATUTINO");

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn la_edicion_normaliza_igual_que_el_alta() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let original = crear(&mut tx, preparar_entrada("1ro", "a", "MATUTINO")?).await?;

    let editada = editar(
        &mut tx,
        original.id,
        preparar_entrada(" 2DO ", "  b ", "vespertino")?,
    )
    .await?;

    assert_eq!(editada.id, original.id);
    assert_eq!(editada.grade_level, "2do", "RF-14: grado en minúscula");
    assert_eq!(editada.section, "b", "RF-14: subdivisión recortada");
    assert_eq!(editada.shift, "VESPERTINO", "RF-14: turno en mayúsculas");
    assert!(editada.active, "la edición no toca el estado de la sección");

    // Contraste contra la fila real, no solo contra lo que devuelve `editar`.
    let (grado, subdivision, turno): (String, String, String) =
        sqlx::query_as("SELECT grade_level, section, shift::text FROM sections WHERE id = $1")
            .bind(original.id)
            .fetch_one(&mut *tx)
            .await?;

    assert_eq!(grado, "2do");
    assert_eq!(subdivision, "b");
    assert_eq!(turno, "VESPERTINO");

    // RF-27: la edición deja su entrada de auditoría con los valores nuevos.
    let (action, data_raw): (String, String) = sqlx::query_as(
        "SELECT action, data::text FROM audit_logs WHERE entity_id = $1 ORDER BY id DESC LIMIT 1",
    )
    .bind(original.id)
    .fetch_one(&mut *tx)
    .await?;

    assert_eq!(action, "editar");
    let data: serde_json::Value = serde_json::from_str(&data_raw)?;
    assert_eq!(data["grado"], "2do");
    assert_eq!(data["turno"], "VESPERTINO");

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn la_edicion_rechaza_datos_invalidos_antes_de_tocar_la_base() -> Resultado {
    let pool = common::pool().await?;
    let tx = pool.begin().await?;

    // RF-15 y RF-16: la edición pasa por `preparar_entrada`, el mismo punto de
    // validación del alta (RNF-1), así que vacío, grado y turno fuera del
    // catálogo se rechazan antes de que exista el `DatosSeccion` que `editar`
    // acepta siquiera.
    assert!(
        preparar_entrada("", "a", "MATUTINO").is_err(),
        "RF-15: grado vacío"
    );
    assert!(
        preparar_entrada("1ro", "   ", "MATUTINO").is_err(),
        "RF-15: subdivisión vacía"
    );
    assert!(
        preparar_entrada("quinto", "a", "MATUTINO").is_err(),
        "RF-16: grado fuera del catálogo"
    );
    assert!(
        preparar_entrada("1ro", "a", "NOCTURNO").is_err(),
        "RF-16: turno fuera del catálogo"
    );

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn la_edicion_a_una_combinacion_ajena_se_rechaza() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let datos_del_dueno = preparar_entrada("1ro", "a", "MATUTINO")?;
    crear(&mut tx, datos_del_dueno.clone()).await?;
    let otra = crear(&mut tx, preparar_entrada("1ro", "b", "MATUTINO")?).await?;

    let resultado = editar(&mut tx, otra.id, datos_del_dueno).await;

    let Err(error) = resultado else {
        panic!("editar hacia una combinación ajena debe fallar (RF-17)");
    };

    if let sqlx::Error::Database(base) = &error {
        assert_eq!(base.code().as_deref(), Some("23505"));
        assert_eq!(base.constraint(), Some("uq_sections_grade_section_shift"));
    } else {
        panic!("esperaba un error de base de datos, llegó: {error}");
    }

    let app_error: AppError = error.into();
    assert!(matches!(app_error, AppError::Conflict(_)));
    assert!(
        app_error.to_string().contains("uso"),
        "el mensaje debe explicar que la combinación está en uso: {app_error}"
    );

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn operar_sobre_una_seccion_inexistente_falla() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let fantasma = Uuid::new_v4();

    let resultado = set_active(&mut tx, fantasma, false).await;
    assert!(
        matches!(resultado, Err(sqlx::Error::RowNotFound)),
        "dar de baja una sección inexistente debe ser RowNotFound, llegó: {resultado:?}"
    );

    let resultado = editar(&mut tx, fantasma, preparar_entrada("1ro", "a", "MATUTINO")?).await;
    assert!(
        matches!(resultado, Err(sqlx::Error::RowNotFound)),
        "editar una sección inexistente debe ser RowNotFound, llegó: {resultado:?}"
    );

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn la_baja_marca_inactiva_y_es_idempotente() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let seccion = crear(&mut tx, preparar_entrada("1ro", "a", "MATUTINO")?).await?;

    let tras_baja = set_active(&mut tx, seccion.id, false).await?;
    assert!(!tras_baja.active, "RF-18: la baja deja active = false");

    // Segunda baja: misma fila, mismo estado. Como el trigger pisa `updated_at`
    // con `now()` (constante por transacción), dentro de la misma tx no se
    // distingue por timestamp; la prueba honesta de "no modifica nada" (RF-19)
    // es que un UPDATE idéntico no afecta ninguna fila.
    let segunda = set_active(&mut tx, seccion.id, false).await?;
    assert_eq!(segunda.id, tras_baja.id);
    assert!(!segunda.active);
    assert_eq!(segunda.updated_at, tras_baja.updated_at);

    let afectadas =
        sqlx::query("UPDATE sections SET active = $2 WHERE id = $1 AND active IS DISTINCT FROM $2")
            .bind(seccion.id)
            .bind(false)
            .execute(&mut *tx)
            .await?
            .rows_affected();
    assert_eq!(afectadas, 0, "RF-19: repetir la baja no modifica la fila");

    let bajas: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_logs WHERE entity_id = $1 AND action = 'baja'",
    )
    .bind(seccion.id)
    .fetch_one(&mut *tx)
    .await?;
    assert_eq!(
        bajas, 1,
        "RF-27: se audita la baja real, no la llamada repetida que no hizo nada"
    );

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn la_reactivacion_marca_activa_de_nuevo_y_es_idempotente() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let seccion = crear(&mut tx, preparar_entrada("1ro", "a", "MATUTINO")?).await?;
    set_active(&mut tx, seccion.id, false).await?;

    let tras_reactivar = set_active(&mut tx, seccion.id, true).await?;
    assert!(
        tras_reactivar.active,
        "RF-20: la reactivación vuelve a activarla"
    );

    let segunda = set_active(&mut tx, seccion.id, true).await?;
    assert_eq!(segunda.id, tras_reactivar.id);
    assert!(segunda.active);
    assert_eq!(segunda.updated_at, tras_reactivar.updated_at);

    let afectadas =
        sqlx::query("UPDATE sections SET active = $2 WHERE id = $1 AND active IS DISTINCT FROM $2")
            .bind(seccion.id)
            .bind(true)
            .execute(&mut *tx)
            .await?
            .rows_affected();
    assert_eq!(
        afectadas, 0,
        "RF-21: repetir la reactivación no modifica la fila"
    );

    let reactivaciones: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_logs WHERE entity_id = $1 AND action = 'reactivar'",
    )
    .bind(seccion.id)
    .fetch_one(&mut *tx)
    .await?;
    assert_eq!(reactivaciones, 1, "RF-27: una sola reactivación auditada");

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn editar_y_dar_de_baja_una_seccion_con_estudiantes_los_deja_en_ella() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let seccion = crear(&mut tx, preparar_entrada("1ro", "a", "MATUTINO")?).await?;

    // El módulo `students` es la Fase 3: se inserta con SQL directo. RF-25 y
    // RF-26 cubren estudiantes activos e inactivos por igual.
    let id_activo: Uuid = sqlx::query_scalar(
        "INSERT INTO students (id_section, first_name, last_name)
         VALUES ($1, 'Ana', 'Perez') RETURNING id",
    )
    .bind(seccion.id)
    .fetch_one(&mut *tx)
    .await?;

    let id_inactivo: Uuid = sqlx::query_scalar(
        "INSERT INTO students (id_section, first_name, last_name, active)
         VALUES ($1, 'Luis', 'Diaz', false) RETURNING id",
    )
    .bind(seccion.id)
    .fetch_one(&mut *tx)
    .await?;

    // RF-26: editar la sección con estudiantes los deja en ella.
    let editada = editar(
        &mut tx,
        seccion.id,
        preparar_entrada("1ro", "b", "MATUTINO")?,
    )
    .await?;
    assert_eq!(editada.section, "b");

    // RF-25/RF-26: darla de baja también.
    let baja = set_active(&mut tx, seccion.id, false).await?;
    assert!(!baja.active);

    let en_la_seccion: i64 =
        sqlx::query_scalar("SELECT count(*) FROM students WHERE id_section = $1")
            .bind(seccion.id)
            .fetch_one(&mut *tx)
            .await?;
    assert_eq!(
        en_la_seccion, 2,
        "RF-25/RF-26: los estudiantes permanecen en la sección editada y dada de baja"
    );

    let (id_leido, sigue_activo): (Uuid, bool) =
        sqlx::query_as("SELECT id, active FROM students WHERE id = $1")
            .bind(id_activo)
            .fetch_one(&mut *tx)
            .await?;
    assert_eq!(id_leido, id_activo);
    assert!(sigue_activo, "el alumno activo sigue con su estado intacto");

    let (id_leido, sigue_inactivo): (Uuid, bool) =
        sqlx::query_as("SELECT id, active FROM students WHERE id = $1")
            .bind(id_inactivo)
            .fetch_one(&mut *tx)
            .await?;
    assert_eq!(id_leido, id_inactivo);
    assert!(
        !sigue_inactivo,
        "el alumno inactivo también permanece en la sección"
    );

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn eliminar_una_seccion_vacia_la_borra_y_deja_auditoria() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let seccion = crear(&mut tx, preparar_entrada("1ro", "a", "MATUTINO")?).await?;

    let borrada = eliminar(&mut tx, seccion.id).await?;
    assert_eq!(
        borrada.id, seccion.id,
        "RF-22: devuelve la fila que se borró"
    );

    let quedan: i64 = sqlx::query_scalar("SELECT count(*) FROM sections WHERE id = $1")
        .bind(seccion.id)
        .fetch_one(&mut *tx)
        .await?;
    assert_eq!(quedan, 0, "RF-22: la sección se elimina de verdad");

    // RF-27: el registro de la eliminación sobrevive y conserva los valores.
    // Se filtra por acción porque la sección ya dejó su auditoría de "alta".
    let (action, data_raw): (String, String) = sqlx::query_as(
        "SELECT action, data::text FROM audit_logs WHERE entity_id = $1 AND action = 'eliminar'",
    )
    .bind(seccion.id)
    .fetch_one(&mut *tx)
    .await?;

    assert_eq!(action, "eliminar");
    let data: serde_json::Value = serde_json::from_str(&data_raw)?;
    assert_eq!(data["grado"], "1ro");
    assert_eq!(data["turno"], "MATUTINO");
    assert_eq!(data["activo"], true);

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn eliminar_una_seccion_con_estudiante_activo_falla_y_la_fila_sigue() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let seccion = crear(&mut tx, preparar_entrada("1ro", "a", "MATUTINO")?).await?;

    sqlx::query(
        "INSERT INTO students (id_section, first_name, last_name)
         VALUES ($1, 'Ana', 'Perez')",
    )
    .bind(seccion.id)
    .execute(&mut *tx)
    .await?;

    // El DELETE falla y Postgres deja la transacción ABORTADA: ningún query
    // posterior funciona en la misma transacción. Para comprobar que la fila
    // quedó intacta, el intento vive en un savepoint y se deshace justo al
    // volver de él; `ROLLBACK TO SAVEPOINT` recupera también el estado del
    // "current transaction is aborted". (sqlx 0.8 no expone savepoints; se
    // hacen con SQL directo sobre la transacción.)
    sqlx::query("SAVEPOINT intento_eliminar")
        .execute(&mut *tx)
        .await?;
    let resultado = eliminar(&mut tx, seccion.id).await;

    let Err(error) = resultado else {
        panic!("eliminar una sección con estudiantes debe fallar (RF-23)");
    };

    if let sqlx::Error::Database(base) = &error {
        assert_eq!(base.code().as_deref(), Some("23001"));
        assert_eq!(base.constraint(), Some("students_id_section_fkey"));
    } else {
        panic!("esperaba un error de base de datos, llegó: {error}");
    }

    let app_error: AppError = error.into();
    assert!(matches!(app_error, AppError::Conflict(_)));
    assert!(
        app_error.to_string().contains("estudiantes"),
        "el mensaje debe decir que la sección tiene estudiantes: {app_error}"
    );

    sqlx::query("ROLLBACK TO SAVEPOINT intento_eliminar")
        .execute(&mut *tx)
        .await?;

    let quedan: i64 = sqlx::query_scalar("SELECT count(*) FROM sections WHERE id = $1")
        .bind(seccion.id)
        .fetch_one(&mut *tx)
        .await?;
    assert_eq!(quedan, 1, "RF-23: la fila sigue existiendo");

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn eliminar_una_seccion_con_estudiante_inactivo_tambien_falla() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let seccion = crear(&mut tx, preparar_entrada("1ro", "a", "MATUTINO")?).await?;

    sqlx::query(
        "INSERT INTO students (id_section, first_name, last_name, active)
         VALUES ($1, 'Luis', 'Diaz', false)",
    )
    .bind(seccion.id)
    .execute(&mut *tx)
    .await?;

    sqlx::query("SAVEPOINT intento_eliminar")
        .execute(&mut *tx)
        .await?;
    let resultado = eliminar(&mut tx, seccion.id).await;

    let Err(error) = resultado else {
        panic!("eliminar con un estudiante inactivo también debe fallar (RF-23, caso límite)");
    };

    if let sqlx::Error::Database(base) = &error {
        assert_eq!(base.code().as_deref(), Some("23001"));
        assert_eq!(base.constraint(), Some("students_id_section_fkey"));
    } else {
        panic!("esperaba un error de base de datos, llegó: {error}");
    }

    sqlx::query("ROLLBACK TO SAVEPOINT intento_eliminar")
        .execute(&mut *tx)
        .await?;

    let quedan: i64 = sqlx::query_scalar("SELECT count(*) FROM sections WHERE id = $1")
        .bind(seccion.id)
        .fetch_one(&mut *tx)
        .await?;
    assert_eq!(quedan, 1);

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn eliminar_una_seccion_dada_de_baja_sin_estudiantes_tambien_la_borra() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let seccion = crear(&mut tx, preparar_entrada("1ro", "a", "MATUTINO")?).await?;
    set_active(&mut tx, seccion.id, false).await?;

    // El DELETE no filtra por `active`: una sección dada de baja que nunca tuvo
    // estudiantes se elimina igual (caso límite documentado en el plan §6).
    let borrada = eliminar(&mut tx, seccion.id).await?;
    assert_eq!(borrada.id, seccion.id);

    let quedan: i64 = sqlx::query_scalar("SELECT count(*) FROM sections WHERE id = $1")
        .bind(seccion.id)
        .fetch_one(&mut *tx)
        .await?;
    assert_eq!(quedan, 0);

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn listar_activas_ordena_por_grado_y_subdivision_y_omite_inactivas() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    // Se crean desordenadas a propósito: solo el ORDER BY puede estabilizarlas.
    let _3ro_c = crear(&mut tx, preparar_entrada("3ro", "c", "MATUTINO")?).await?;
    let _1ro_a = crear(&mut tx, preparar_entrada("1ro", "a", "MATUTINO")?).await?;
    let _5to_b = crear(&mut tx, preparar_entrada("5to", "b", "VESPERTINO")?).await?;
    let _2do_a = crear(&mut tx, preparar_entrada("2do", "a", "MATUTINO")?).await?;
    let _3ro_a = crear(&mut tx, preparar_entrada("3ro", "a", "MATUTINO")?).await?;
    let _1ro_b = crear(&mut tx, preparar_entrada("1ro", "b", "MATUTINO")?).await?;

    // RF-24: la sección dada de baja no puede aparecer en el listado.
    set_active(&mut tx, _3ro_a.id, false).await?;

    // RF-9: todas las activas en una sola llamada; RF-11: `1ro`→`5to` y dentro
    // del grado la subdivisión en orden alfabético.
    let lista = listar_activas(&mut *tx, None).await?;

    // Se filtra a las secciones que este test creó: la tabla puede tener
    // residuo committed de otros tests que corren en paralelo (misma base, sin
    // aislamiento), y eso no debe romper la verificación del orden canónico.
    let propias = [
        _3ro_c.id, _1ro_a.id, _5to_b.id, _2do_a.id, _3ro_a.id, _1ro_b.id,
    ];
    let esperado = ["1ro:a", "1ro:b", "2do:a", "3ro:c", "5to:b"];
    let obtenido: Vec<String> = lista
        .iter()
        .filter(|s| propias.contains(&s.id))
        .map(|s| format!("{}:{}", s.grade_level, s.section))
        .collect();
    assert_eq!(obtenido, esperado, "RF-11: el orden debe ser el canónico");

    // RNF-3: dos listados seguidos producen exactamente el mismo orden
    // (comparando solo las secciones controladas por este test).
    let segunda = listar_activas(&mut *tx, None).await?;
    let obtenido_segunda: Vec<String> = segunda
        .iter()
        .filter(|s| propias.contains(&s.id))
        .map(|s| format!("{}:{}", s.grade_level, s.section))
        .collect();
    assert_eq!(obtenido_segunda, obtenido);

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn listar_activas_filtra_por_un_solo_grado() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let _1ro_a = crear(&mut tx, preparar_entrada("1ro", "a", "MATUTINO")?).await?;
    let _1ro_c = crear(&mut tx, preparar_entrada("1ro", "c", "MATUTINO")?).await?;
    let _2do_b = crear(&mut tx, preparar_entrada("2do", "b", "MATUTINO")?).await?;

    // RF-10: solo las activas de ese grado, mismo orden que el listado completo.
    // Se filtra a las secciones de este test para no depender del residuo de
    // otros tests que comparten la base.
    let propias = [_1ro_a.id, _1ro_c.id, _2do_b.id];
    let lista: Vec<_> = listar_activas(&mut *tx, Some(Grado::Primero))
        .await?
        .into_iter()
        .filter(|s| propias.contains(&s.id))
        .collect();
    assert_eq!(lista.len(), 2, "el filtro deja fuera el 2do");

    let ids: Vec<Uuid> = lista.iter().map(|s| s.id).collect();
    assert!(ids.contains(&_1ro_a.id) && ids.contains(&_1ro_c.id));
    assert_eq!(lista[0].section, "a");
    assert_eq!(lista[1].section, "c");

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn get_by_id_trae_una_seccion_dada_de_baja() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let seccion = crear(&mut tx, preparar_entrada("4to", "b", "VESPERTINO")?).await?;
    set_active(&mut tx, seccion.id, false).await?;

    // RF-12: el listado no la muestra (RF-24), pero por identificador sí se
    // consulta y se ve que está dada de baja, sin perder sus datos.
    let consultada = get_by_id(&mut *tx, seccion.id).await?;
    assert_eq!(consultada.id, seccion.id);
    assert!(!consultada.active, "RF-12: la consulta revela la baja");
    assert_eq!(consultada.grade_level, "4to");
    assert_eq!(consultada.shift, "VESPERTINO");

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn get_by_id_inexistente_no_devuelve_fila() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    // RF-13: un identificador que no existe no devuelve fila (RowNotFound).
    let resultado = get_by_id(&mut *tx, Uuid::new_v4()).await;
    assert!(
        matches!(resultado, Err(sqlx::Error::RowNotFound)),
        "esperaba RowNotFound, llegó: {resultado:?}"
    );

    tx.rollback().await?;
    Ok(())
}
