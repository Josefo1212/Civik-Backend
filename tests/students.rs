//! T3 — el alta de un estudiante contra la base real.
//!
//! Qué se prueba acá y cómo se relaciona con la spec:
//! - RF-1: `crear` persiste la fila y devuelve su identificador.
//! - RF-3, RF-4: la sección se valida dentro de la misma transacción —
//!   inexistente → `SeccionNoExiste`, inactiva → `SeccionInactiva`.
//! - RF-5: la cédula duplicada se rechaza con la restricción real, también si
//!   la dueña está inactiva (la cédula nunca se libera).
//! - RF-6, RF-7: lo que normalizó `domain::preparar_entrada` se guarda tal
//!   cual; la cédula ausente queda `NULL` y dos estudiantes sin cédula
//!   conviven.
//! - RF-21: el alta deja su entrada de "alta" en el historial.
//!
//! Los casos de validación pura (RF-2, RF-12) viven en los tests de
//! `domain.rs`; acá se ejercita el camino completo hasta la base de datos.

mod common;

use civik_backend::modules::sections::domain::{DatosSeccion, Grado, Turno};
use civik_backend::modules::sections::queries as secciones;
use civik_backend::modules::students::domain::preparar_entrada;
use civik_backend::modules::students::queries::{
    consultar, crear, listar_por_seccion, ErrorEstudiante,
};
use civik_backend::shared::error::AppError;
use uuid::Uuid;

type Resultado = Result<(), Box<dyn std::error::Error>>;

/// Una sección de prueba, creada con el propio pipeline de `sections`
/// (normaliza y audita, como en producción).
async fn crear_seccion(tx: &mut sqlx::PgTransaction<'_>, seccion: &str) -> secciones::Seccion {
    let datos = DatosSeccion {
        grado: Grado::Primero,
        seccion: seccion.to_string(),
        turno: Turno::Matutino,
    };
    secciones::crear(tx, datos)
        .await
        .expect("la sección de prueba debe crearse")
}

async fn datos_de_prueba(
    id_seccion: Uuid,
    apellido: &str,
    nombre: &str,
    cedula: Option<&str>,
) -> civik_backend::modules::students::domain::DatosEstudiante {
    preparar_entrada(id_seccion, apellido, nombre, cedula, None).unwrap()
}

#[tokio::test]
async fn el_alta_persiste_la_fila_y_devuelve_identificador() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let seccion = crear_seccion(&mut tx, "e1").await;
    let datos = datos_de_prueba(seccion.id, "García", "Ana María", Some("V-100")).await;
    let estudiante = crear(&mut tx, datos).await?;

    assert_ne!(estudiante.id, Uuid::nil(), "RF-1: devuelve identificador");
    assert_eq!(estudiante.id_section, seccion.id);
    assert!(estudiante.active);

    // Contraste contra la fila real, no solo contra lo que devuelve `crear`.
    let (id_section, first_name, last_name) = sqlx::query_as::<_, (Uuid, String, String)>(
        "SELECT id_section, first_name, last_name FROM students WHERE id = $1",
    )
    .bind(estudiante.id)
    .fetch_one(&mut *tx)
    .await?;

    assert_eq!(id_section, seccion.id);
    assert_eq!(
        first_name, "Ana María",
        "RF-7: colapsado, capitalización intacta"
    );
    assert_eq!(last_name, "García");

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn el_alta_deja_su_entrada_en_el_historial() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let seccion = crear_seccion(&mut tx, "e2").await;
    let datos = datos_de_prueba(seccion.id, "Pérez", "Luis", Some("V-101")).await;
    let estudiante = crear(&mut tx, datos).await?;

    let (entity, action, data): (String, String, serde_json::Value) = sqlx::query_as(
        "SELECT entity, action, data FROM audit_logs \
         WHERE entity = 'students' AND entity_id = $1 AND action = 'alta'",
    )
    .bind(estudiante.id)
    .fetch_one(&mut *tx)
    .await?;

    assert_eq!(entity, "students");
    assert_eq!(action, "alta", "RF-21: la acción se registra en español");
    assert!(
        data["apellido"] == "Pérez",
        "la auditoría guarda los valores: {data}"
    );

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn el_alta_en_seccion_inexistente_informa_que_no_existe() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let datos = datos_de_prueba(Uuid::new_v4(), "García", "Ana", None).await;
    let resultado = crear(&mut tx, datos).await;

    assert!(
        matches!(resultado, Err(ErrorEstudiante::SeccionNoExiste)),
        "sección inexistente → SeccionNoExiste (RF-3), llegó: {resultado:?}"
    );

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn el_alta_en_seccion_inactiva_informa_que_esta_dada_de_baja() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let seccion = crear_seccion(&mut tx, "e3").await;
    secciones::set_active(&mut tx, seccion.id, false).await?;

    let datos = datos_de_prueba(seccion.id, "García", "Ana", None).await;
    let resultado = crear(&mut tx, datos).await;

    assert!(
        matches!(resultado, Err(ErrorEstudiante::SeccionInactiva)),
        "sección inactiva → SeccionInactiva (RF-4), llegó: {resultado:?}"
    );

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn el_alta_de_un_estudiante_con_cedula_duplicada_falla() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let seccion = crear_seccion(&mut tx, "e4").await;
    crear(
        &mut tx,
        datos_de_prueba(seccion.id, "García", "Ana", Some("V-200")).await,
    )
    .await?;

    let resultado = crear(
        &mut tx,
        datos_de_prueba(seccion.id, "López", "María", Some("V-200")).await,
    )
    .await;

    let Err(ErrorEstudiante::Sqlx(error)) = resultado else {
        panic!("una cédula repetida debe fallar (RF-5), llegó: {resultado:?}");
    };

    if let sqlx::Error::Database(base) = &error {
        assert_eq!(base.code().as_deref(), Some("23505"));
        assert_eq!(base.constraint(), Some("uq_students_national_id"));
    } else {
        panic!("esperaba un error de base de datos, llegó: {error}");
    }

    let app_error: AppError = error.into();
    assert!(matches!(app_error, AppError::Conflict(_)));
    assert!(
        app_error.to_string().contains("cédula"),
        "el mensaje debe nombrar la cédula: {app_error}"
    );

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn la_cedula_de_un_estudiante_inactivo_no_se_libera() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let seccion = crear_seccion(&mut tx, "e5").await;
    let primera = crear(
        &mut tx,
        datos_de_prueba(seccion.id, "García", "Ana", Some("V-300")).await,
    )
    .await?;

    // Baja directa: `set_active` de students llega en T6.
    sqlx::query("UPDATE students SET active = false WHERE id = $1")
        .bind(primera.id)
        .execute(&mut *tx)
        .await?;

    let resultado = crear(
        &mut tx,
        datos_de_prueba(seccion.id, "López", "María", Some("V-300")).await,
    )
    .await;

    assert!(
        matches!(resultado, Err(ErrorEstudiante::Sqlx(_))),
        "la cédula de un inactivo sigue ocupada (RF-5, caso límite): {resultado:?}"
    );

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn consultar_devuelve_el_estudiante_incluido_su_estado() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let seccion = crear_seccion(&mut tx, "e7").await;
    let estudiante = crear(
        &mut tx,
        datos_de_prueba(seccion.id, "García", "Ana", Some("V-400")).await,
    )
    .await?;

    let leido = consultar(&mut *tx, estudiante.id).await?;
    assert_eq!(leido.id, estudiante.id, "RF-10: consulta por id");
    assert_eq!(leido.id_section, seccion.id, "RF-10: dice su sección");
    assert_eq!(leido.first_name, "Ana");
    assert!(leido.active, "RF-10: dice su estado");

    // Un estudiante dado de baja también se consulta, marcado como inactivo
    // (caso límite de la spec).
    sqlx::query("UPDATE students SET active = false WHERE id = $1")
        .bind(estudiante.id)
        .execute(&mut *tx)
        .await?;

    let leido = consultar(&mut *tx, estudiante.id).await?;
    assert!(
        !leido.active,
        "el inactivo se devuelve marcado como inactivo"
    );

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn consultar_un_identificador_inexistente_responde_que_no_se_encontro() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let resultado = consultar(&mut *tx, Uuid::new_v4()).await;

    assert!(
        matches!(resultado, Err(ErrorEstudiante::EstudianteNoExiste)),
        "id inexistente → EstudianteNoExiste (RF-11), llegó: {resultado:?}"
    );

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn el_listado_por_defecto_devuelve_solo_activos_en_orden() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let seccion = crear_seccion(&mut tx, "e8").await;
    let ana = crear(
        &mut tx,
        datos_de_prueba(seccion.id, "Pérez", "Ana", None).await,
    )
    .await?;
    crear(
        &mut tx,
        datos_de_prueba(seccion.id, "García", "luis", None).await,
    )
    .await?;
    crear(
        &mut tx,
        datos_de_prueba(seccion.id, "Pérez", "beatriz", None).await,
    )
    .await?;
    // Uno inactivo: no debe aparecer en el listado por defecto (RF-17).
    sqlx::query("UPDATE students SET active = false WHERE id = $1")
        .bind(ana.id)
        .execute(&mut *tx)
        .await?;

    let listado = listar_por_seccion(
        &mut tx, seccion.id, false, // RF-8: por defecto, solo activos
    )
    .await?;

    let orden: Vec<(String, String)> = listado
        .iter()
        .map(|estudiante| (estudiante.last_name.clone(), estudiante.first_name.clone()))
        .collect();
    assert_eq!(
        orden,
        vec![
            ("García".to_string(), "luis".to_string()),
            ("Pérez".to_string(), "beatriz".to_string()),
        ],
        "ordena por lower(apellido), lower(nombre) e ignora mayúsculas"
    );

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn incluir_inactivas_devuelve_tambien_las_inactivas_marcadas() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let seccion = crear_seccion(&mut tx, "e9").await;
    let inactivo = crear(
        &mut tx,
        datos_de_prueba(seccion.id, "Rojas", "Carlos", None).await,
    )
    .await?;
    crear(
        &mut tx,
        datos_de_prueba(seccion.id, "Acosta", "María", None).await,
    )
    .await?;
    sqlx::query("UPDATE students SET active = false WHERE id = $1")
        .bind(inactivo.id)
        .execute(&mut *tx)
        .await?;

    let listado = listar_por_seccion(&mut tx, seccion.id, true).await?;

    assert_eq!(listado.len(), 2, "RF-9: incluye a los inactivos");
    assert!(
        listado.iter().any(|estudiante| !estudiante.active),
        "RF-9: la lista marca el estado de cada uno"
    );

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn el_listado_de_una_seccion_inactiva_sigue_funcionando() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let seccion = crear_seccion(&mut tx, "e10").await;
    crear(
        &mut tx,
        datos_de_prueba(seccion.id, "García", "Ana", None).await,
    )
    .await?;
    secciones::set_active(&mut tx, seccion.id, false).await?;

    // La sección dada de baja no recibe cupos nuevos, pero su padrón sigue
    // siendo consultable (caso límite de la spec).
    let listado = listar_por_seccion(&mut tx, seccion.id, false).await?;
    assert_eq!(listado.len(), 1);

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn el_listado_de_una_seccion_inexistente_responde_que_no_existe() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let resultado = listar_por_seccion(&mut tx, Uuid::new_v4(), false).await;

    assert!(
        matches!(resultado, Err(ErrorEstudiante::SeccionNoExiste)),
        "sección inexistente → SeccionNoExiste (RF-11 del listado), llegó: {resultado:?}"
    );

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn el_orden_del_listado_es_estable_entre_llamadas() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let seccion = crear_seccion(&mut tx, "e11").await;
    crear(
        &mut tx,
        datos_de_prueba(seccion.id, "García", "Ana", None).await,
    )
    .await?;
    crear(
        &mut tx,
        datos_de_prueba(seccion.id, "García", "Ana", None).await,
    )
    .await?;

    // RNF-3: dos dispositivos sin conexión deben poder reconciliar; el orden
    // no cambia entre llamadas ni entre estudiantes de nombre idéntico.
    let primera = listar_por_seccion(&mut tx, seccion.id, false).await?;
    let segunda = listar_por_seccion(&mut tx, seccion.id, false).await?;

    assert_eq!(primera.len(), 2);
    let ids_primera: Vec<Uuid> = primera.iter().map(|e| e.id).collect();
    let ids_segunda: Vec<Uuid> = segunda.iter().map(|e| e.id).collect();
    assert_eq!(ids_primera, ids_segunda, "el orden must ser determinista");

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn dos_estudiantes_sin_cedula_conviven() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let seccion = crear_seccion(&mut tx, "e6").await;
    crear(
        &mut tx,
        datos_de_prueba(seccion.id, "García", "Ana", None).await,
    )
    .await?;
    crear(
        &mut tx,
        datos_de_prueba(seccion.id, "López", "María", Some("  ")).await,
    )
    .await?;

    // RF-7: la ausencia de cédula es NULL y no colisiona con la unicidad.
    let sin_cedula: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM students WHERE id_section = $1 AND national_id IS NULL",
    )
    .bind(seccion.id)
    .fetch_one(&mut *tx)
    .await?;

    assert_eq!(sin_cedula, 2);

    tx.rollback().await?;
    Ok(())
}
