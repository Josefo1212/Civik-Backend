use async_trait::async_trait;
use uuid::Uuid;

use super::domain::Conteos;

#[async_trait]
pub trait GradeRepository {
    async fn obtener_conteos(
        &self,
        id_lapse: Uuid,
        id_student: Uuid,
    ) -> Result<Conteos, sqlx::Error>;
}
