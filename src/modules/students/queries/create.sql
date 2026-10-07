-- create.sql
-- Da de alta un estudiante en la sección `$1` y devuelve la fila completa
-- (RF-1). `phone` llega como NULL desde Rust cuando el instructor no lo
-- informó.
INSERT INTO students (id_section, first_name, last_name, phone)
VALUES ($1, $2, $3, $4)
RETURNING id, id_section, first_name, last_name, phone, active,
          created_at, updated_at;