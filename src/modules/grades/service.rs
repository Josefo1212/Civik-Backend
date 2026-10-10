//! Orquestación de `grades` (hexagonal completo): junta el roster, los conteos
//! agregados y N, y resuelve la nota de cada estudiante con el dominio puro.
//! No persiste nada: la nota se recalcula en cada consulta (RF-13).

use std::collections::HashMap;

use uuid::Uuid;

use crate::shared::error::AppError;

use super::domain::{resolver_nota, Conteos, Nota};
use super::ports::{AusentesEstudiante, GradeRepository};

/// Una fila del listado de notas: la identidad del estudiante y su nota.
pub struct FilaNota {
    pub id_student: Uuid,
    pub first_name: String,
    pub last_name: String,
    pub nota: Nota,
}

pub struct GradeService<R: GradeRepository> {
    repo: R,
}

impl<R: GradeRepository> GradeService<R> {
    pub fn new(repo: R) -> Self {
        Self { repo }
    }

    /// Las notas de los estudiantes de una sección en un lapso (RF-5, RF-6,
    /// RF-9, RF-12, RF-13): `404` si el lapso o la sección no existen,
    /// `SinNota` (→ `null`) cuando no hay eventos elegibles y no hay override.
    ///
    /// Cada estudiante se resuelve de forma independiente (RF-12): la ausencia
    /// de datos de uno no toca la nota de otro. El override se resuelve en el
    /// dominio (`resolver_nota`), no en el SQL.
    pub async fn notas_de_seccion(
        &self,
        id_lapse: Uuid,
        id_section: Uuid,
    ) -> Result<Vec<FilaNota>, AppError> {
        if !self.repo.existe_lapso(id_lapse).await? {
            return Err(AppError::NotFound("No se encontró el lapso".into()));
        }
        if !self.repo.existe_seccion(id_section).await? {
            return Err(AppError::NotFound("No se encontró la sección".into()));
        }

        let n_eventos_elegibles = self.repo.n_eventos_elegibles(id_lapse).await?;
        let ausentes = self.repo.ausentes_por_estudiante(id_lapse).await?;
        let roster = self.repo.roster(id_lapse, id_section).await?;

        let por_estudiante: HashMap<Uuid, AusentesEstudiante> =
            ausentes.into_iter().map(|a| (a.id_student, a)).collect();

        let filas = roster
            .into_iter()
            .map(|alumno| {
                let conteos = match por_estudiante.get(&alumno.id_student) {
                    Some(a) => Conteos {
                        ausentes_regular: a.ausentes_regular,
                        ausentes_extra: a.ausentes_extra,
                        ausentes_presentacion: a.ausentes_presentacion,
                        n_eventos_elegibles,
                    },
                    None => Conteos {
                        ausentes_regular: 0,
                        ausentes_extra: 0,
                        ausentes_presentacion: 0,
                        n_eventos_elegibles,
                    },
                };

                FilaNota {
                    id_student: alumno.id_student,
                    first_name: alumno.first_name,
                    last_name: alumno.last_name,
                    nota: resolver_nota(&conteos, alumno.override_value),
                }
            })
            .collect();

        Ok(filas)
    }
}

/// El valor de la nota en la forma que espera el JSON (RF-8): `None` cuando no
/// hay nota (→ `null`); el cálculo sale con un decimal y el override con sus
/// dos decimales exactos. Se expone acá para que el handler no decida el
/// formato.
pub fn formatear_nota(nota: &Nota) -> Option<String> {
    match nota {
        Nota::SinNota => None,
        Nota::Valor(valor) => Some(valor.to_string()),
    }
}
