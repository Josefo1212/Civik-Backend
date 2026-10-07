-- set_active.sql — RF-17 a RF-20
-- Marca al estudiante activo o inactivo. El `active IS DISTINCT FROM $2` hace
-- idempotente la llamada: si ya estaba en el estado pedido no devuelve filas,
-- no mueve `updated_at` y no genera auditoría falsa (RF-18, RF-20).
UPDATE students
SET active = $2
WHERE id = $1
  AND active IS DISTINCT FROM $2
RETURNING id, id_section, national_id, first_name, last_name, phone,
          active, created_at, updated_at;