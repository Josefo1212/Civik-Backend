use uuid::Uuid;

use super::domain::{compute_grade};
use super::ports::GradeRepository;

pub struct GradeService<R: GradeRepository> {
    repo: R,
}

impl<R: GradeRepository> GradeService<R> {
    pub fn new(repo: R) -> Self {
        Self { repo }
    }

    pub async fn calcular_nota(&self, id_lapse: Uuid, id_student: Uuid) -> Result<rust_decimal::Decimal, sqlx::Error> {
        let conteos = self.repo.obtener_conteos(id_lapse, id_student).await?;
        let res = compute_grade(conteos);
        Ok(res.nota)
    }
}
