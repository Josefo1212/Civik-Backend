//! Adaptador `sqlx` del puerto `GradeRepository` (RNF-2): implementa cada
//! método delegando en `queries.rs`, que es donde viven las consultas `.sql`.
//! El adaptador solo aporta el `PgPool`; toda la lógica de SQL y su mapeo está
//! en un único lugar reutilizable por los tests dentro de una transacción.

use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use super::ports::{AlumnoSeccion, AusentesEstudiante, GradeRepository};
use super::queries;

#[derive(Clone)]
pub struct PgGradeRepository {
    pool: PgPool,
}

impl PgGradeRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl GradeRepository for PgGradeRepository {
    async fn existe_lapso(&self, id_lapse: Uuid) -> Result<bool, sqlx::Error> {
        queries::existe_lapso(&self.pool, id_lapse).await
    }

    async fn existe_seccion(&self, id_section: Uuid) -> Result<bool, sqlx::Error> {
        queries::existe_seccion(&self.pool, id_section).await
    }

    async fn n_eventos_elegibles(&self, id_lapse: Uuid) -> Result<i64, sqlx::Error> {
        queries::n_eventos_elegibles(&self.pool, id_lapse).await
    }

    async fn ausentes_por_estudiante(
        &self,
        id_lapse: Uuid,
    ) -> Result<Vec<AusentesEstudiante>, sqlx::Error> {
        queries::ausentes_por_estudiante(&self.pool, id_lapse).await
    }

    async fn roster(
        &self,
        id_lapse: Uuid,
        id_section: Uuid,
    ) -> Result<Vec<AlumnoSeccion>, sqlx::Error> {
        queries::roster(&self.pool, id_lapse, id_section).await
    }
}
