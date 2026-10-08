-- Transición de estado de un lapso (RF-12 a RF-16): activación y cierre.
-- `$2` es el estado nuevo y `$3` el esperado; el `status = $3` en el WHERE es
-- un assert sobre la fila que el service ya validó con `domain::validar_transicion`.
-- Si no devuelve filas hay dos posibilidades que la base no separa, y el
-- llamador (`cambiar_estado`) las distingue con get_by_id: o el lapso no
-- existe (RowNotFound) o ya estaba en un estado que no permite la operación
-- (se devuelve tal cual, sin estado nuevo y sin auditoría).
UPDATE lapses
SET status = $2::text::lapse_status
WHERE id = $1 AND status = $3::text::lapse_status
RETURNING id, school_year, lapse_number, start_date, end_date,
          practice_day::text AS practice_day, status::text AS status,
          created_at, updated_at;