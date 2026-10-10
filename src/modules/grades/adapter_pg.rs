use async_trait::async_trait;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use super::domain::Conteos;
use super::ports::GradeRepository;

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
    async fn obtener_conteos(&self, id_lapse: Uuid, id_student: Uuid) -> Result<Conteos, sqlx::Error> {
        let row = sqlx::query(
            "SELECT
                COALESCE(SUM(CASE WHEN a.state='AUSENTE' AND e.type_event='PRACTICA_REGULAR' THEN 1 ELSE 0 END),0) as ausentes_regular,
                COALESCE(SUM(CASE WHEN a.state='AUSENTE' AND e.type_event='PRACTICA_EXTRA' THEN 1 ELSE 0 END),0) as ausentes_extra,
                COALESCE(SUM(CASE WHEN a.state='AUSENTE' AND e.type_event='PRESENTACION' THEN 1 ELSE 0 END),0) as ausentes_presentacion,
                COUNT(DISTINCT e.id) as total_eventos_elegibles
             FROM events e
             LEFT JOIN attendance a ON a.id_event = e.id AND a.id_student = $2
             WHERE e.id_lapse = $1 AND e.status='SCHEDULED'",
        )
        .bind(id_lapse)
        .bind(id_student)
        .fetch_one(&self.pool)
        .await?;
        Ok(Conteos {
            ausentes_regular: row.get::<i64, _>("ausentes_regular"),
            ausentes_extra: row.get::<i64, _>("ausentes_extra"),
            ausentes_presentacion: row.get::<i64, _>("ausentes_presentacion"),
            total_eventos_elegibles: row.get::<i64, _>("total_eventos_elegibles"),
        })
    }
}
