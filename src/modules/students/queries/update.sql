-- update.sql — RF-12 a RF-16
-- Edita los datos del estudiante y su sección. Cuando `national_id` o `phone`
-- son NULL, la columna queda NULL (RF-7 también en la edición: "silenciar"
-- una cédula es posible mandando vacío desde `preparar_entrada`, que llega
-- hasta acá como None).
UPDATE students
SET id_section = $2, first_name = $3, last_name = $4,
    national_id = $5, phone = $6
WHERE id = $1
RETURNING id, id_section, national_id, first_name, last_name, phone,
          active, created_at, updated_at;