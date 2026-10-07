-- Edición de la identidad de una sección (RF-14 a RF-17), con las mismas
-- reglas que create.sql: `$4` viaja como text y Postgres lo castea a
-- shift_type (sqlx no mapea enums propios en compilación; el catálogo lo
-- garantiza `domain::Turno` y el enum revalida el literal). El RETURNING
-- conserva solo columnas base NOT NULL: las expresiones casteadas se infieren
-- como nulables y ensucian el mapeo a la fila; el turno lo rearma `editar`
-- desde los datos ya normalizados.
--
-- El UPDATE sin filas significa que la sección no existe (RowNotFound), y el
-- 23505 sobre uq_sections_grade_section_shift deja el intento de duplicado
-- para que `AppError::from` lo traduzca a 409 (RF-17). trg_sections_updated_at
-- renueva updated_at.
UPDATE sections
SET grade_level = $2, section = $3, shift = $4::text::shift_type
WHERE id = $1
RETURNING id, grade_level, section, active, created_at, updated_at;