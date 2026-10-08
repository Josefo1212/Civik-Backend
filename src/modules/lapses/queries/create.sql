-- Alta de un lapso (RF-1). `$5` viaja como text y Postgres lo castea a
-- week_day (sqlx no mapea enums propios en compilación; el catálogo lo
-- garantiza `domain::DiaSemana` y el enum revalida el literal). El RETURNING
-- lee practice_day y status con `::text AS`: las expresiones casteadas se
-- infieren como nulables en el macro y `FilaLapso` las normaliza
-- (ver queries.rs). status nace PLANNED por el DEFAULT de la columna.
INSERT INTO lapses (school_year, lapse_number, start_date, end_date, practice_day)
VALUES ($1, $2, $3, $4, $5::text::week_day)
RETURNING id, school_year, lapse_number, start_date, end_date,
          practice_day::text AS practice_day, status::text AS status,
          created_at, updated_at;