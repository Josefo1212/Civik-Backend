-- update.sql — RF-10 a RF-13
-- Edita los datos del estudiante y su sección. Cuando `phone` es NULL, la
-- columna queda NULL (el `""` ya llegó como None desde `preparar_entrada`).
UPDATE students
SET id_section = $2, first_name = $3, last_name = $4, phone = $5
WHERE id = $1
RETURNING id, id_section, first_name, last_name, phone, active,
          created_at, updated_at;