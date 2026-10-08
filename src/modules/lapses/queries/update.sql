-- Edición estructural de un lapso (RF-8, RF-10), con las mismas reglas que
-- create.sql: `$6` viaja como text y Postgres lo castea a week_day. Un lapso
-- CLOSED nunca llega acá con el flujo sano (el service valida la transición
-- con get_by_id + domain), y aunque llegara, `trg_lapses_closed_frozen`
-- rechaza el UPDATE con 23514: esa defensa final se prueba en conflictos_bd.
-- El WHERE sin guarda de estado es a propósito: "no modificar un CLOSED" es
-- una invariante de la base, no un filtro de filas.
--
-- El 23505 sobre uq_lapses_year_number y el 23P01 sobre lapses_no_overlap
-- suben tal cual y AppError::from los traduce a 409, igual que en el alta.
UPDATE lapses
SET school_year = $2, lapse_number = $3, start_date = $4, end_date = $5,
    practice_day = $6::text::week_day
WHERE id = $1
RETURNING id, school_year, lapse_number, start_date, end_date,
          practice_day::text AS practice_day, status::text AS status,
          created_at, updated_at;