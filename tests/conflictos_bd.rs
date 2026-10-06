//! `From<sqlx::Error>` contra errores que Postgres produce de verdad.
//!
//! Los tests unitarios de `shared::error` pasan códigos y nombres escritos a
//! mano; acá se comprueba que la cadena completa —restricción violada →
//! `sqlx::Error` → `AppError` → status HTTP— funciona con el error real.
//!
//! Cada violación vive en su transacción: Postgres deja la transacción
//! abortada al primer error y cualquier sentencia posterior falla.

mod common;

use axum::response::IntoResponse;
use civik_backend::shared::error::AppError;
use uuid::Uuid;

type Resultado = Result<(), Box<dyn std::error::Error>>;

#[tokio::test]
async fn duplicado_real_de_seccion_se_convierte_en_409() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    sqlx::query(
        "INSERT INTO sections (grade_level, section, shift) VALUES ('1ro', 'a', 'MATUTINO')",
    )
    .execute(&mut *tx)
    .await?;

    let resultado = sqlx::query(
        "INSERT INTO sections (grade_level, section, shift) VALUES ('1ro', 'a', 'MATUTINO')",
    )
    .execute(&mut *tx)
    .await;

    let Err(error) = resultado else {
        panic!("la segunda sección debe violar uq_sections_grade_section_shift");
    };

    if let sqlx::Error::Database(base) = &error {
        assert_eq!(base.code().as_deref(), Some("23505"));
        assert_eq!(base.constraint(), Some("uq_sections_grade_section_shift"));
    } else {
        panic!("esperaba un error de base de datos, llegó: {error}");
    }

    let app_error: AppError = error.into();
    assert!(
        matches!(app_error, AppError::Conflict(_)),
        "esperaba Conflict, llegó: {app_error}"
    );
    assert_eq!(
        app_error.into_response().status(),
        axum::http::StatusCode::CONFLICT
    );

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn borrado_violado_con_estudiantes_real_se_convierte_en_409() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let resultado = sqlx::query(
        "INSERT INTO students (id_section, first_name, last_name) VALUES ($1, 'Ana', 'Pérez')",
    )
    .bind(Uuid::new_v4()) // sección que no existe → viola la clave foránea
    .execute(&mut *tx)
    .await;

    let Err(error) = resultado else {
        panic!("insertar un estudiante con la sección inexistente debe violar students_id_section_fkey");
    };

    if let sqlx::Error::Database(base) = &error {
        assert_eq!(base.code().as_deref(), Some("23503"));
        assert_eq!(base.constraint(), Some("students_id_section_fkey"));
    } else {
        panic!("esperaba un error de base de datos, llegó: {error}");
    }

    let app_error: AppError = error.into();
    assert!(
        matches!(app_error, AppError::Conflict(_)),
        "esperaba Conflict, llegó: {app_error}"
    );
    let mensaje = app_error.to_string();
    assert!(
        mensaje.contains("estudiantes"),
        "el mensaje debe nombrar el problema: {mensaje}"
    );

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn un_23505_real_de_otra_restriccion_no_se_convierte_en_409() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO sections (id, grade_level, section, shift) VALUES ($1, '1ro', 'a', 'MATUTINO')")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    // Mismo código que un duplicado de sección (23505), otra restricción.
    let resultado = sqlx::query(
        "INSERT INTO sections (id, grade_level, section, shift) VALUES ($1, '2do', 'b', 'VESPERTINO')",
    )
    .bind(id)
    .execute(&mut *tx)
    .await;

    let Err(error) = resultado else {
        panic!("insertar dos filas con el mismo id debe violar sections_pkey");
    };

    // Compruebo que el error que llega es el que pretendo probar: mismo
    // código 23505 que un duplicado de sección, otra restricción.
    if let sqlx::Error::Database(base) = &error {
        assert_eq!(
            base.code().as_deref(),
            Some("23505"),
            "código: {:?}",
            base.code()
        );
        assert_eq!(base.constraint(), Some("sections_pkey"));
    } else {
        panic!("esperaba un error de base de datos, llegó: {error}");
    }

    let app_error: AppError = error.into();
    assert!(
        !matches!(app_error, AppError::Conflict(_)),
        "un 23505 sobre la llave primaria no es un conflicto de negocio: {app_error}"
    );
    assert_eq!(
        app_error.into_response().status(),
        axum::http::StatusCode::INTERNAL_SERVER_ERROR
    );

    tx.rollback().await?;
    Ok(())
}
