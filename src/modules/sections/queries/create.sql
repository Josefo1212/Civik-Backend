-- `$3` viaja como text y Postgres lo castea a shift_type: sqlx no mapea enums
-- propios en compilación, así que el catálogo lo garantiza el dominio (`Turno`)
-- y revalida el enum de Postgres sobre un literal inválido. El RETURNING
-- conserva solo columnas base (todas NOT NULL): las expresiones casteadas se
-- infieren como nulables y ensucian el mapeo a la fila.
INSERT INTO sections (grade_level, section, shift)
VALUES ($1, $2, $3::text::shift_type)
RETURNING id, grade_level, section, active, created_at, updated_at;