//! T3 — el alta de un estudiante contra la base real.
//!
//! Qué se prueba acá y cómo se relaciona con la spec:
//! - RF-1: `crear` persiste la fila y devuelve su identificador.
//! - RF-3, RF-4: la sección se valida dentro de la misma transacción —
//!   inexistente → `SeccionNoExiste`, inactiva → `SeccionInactiva`.
//! - RF-5, RF-6, RF-7: lo que normalizó `domain::preparar_entrada` se guarda
//!   tal cual y el padrón lista e incluye estado.
//! - RF-18: cada cambio deja su entrada en el historial.
//!
//! Los casos de validación pura (RF-2, RF-10, RF-11) viven en los tests de
//! `domain.rs`; acá se ejercita el camino completo hasta la base de datos.

mod common;

use civik_backend::modules::sections::domain::{DatosSeccion, Grado, Turno};
use civik_backend::modules::sections::queries as secciones;
use civik_backend::modules::students::domain::{preparar_entrada, DatosEstudiante};
use civik_backend::modules::students::queries::{
    consultar, crear, editar, listar_por_seccion, set_active, ErrorEstudiante,
};
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
) -> civik_backend::modules::students::domain::DatosEstudiante {
    preparar_entrada(id_seccion, apellido, nombre, None).unwrap()
}

#[tokio::test]
async fn el_alta_persiste_la_fila_y_devuelve_identificador() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let seccion = crear_seccion(&mut tx, "e1").await;
    let datos = datos_de_prueba(seccion.id, "García", "Ana María").await;
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
        "RF-5: colapsado, capitalización intacta"
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
    let datos = datos_de_prueba(seccion.id, "Pérez", "Luis").await;
    let estudiante = crear(&mut tx, datos).await?;

    let (action, data): (String, serde_json::Value) = sqlx::query_as(
        "SELECT action, data FROM audit_logs \
         WHERE entity = 'students' AND entity_id = $1 AND action = 'alta'",
    )
    .bind(estudiante.id)
    .fetch_one(&mut *tx)
    .await?;

    assert_eq!(action, "alta", "RF-18: la acción se registra en español");
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

    let datos = datos_de_prueba(Uuid::new_v4(), "García", "Ana").await;
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

    let datos = datos_de_prueba(seccion.id, "García", "Ana").await;
    let resultado = crear(&mut tx, datos).await;

    assert!(
        matches!(resultado, Err(ErrorEstudiante::SeccionInactiva)),
        "sección inactiva → SeccionInactiva (RF-4), llegó: {resultado:?}"
    );

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn consultar_devuelve_el_estudiante_incluido_su_estado() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let seccion = crear_seccion(&mut tx, "e7").await;
    let estudiante = crear(&mut tx, datos_de_prueba(seccion.id, "García", "Ana").await).await?;

    let leido = consultar(&mut *tx, estudiante.id).await?;
    assert_eq!(leido.id, estudiante.id, "RF-8: consulta por id");
    assert_eq!(leido.id_section, seccion.id, "RF-8: dice su sección");
    assert_eq!(leido.first_name, "Ana");
    assert!(leido.active, "RF-8: dice su estado");

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
        "id inexistente → EstudianteNoExiste (RF-9), llegó: {resultado:?}"
    );

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn el_listado_por_defecto_devuelve_solo_activos_en_orden() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let seccion = crear_seccion(&mut tx, "e8").await;
    let ana = crear(&mut tx, datos_de_prueba(seccion.id, "Pérez", "Ana").await).await?;
    crear(&mut tx, datos_de_prueba(seccion.id, "García", "luis").await).await?;
    crear(
        &mut tx,
        datos_de_prueba(seccion.id, "Pérez", "beatriz").await,
    )
    .await?;
    // Uno inactivo: no debe aparecer en el listado por defecto (RF-14).
    sqlx::query("UPDATE students SET active = false WHERE id = $1")
        .bind(ana.id)
        .execute(&mut *tx)
        .await?;

    let listado = listar_por_seccion(
        &mut tx, seccion.id, false, // RF-6: por defecto, solo activos
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
        datos_de_prueba(seccion.id, "Rojas", "Carlos").await,
    )
    .await?;
    crear(
        &mut tx,
        datos_de_prueba(seccion.id, "Acosta", "María").await,
    )
    .await?;
    sqlx::query("UPDATE students SET active = false WHERE id = $1")
        .bind(inactivo.id)
        .execute(&mut *tx)
        .await?;

    let listado = listar_por_seccion(&mut tx, seccion.id, true).await?;

    assert_eq!(listado.len(), 2, "RF-7: incluye a los inactivos");
    assert!(
        listado.iter().any(|estudiante| !estudiante.active),
        "RF-7: la lista marca el estado de cada uno"
    );

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn el_listado_de_una_seccion_inactiva_sigue_funcionando() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let seccion = crear_seccion(&mut tx, "e10").await;
    crear(&mut tx, datos_de_prueba(seccion.id, "García", "Ana").await).await?;
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
        "sección inexistente → SeccionNoExiste (RF-9 del listado), llegó: {resultado:?}"
    );

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn el_orden_del_listado_es_estable_entre_llamadas() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let seccion = crear_seccion(&mut tx, "e11").await;
    crear(&mut tx, datos_de_prueba(seccion.id, "García", "Ana").await).await?;
    crear(&mut tx, datos_de_prueba(seccion.id, "García", "Ana").await).await?;

    // RNF-3: dos dispositivos sin conexión deben poder reconciliar; el orden
    // no cambia entre llamadas ni entre estudiantes de nombre idéntico.
    let primera = listar_por_seccion(&mut tx, seccion.id, false).await?;
    let segunda = listar_por_seccion(&mut tx, seccion.id, false).await?;

    assert_eq!(primera.len(), 2);
    let ids_primera: Vec<Uuid> = primera.iter().map(|e| e.id).collect();
    let ids_segunda: Vec<Uuid> = segunda.iter().map(|e| e.id).collect();
    assert_eq!(ids_primera, ids_segunda, "el orden debe ser determinista");

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn la_baja_marca_inactivo_y_lo_omite_del_listado_por_defecto() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let seccion = crear_seccion(&mut tx, "e24").await;
    let estudiante = crear(&mut tx, datos_de_prueba(seccion.id, "García", "Ana").await).await?;

    let bajado = set_active(&mut tx, estudiante.id, false).await?;

    assert!(
        !bajado.active,
        "RF-14: la baja lo marca inactivo sin borrarlo"
    );
    let leido = consultar(&mut *tx, estudiante.id).await?;
    assert!(!leido.active, "RF-6: el listado por defecto lo omite");
    assert_eq!(
        listar_por_seccion(&mut tx, seccion.id, false).await?.len(),
        0
    );
    assert_eq!(
        listar_por_seccion(&mut tx, seccion.id, true).await?.len(),
        1
    );

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn bajar_dos_veces_no_modifica_nada_y_es_exitoso() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let seccion = crear_seccion(&mut tx, "e25").await;
    let estudiante = crear(&mut tx, datos_de_prueba(seccion.id, "García", "Ana").await).await?;

    let primera = set_active(&mut tx, estudiante.id, false).await?;
    let (updated_tras_baja,) = sqlx::query_as::<
        _,
        (sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,),
    >("SELECT updated_at FROM students WHERE id = $1")
    .bind(estudiante.id)
    .fetch_one(&mut *tx)
    .await?;

    // RF-15: segundo "baja" sobre el mismo estudiante: misma fila, mismo
    // `updated_at`, respuesta de éxito.
    let segunda = set_active(&mut tx, estudiante.id, false).await?;
    let (updated_tras_repetir,) = sqlx::query_as::<
        _,
        (sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,),
    >("SELECT updated_at FROM students WHERE id = $1")
    .bind(estudiante.id)
    .fetch_one(&mut *tx)
    .await?;

    assert_eq!(primera.id, segunda.id);
    assert!(!segunda.active);
    assert_eq!(
        updated_tras_baja, updated_tras_repetir,
        "RF-15: repetir la baja no modifica nada"
    );

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn la_reactivacion_vuelve_el_estudiante_al_listado() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let seccion = crear_seccion(&mut tx, "e26").await;
    let estudiante = crear(&mut tx, datos_de_prueba(seccion.id, "García", "Ana").await).await?;
    set_active(&mut tx, estudiante.id, false).await?;

    let reactivado = set_active(&mut tx, estudiante.id, true).await?;

    assert!(reactivado.active, "RF-16: la reactivación lo vuelve activo");
    assert_eq!(
        listar_por_seccion(&mut tx, seccion.id, false).await?.len(),
        1
    );

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn reactivar_dos_veces_no_modifica_nada() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let seccion = crear_seccion(&mut tx, "e27").await;
    let estudiante = crear(&mut tx, datos_de_prueba(seccion.id, "García", "Ana").await).await?;

    set_active(&mut tx, estudiante.id, true).await?;
    let (updated_primera,) = sqlx::query_as::<
        _,
        (sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,),
    >("SELECT updated_at FROM students WHERE id = $1")
    .bind(estudiante.id)
    .fetch_one(&mut *tx)
    .await?;

    let segunda = set_active(&mut tx, estudiante.id, true).await?;
    let (updated_segunda,) = sqlx::query_as::<
        _,
        (sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,),
    >("SELECT updated_at FROM students WHERE id = $1")
    .bind(estudiante.id)
    .fetch_one(&mut *tx)
    .await?;

    assert!(segunda.active);
    assert_eq!(
        updated_primera, updated_segunda,
        "RF-17: repetir la reactivación no modifica nada"
    );

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn la_baja_y_la_reactivacion_de_un_estudiante_inexistente_no_se_encontraron() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let inexistente = Uuid::new_v4();
    assert!(matches!(
        set_active(&mut tx, inexistente, false).await,
        Err(ErrorEstudiante::EstudianteNoExiste)
    ));
    assert!(matches!(
        set_active(&mut tx, inexistente, true).await,
        Err(ErrorEstudiante::EstudianteNoExiste)
    ));

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn la_baja_idempotente_no_deja_auditoria_falsa() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let seccion = crear_seccion(&mut tx, "e28").await;
    let estudiante = crear(&mut tx, datos_de_prueba(seccion.id, "García", "Ana").await).await?;

    set_active(&mut tx, estudiante.id, false).await?;
    set_active(&mut tx, estudiante.id, false).await?;

    // RF-18: solo la primera cambió algo y dejó su "baja"; la repetida no
    // escribe una entrada falsa.
    let bajas: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_logs \
         WHERE entity = 'students' AND entity_id = $1 AND action = 'baja'",
    )
    .bind(estudiante.id)
    .fetch_one(&mut *tx)
    .await?;
    assert_eq!(bajas, 1, "RF-18: una sola entrada de baja de verdad");

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn la_reactivacion_deja_su_entrada_de_auditoria() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let seccion = crear_seccion(&mut tx, "e29").await;
    let estudiante = crear(&mut tx, datos_de_prueba(seccion.id, "García", "Ana").await).await?;
    set_active(&mut tx, estudiante.id, false).await?;
    set_active(&mut tx, estudiante.id, true).await?;

    let reactor: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_logs \
         WHERE entity = 'students' AND entity_id = $1 AND action = 'reactivar'",
    )
    .bind(estudiante.id)
    .fetch_one(&mut *tx)
    .await?;
    assert_eq!(reactor, 1, "RF-18: la reactivación de verdad se registra");

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn el_editar_normaliza_igual_que_el_alta() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let seccion = crear_seccion(&mut tx, "e12").await;
    let estudiante = crear(&mut tx, datos_de_prueba(seccion.id, "García", "Ana").await).await?;

    // RF-10: el mismo `preparar_entrada` del alta; `"  ... "` se colapsa.
    let datos = preparar_entrada(seccion.id, " Rojas   Martínez ", "María  Luisa ", None)?;
    let editado = editar(&mut tx, estudiante.id, datos).await?;

    assert_eq!(
        editado.last_name, "Rojas Martínez",
        "RF-10: apellido colapsado"
    );
    assert_eq!(editado.first_name, "María Luisa", "RF-10: nombre colapsado");
    assert_eq!(editado.id_section, seccion.id);

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn el_editar_mueve_a_otra_seccion_activa_y_la_valida() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let origen = crear_seccion(&mut tx, "e15").await;
    let destino = crear_seccion(&mut tx, "e16").await;
    let estudiante = crear(&mut tx, datos_de_prueba(origen.id, "García", "Ana").await).await?;

    // RF-12: mover a una sección existente y activa funciona, y el padrón del
    // origen queda sin el estudiante (el mismo lista en el destino).
    let movido = editar(
        &mut tx,
        estudiante.id,
        datos_de_prueba(destino.id, "García", "Ana").await,
    )
    .await?;

    assert_eq!(
        movido.id_section, destino.id,
        "RF-12: el cambio de sección aplica"
    );

    // RF-13 (primer término): sin asistencia registrada, el cambio se permite.
    let en_origen = listar_por_seccion(&mut tx, origen.id, false).await?;
    assert!(en_origen.is_empty(), "el origen queda sin el estudiante");
    assert_eq!(
        listar_por_seccion(&mut tx, destino.id, false).await?.len(),
        1
    );

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn el_editar_a_seccion_inexistente_informa_que_no_existe() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let seccion = crear_seccion(&mut tx, "e17").await;
    let estudiante = crear(&mut tx, datos_de_prueba(seccion.id, "García", "Ana").await).await?;

    let resultado = editar(
        &mut tx,
        estudiante.id,
        datos_de_prueba(Uuid::new_v4(), "García", "Ana").await,
    )
    .await;

    assert!(
        matches!(resultado, Err(ErrorEstudiante::SeccionNoExiste)),
        "mover a una sección inexistente → SeccionNoExiste (RF-12), llegó: {resultado:?}"
    );

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn el_editar_a_seccion_inactiva_informa_que_esta_dada_de_baja() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let origen = crear_seccion(&mut tx, "e18").await;
    let destino = crear_seccion(&mut tx, "e19").await;
    secciones::set_active(&mut tx, destino.id, false).await?;
    let estudiante = crear(&mut tx, datos_de_prueba(origen.id, "García", "Ana").await).await?;

    let resultado = editar(
        &mut tx,
        estudiante.id,
        datos_de_prueba(destino.id, "García", "Ana").await,
    )
    .await;

    assert!(
        matches!(resultado, Err(ErrorEstudiante::SeccionInactiva)),
        "mover a una sección inactiva → SeccionInactiva (RF-12), llegó: {resultado:?}"
    );

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn el_editar_sin_cambiar_de_seccion_no_vuelve_a_validarla() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let seccion = crear_seccion(&mut tx, "e20").await;
    let estudiante = crear(&mut tx, datos_de_prueba(seccion.id, "García", "Ana").await).await?;
    secciones::set_active(&mut tx, seccion.id, false).await?;

    // Literal de RF-12: "CUANDO la edición CAMBIA la sección". Corregir el
    // teléfono de un estudiante de una sección ya dada de baja no es cambiar
    // de sección: no hay que volver a exigir que la sección esté activa.
    let corregir = preparar_entrada(seccion.id, "García", "Ana", Some("04121"))?;
    let editado = editar(&mut tx, estudiante.id, corregir).await?;

    assert_eq!(editado.phone.as_deref(), Some("04121"));
    assert!(
        editado.active,
        "la edición no toca el estado del estudiante"
    );

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn el_editar_de_un_estudiante_inexistente_informa_que_no_se_encontro() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let seccion = crear_seccion(&mut tx, "e21").await;
    let resultado = editar(
        &mut tx,
        Uuid::new_v4(),
        datos_de_prueba(seccion.id, "García", "Ana").await,
    )
    .await;

    assert!(
        matches!(resultado, Err(ErrorEstudiante::EstudianteNoExiste)),
        "editar un id inexistente → EstudianteNoExiste, llegó: {resultado:?}"
    );

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn el_editar_deja_su_entrada_de_auditoria() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let seccion = crear_seccion(&mut tx, "e22").await;
    let estudiante = crear(&mut tx, datos_de_prueba(seccion.id, "García", "Ana").await).await?;

    let datos = preparar_entrada(seccion.id, "Rojas", "Carlos", None)?;
    editar(&mut tx, estudiante.id, datos).await?;

    let (action, data): (String, serde_json::Value) = sqlx::query_as(
        "SELECT action, data FROM audit_logs \
         WHERE entity = 'students' AND entity_id = $1 AND action = 'editar'",
    )
    .bind(estudiante.id)
    .fetch_one(&mut *tx)
    .await?;

    assert_eq!(action, "editar", "RF-18: la edición se registra en español");
    assert!(
        data["apellido"] == "Rojas",
        "la auditoría guarda el valor nuevo: {data}"
    );

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn los_nombres_en_blanco_se_rechazan_aunque_se_salte_el_dominio() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let seccion = crear_seccion(&mut tx, "e23").await;
    let estudiante = crear(&mut tx, datos_de_prueba(seccion.id, "García", "Ana").await).await?;

    // Bypass deliberado de `preparar_entrada`: un bug futuro del dominio no
    // debe poder guardar un apellido en blanco, porque la base también tiene
    // el CHECK `students_names_not_blank` (RF-11 en profundidad).
    let invalido = DatosEstudiante {
        id_seccion: seccion.id,
        apellido: "   ".to_string(),
        nombre: "Ana".to_string(),
        telefono: None,
    };
    let resultado = editar(&mut tx, estudiante.id, invalido).await;

    let Err(ErrorEstudiante::Sqlx(error)) = resultado else {
        panic!("un nombre en blanco debe fallar en la base (RF-11), llegó: {resultado:?}");
    };
    if let sqlx::Error::Database(base) = &error {
        assert_eq!(base.code().as_deref(), Some("23514"));
        assert_eq!(base.constraint(), Some("students_names_not_blank"));
    } else {
        panic!("esperaba un error de base de datos, llegó: {error}");
    }

    tx.rollback().await?;
    Ok(())
}
