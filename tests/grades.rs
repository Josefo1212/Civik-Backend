//! Tests de `grades`. Tres capas (RNF-1, RNF-6, RNF-8):
//!
//! 1. Dominio puro (`compute_grade`, `resolver_nota`) — sin base.
//! 2. Service con un `GradeRepository` **falso** en memoria — sin Postgres.
//! 3. Integración con las consultas reales dentro de una transacción
//!    descartable (patrón transacción/rollback): elegibilidad por fecha/lapso,
//!    override y orden del roster.

mod common;

use async_trait::async_trait;
use chrono::NaiveDate;
use civik_backend::modules::attendance::domain::{EventId, StudentId};
use civik_backend::modules::attendance::queries as att_queries;
use civik_backend::modules::events::queries as events_queries;
use civik_backend::modules::grades::domain::{compute_grade, resolver_nota, Conteos, Nota};
use civik_backend::modules::grades::ports::{AlumnoSeccion, AusentesEstudiante, GradeRepository};
use civik_backend::modules::grades::queries as grades_queries;
use civik_backend::modules::grades::service::{formatear_nota, GradeService};
use civik_backend::modules::lapses::domain::preparar_entrada;
use civik_backend::modules::lapses::queries as lapses_queries;
use civik_backend::modules::sections::domain::{DatosSeccion, Grado, Turno};
use civik_backend::modules::sections::queries as sections_queries;
use civik_backend::modules::students::domain::DatosEstudiante;
use civik_backend::modules::students::queries as students_queries;
use civik_backend::shared::error::AppError;
use rust_decimal::Decimal;
use sqlx::PgTransaction;
use uuid::Uuid;

type Resultado = Result<(), Box<dyn std::error::Error>>;

fn fecha(anio: i32, mes: u32, dia: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(anio, mes, dia).expect("fecha válida")
}

// ---------------------------------------------------------------------------
// 1. Dominio puro
// ---------------------------------------------------------------------------

fn conteos(regular: i64, extra: i64, presentacion: i64, n: i64) -> Conteos {
    Conteos {
        ausentes_regular: regular,
        ausentes_extra: extra,
        ausentes_presentacion: presentacion,
        n_eventos_elegibles: n,
    }
}

#[test]
fn el_dominio_publico_calcula_y_resuelve() {
    assert_eq!(
        compute_grade(&conteos(1, 1, 1, 3)),
        Nota::Valor(Decimal::new(165, 1))
    );
    assert_eq!(compute_grade(&conteos(0, 0, 0, 0)), Nota::SinNota);
    assert_eq!(
        resolver_nota(&conteos(1, 0, 0, 1), Some(Decimal::new(1875, 2))),
        Nota::Valor(Decimal::new(1875, 2))
    );
}

// ---------------------------------------------------------------------------
// 2. Service con repositorio falso (RNF-6)
// ---------------------------------------------------------------------------

struct RepoFalso {
    lapso_existe: bool,
    seccion_existe: bool,
    n: i64,
    ausentes: Vec<AusentesEstudiante>,
    roster: Vec<AlumnoSeccion>,
}

#[async_trait]
impl GradeRepository for RepoFalso {
    async fn existe_lapso(&self, _id_lapse: Uuid) -> Result<bool, sqlx::Error> {
        Ok(self.lapso_existe)
    }

    async fn existe_seccion(&self, _id_section: Uuid) -> Result<bool, sqlx::Error> {
        Ok(self.seccion_existe)
    }

    async fn n_eventos_elegibles(&self, _id_lapse: Uuid) -> Result<i64, sqlx::Error> {
        Ok(self.n)
    }

    async fn ausentes_por_estudiante(
        &self,
        _id_lapse: Uuid,
    ) -> Result<Vec<AusentesEstudiante>, sqlx::Error> {
        Ok(self.ausentes.clone())
    }

    async fn roster(
        &self,
        _id_lapse: Uuid,
        _id_section: Uuid,
    ) -> Result<Vec<AlumnoSeccion>, sqlx::Error> {
        Ok(self.roster.clone())
    }
}

fn alumno(
    id: Uuid,
    nombre: &str,
    apellido: &str,
    override_value: Option<Decimal>,
) -> AlumnoSeccion {
    AlumnoSeccion {
        id_student: id,
        first_name: nombre.into(),
        last_name: apellido.into(),
        override_value,
    }
}

#[tokio::test]
async fn el_service_arma_sin_nota_y_respeta_el_orden() -> Resultado {
    let yo = Uuid::new_v4();
    let repo = RepoFalso {
        lapso_existe: true,
        seccion_existe: true,
        n: 0,
        ausentes: vec![],
        roster: vec![alumno(yo, "Ana", "Alfa", None)],
    };
    let servicio = GradeService::new(repo);

    let filas = servicio
        .notas_de_seccion(Uuid::new_v4(), Uuid::new_v4())
        .await?;

    assert_eq!(filas.len(), 1);
    assert_eq!(filas[0].id_student, yo);
    assert_eq!(filas[0].nota, Nota::SinNota);
    assert_eq!(formatear_nota(&filas[0].nota), None);
    Ok(())
}

#[tokio::test]
async fn el_service_deja_que_el_override_gane() -> Resultado {
    let yo = Uuid::new_v4();
    let repo = RepoFalso {
        lapso_existe: true,
        seccion_existe: true,
        n: 3,
        ausentes: vec![AusentesEstudiante {
            id_student: yo,
            ausentes_regular: 5,
            ausentes_extra: 0,
            ausentes_presentacion: 0,
        }],
        roster: vec![alumno(yo, "Ana", "Alfa", Some(Decimal::new(1875, 2)))],
    };
    let servicio = GradeService::new(repo);

    let filas = servicio
        .notas_de_seccion(Uuid::new_v4(), Uuid::new_v4())
        .await?;

    assert_eq!(filas[0].nota, Nota::Valor(Decimal::new(1875, 2)));
    assert_eq!(formatear_nota(&filas[0].nota).as_deref(), Some("18.75"));
    Ok(())
}

#[tokio::test]
async fn el_service_responde_404_si_falta_el_lapso_o_la_seccion() -> Resultado {
    let servicio = GradeService::new(RepoFalso {
        lapso_existe: false,
        seccion_existe: true,
        n: 1,
        ausentes: vec![],
        roster: vec![],
    });
    let error = servicio
        .notas_de_seccion(Uuid::new_v4(), Uuid::new_v4())
        .await
        .err()
        .expect("lapso inexistente debe fallar");
    assert!(matches!(error, AppError::NotFound(_)));

    let servicio = GradeService::new(RepoFalso {
        lapso_existe: true,
        seccion_existe: false,
        n: 1,
        ausentes: vec![],
        roster: vec![],
    });
    let error = servicio
        .notas_de_seccion(Uuid::new_v4(), Uuid::new_v4())
        .await
        .err()
        .expect("sección inexistente debe fallar");
    assert!(matches!(error, AppError::NotFound(_)));
    Ok(())
}

// ---------------------------------------------------------------------------
// 3. Integración (transacción/rollback) con las consultas reales
// ---------------------------------------------------------------------------

/// Crea un lapso dentro de la transacción y lo deja en el estado pedido.
async fn crear_lapso(
    tx: &mut PgTransaction<'_>,
    anio: i32,
    inicio: NaiveDate,
    fin: NaiveDate,
    activar: bool,
) -> Result<Uuid, Box<dyn std::error::Error>> {
    let lap = lapses_queries::crear(
        tx,
        preparar_entrada(
            &anio.to_string(),
            "1",
            &inicio.format("%Y-%m-%d").to_string(),
            &fin.format("%Y-%m-%d").to_string(),
            "THURSDAY",
        )?,
    )
    .await?;
    if activar {
        lapses_queries::activar(tx, lap.id).await?;
    }
    Ok(lap.id)
}

async fn crear_seccion(
    tx: &mut PgTransaction<'_>,
    codigo: &str,
) -> Result<Uuid, Box<dyn std::error::Error>> {
    let sec = sections_queries::crear(
        tx,
        DatosSeccion {
            grado: Grado::Primero,
            seccion: codigo.into(),
            turno: Turno::Matutino,
        },
    )
    .await?;
    Ok(sec.id)
}

async fn crear_estudiante(
    tx: &mut PgTransaction<'_>,
    id_seccion: Uuid,
    nombre: &str,
    apellido: &str,
) -> Result<Uuid, Box<dyn std::error::Error>> {
    let est = students_queries::crear(
        tx,
        DatosEstudiante {
            id_seccion,
            nombre: nombre.into(),
            apellido: apellido.into(),
            telefono: None,
        },
    )
    .await?;
    Ok(est.id)
}

/// Arma los conteos de un estudiante a partir de las consultas reales: N del
/// lapso más sus ausentes (o cero si no aparece). Es la unión que hace el
/// service, pero leyendo dentro de la transacción.
async fn conteos_de(
    tx: &mut PgTransaction<'_>,
    id_lapse: Uuid,
    id_student: Uuid,
) -> Result<Conteos, Box<dyn std::error::Error>> {
    let n = grades_queries::n_eventos_elegibles(tx.as_mut(), id_lapse).await?;
    let ausentes = grades_queries::ausentes_por_estudiante(tx.as_mut(), id_lapse).await?;
    let propio = ausentes.into_iter().find(|a| a.id_student == id_student);

    Ok(match propio {
        Some(a) => Conteos {
            ausentes_regular: a.ausentes_regular,
            ausentes_extra: a.ausentes_extra,
            ausentes_presentacion: a.ausentes_presentacion,
            n_eventos_elegibles: n,
        },
        None => Conteos {
            ausentes_regular: 0,
            ausentes_extra: 0,
            ausentes_presentacion: 0,
            n_eventos_elegibles: n,
        },
    })
}

#[tokio::test]
async fn lapso_abierto_con_evento_futuro_no_tiene_nota() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let lap = crear_lapso(&mut tx, 2061, fecha(2061, 1, 5), fecha(2061, 12, 16), true).await?;
    let sec = crear_seccion(&mut tx, "G61").await?;
    let est = crear_estudiante(&mut tx, sec, "Ana", "Alfa").await?;
    let ev = events_queries::crear_manual(
        tx.as_mut(),
        lap,
        fecha(2061, 6, 15),
        "PRACTICA_REGULAR",
        "Futura",
    )
    .await?;
    att_queries::upsert(tx.as_mut(), EventId(ev.id), StudentId(est), "AUSENTE").await?;

    // El lapso no está CLOSED y el evento es futuro: no es elegible.
    assert_eq!(
        grades_queries::n_eventos_elegibles(tx.as_mut(), lap).await?,
        0
    );
    let c = conteos_de(&mut tx, lap, est).await?;
    assert_eq!(resolver_nota(&c, None), Nota::SinNota);

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn lapso_cerrado_cuenta_el_evento_futuro() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let lap = crear_lapso(&mut tx, 2062, fecha(2062, 1, 5), fecha(2062, 12, 16), true).await?;
    let sec = crear_seccion(&mut tx, "G62").await?;
    let est = crear_estudiante(&mut tx, sec, "Ana", "Alfa").await?;
    let ev = events_queries::crear_manual(
        tx.as_mut(),
        lap,
        fecha(2062, 6, 15),
        "PRACTICA_REGULAR",
        "Futura",
    )
    .await?;
    // La asistencia se registra antes de cerrar (después el lapso no admite datos).
    att_queries::upsert(tx.as_mut(), EventId(ev.id), StudentId(est), "AUSENTE").await?;
    lapses_queries::cerrar(&mut tx, lap).await?;

    assert_eq!(
        grades_queries::n_eventos_elegibles(tx.as_mut(), lap).await?,
        1
    );
    let c = conteos_de(&mut tx, lap, est).await?;
    assert_eq!(resolver_nota(&c, None), Nota::Valor(Decimal::new(19, 0)));

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn lapso_abierto_con_evento_pasado_cuenta() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    // Año pasado: el evento ya ocurrió respecto de hoy.
    let lap = crear_lapso(&mut tx, 2001, fecha(2001, 1, 5), fecha(2001, 12, 16), true).await?;
    let sec = crear_seccion(&mut tx, "G01").await?;
    let est = crear_estudiante(&mut tx, sec, "Ana", "Alfa").await?;
    let ev = events_queries::crear_manual(
        tx.as_mut(),
        lap,
        fecha(2001, 6, 15),
        "PRACTICA_REGULAR",
        "Pasada",
    )
    .await?;
    att_queries::upsert(tx.as_mut(), EventId(ev.id), StudentId(est), "AUSENTE").await?;

    assert_eq!(
        grades_queries::n_eventos_elegibles(tx.as_mut(), lap).await?,
        1
    );
    let c = conteos_de(&mut tx, lap, est).await?;
    assert_eq!(resolver_nota(&c, None), Nota::Valor(Decimal::new(19, 0)));

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn el_override_prevalece_y_conserva_dos_decimales() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let lap = crear_lapso(&mut tx, 2063, fecha(2063, 1, 5), fecha(2063, 12, 16), true).await?;
    let sec = crear_seccion(&mut tx, "G63").await?;
    let est = crear_estudiante(&mut tx, sec, "Ana", "Alfa").await?;
    let ev = events_queries::crear_manual(
        tx.as_mut(),
        lap,
        fecha(2063, 6, 15),
        "PRACTICA_REGULAR",
        "Pasada",
    )
    .await?;
    att_queries::upsert(tx.as_mut(), EventId(ev.id), StudentId(est), "AUSENTE").await?;

    let override_value = Decimal::new(1875, 2); // 18.75
    sqlx::query("INSERT INTO grade_overrides (id_student, id_lapse, value) VALUES ($1, $2, $3)")
        .bind(est)
        .bind(lap)
        .bind(override_value)
        .execute(tx.as_mut())
        .await?;

    let roster = grades_queries::roster(tx.as_mut(), lap, sec).await?;
    let alumno = roster
        .iter()
        .find(|a| a.id_student == est)
        .expect("el estudiante está en el roster");
    assert_eq!(alumno.override_value, Some(override_value));

    // La fórmula daría 19, pero el override manda, con sus dos decimales.
    let c = conteos_de(&mut tx, lap, est).await?;
    let nota = resolver_nota(&c, alumno.override_value);
    assert_eq!(nota, Nota::Valor(override_value));
    assert_eq!(formatear_nota(&nota).as_deref(), Some("18.75"));

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn el_roster_incluye_de_baja_con_asistencia_y_respeta_el_orden() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let lap = crear_lapso(&mut tx, 2064, fecha(2064, 1, 5), fecha(2064, 12, 16), true).await?;
    let sec = crear_seccion(&mut tx, "G64").await?;
    let zeta = crear_estudiante(&mut tx, sec, "Beto", "Zeta").await?;
    let alfa = crear_estudiante(&mut tx, sec, "Ana", "Alfa").await?;
    let baja = crear_estudiante(&mut tx, sec, "Carla", "Baja").await?;

    let ev = events_queries::crear_manual(
        tx.as_mut(),
        lap,
        fecha(2064, 6, 15),
        "PRACTICA_REGULAR",
        "Pasada",
    )
    .await?;
    att_queries::upsert(tx.as_mut(), EventId(ev.id), StudentId(baja), "AUSENTE").await?;
    students_queries::set_active(&mut tx, baja, false).await?;

    let roster = grades_queries::roster(tx.as_mut(), lap, sec).await?;
    let pos = |id: Uuid| roster.iter().position(|a| a.id_student == id);

    assert!(pos(alfa) < pos(zeta), "orden canónico por apellido");
    assert!(
        pos(baja).is_some(),
        "un estudiante de baja con asistencia en el lapso sigue en el roster"
    );
    assert_eq!(roster[pos(alfa).expect("alfa")].override_value, None);

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn las_notas_son_independientes_por_estudiante() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let lap = crear_lapso(&mut tx, 2002, fecha(2002, 1, 5), fecha(2002, 12, 16), true).await?;
    let sec = crear_seccion(&mut tx, "G02").await?;
    let ausente = crear_estudiante(&mut tx, sec, "Ana", "Alfa").await?;
    let presente = crear_estudiante(&mut tx, sec, "Beto", "Zeta").await?;

    let ev = events_queries::crear_manual(
        tx.as_mut(),
        lap,
        fecha(2002, 6, 15),
        "PRACTICA_REGULAR",
        "Pasada",
    )
    .await?;
    att_queries::upsert(tx.as_mut(), EventId(ev.id), StudentId(ausente), "AUSENTE").await?;
    att_queries::upsert(tx.as_mut(), EventId(ev.id), StudentId(presente), "PRESENTE").await?;

    let c_ausente = conteos_de(&mut tx, lap, ausente).await?;
    let c_presente = conteos_de(&mut tx, lap, presente).await?;

    assert_eq!(
        resolver_nota(&c_ausente, None),
        Nota::Valor(Decimal::new(19, 0))
    );
    assert_eq!(
        resolver_nota(&c_presente, None),
        Nota::Valor(Decimal::new(20, 0))
    );

    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn la_nota_se_recalcula_al_corregir_la_asistencia() -> Resultado {
    let pool = common::pool().await?;
    let mut tx = pool.begin().await?;

    let lap = crear_lapso(&mut tx, 2003, fecha(2003, 1, 5), fecha(2003, 12, 16), true).await?;
    let sec = crear_seccion(&mut tx, "G03").await?;
    let est = crear_estudiante(&mut tx, sec, "Ana", "Alfa").await?;
    let ev = events_queries::crear_manual(
        tx.as_mut(),
        lap,
        fecha(2003, 6, 15),
        "PRACTICA_REGULAR",
        "Pasada",
    )
    .await?;

    att_queries::upsert(tx.as_mut(), EventId(ev.id), StudentId(est), "PRESENTE").await?;
    let c = conteos_de(&mut tx, lap, est).await?;
    assert_eq!(resolver_nota(&c, None), Nota::Valor(Decimal::new(20, 0)));

    att_queries::set_state(tx.as_mut(), EventId(ev.id), StudentId(est), "AUSENTE").await?;
    let c = conteos_de(&mut tx, lap, est).await?;
    assert_eq!(resolver_nota(&c, None), Nota::Valor(Decimal::new(19, 0)));

    tx.rollback().await?;
    Ok(())
}
