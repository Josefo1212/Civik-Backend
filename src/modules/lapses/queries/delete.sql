-- Borrado físico del lapso (RF-17, RF-18), la única excepción a "nada se
-- borra físicamente": la constitución la permite para el lapso vacío, no
-- cerrado y no terminado. El service ya validó esas condiciones con
-- `domain::validar_transicion` + `validar_borrado_permitido` (para dar el
-- 409 entendible); el RETURNING conserva la fila que desaparece y es con lo
-- que `eliminar` confirma que el borrado ocurrió.
--
-- Detrás quedan dos defensas de la base que se prueban en conflictos_bd: el
-- trigger `lapses_delete_guard` (no borra CLOSED ni terminado) y las FKs
-- RESTRICT de events/grade_overrides (23001, que AppError::from traduce a
-- Conflict).
DELETE FROM lapses
WHERE id = $1
RETURNING id, school_year, lapse_number, start_date, end_date,
          practice_day::text AS practice_day, status::text AS status,
          created_at, updated_at;