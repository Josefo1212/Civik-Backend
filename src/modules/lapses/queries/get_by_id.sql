-- Un lapso por su identificador, en cualquier estado (RF-6, RF-7). La lectura
-- es la vía que usa el service para separar "no existe" (RowNotFound) de
-- "está en un estado que no permite la operación" (la fila, con su estado
-- real), y la que conserva la fila que `eliminar` va a auditar antes del
-- borrado. El cast de practice_day/status es el mismo de create.sql.
SELECT id, school_year, lapse_number, start_date, end_date,
       practice_day::text AS practice_day, status::text AS status,
       created_at, updated_at
FROM lapses
WHERE id = $1;