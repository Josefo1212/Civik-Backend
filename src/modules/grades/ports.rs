//! Puerto del módulo `grades` (hexagonal completo, RNF-2).
//!
//! El trait `GradeRepository` es lo que permite probar la orquestación del
//! service con un repositorio **en memoria** (RNF-6), sin Postgres. El
//! adaptador real (`adapter_pg.rs`) lo implementa con `sqlx`; el dominio puro
//! (`domain.rs`) nunca ve ninguna de estas firmas.

use async_trait::async_trait;
use rust_decimal::Decimal;
use uuid::Uuid;

/// Los ausentes de un estudiante en los eventos elegibles de un lapso, ya
/// agregados por tipo (RNF-4). No es una fila de asistencia: es el conteo que
/// el dominio necesita para la fórmula.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AusentesEstudiante {
    pub id_student: Uuid,
    pub ausentes_regular: i64,
    pub ausentes_extra: i64,
    pub ausentes_presentacion: i64,
}

/// Un estudiante del roster de una sección en un lapso, con su override si lo
/// tiene (`None` si no) y su identidad para pintarlo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlumnoSeccion {
    pub id_student: Uuid,
    pub first_name: String,
    pub last_name: String,
    pub override_value: Option<Decimal>,
}

/// Las consultas que `grades` necesita de la infraestructura.
#[async_trait]
pub trait GradeRepository: Send + Sync {
    /// ¿Existe el lapso? `404` si no (RF-10).
    async fn existe_lapso(&self, id_lapse: Uuid) -> Result<bool, sqlx::Error>;

    /// ¿Existe la sección? `404` si no (RF-11).
    async fn existe_seccion(&self, id_section: Uuid) -> Result<bool, sqlx::Error>;

    /// Cuántos eventos elegibles tiene el lapso (N, a nivel de lapso, RF-3 y
    /// RF-4).
    async fn n_eventos_elegibles(&self, id_lapse: Uuid) -> Result<i64, sqlx::Error>;

    /// Los ausentes por estudiante y por tipo, solo de eventos elegibles y solo
    /// `AUSENTE` (RNF-4).
    async fn ausentes_por_estudiante(
        &self,
        id_lapse: Uuid,
    ) -> Result<Vec<AusentesEstudiante>, sqlx::Error>;

    /// El roster de la sección en el lapso (activos ∪ con asistencia), con su
    /// override y en el orden canónico (RF-9).
    async fn roster(
        &self,
        id_lapse: Uuid,
        id_section: Uuid,
    ) -> Result<Vec<AlumnoSeccion>, sqlx::Error>;
}
