-- create.sql
-- Da de alta un estudiante en la sección `$1` y devuelve la fila completa
-- (RF-1). `national_id` y `phone` llegan como NULL desde Rust cuando el
-- instructor no los informó (RF-7).
INSERT INTO students (id_section, first_name, last_name, national_id, phone)
VALUES ($1, $2, $3, $4, $5)
RETURNING id, id_section, national_id, first_name, last_name, phone, active,
          created_at, updated_at;