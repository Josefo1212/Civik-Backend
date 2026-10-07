-- get_by_id.sql
-- Un estudiante por su identificador, activo o inactivo (RF-8). Si no
-- existe, `fetch_optional` devuelve `None` y `queries::consultar` responde
-- "no se encontró" (RF-9).
SELECT id, id_section, first_name, last_name, phone, active, created_at,
       updated_at
FROM students
WHERE id = $1;