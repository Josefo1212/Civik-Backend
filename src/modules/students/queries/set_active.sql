-- set_active.sql — RF-14 a RF-17
-- Marca al estudiante activo o inactivo. El `active IS DISTINCT FROM $2` hace
-- idempotente la llamada: si ya estaba en el estado pedido no devuelve filas,
-- no mueve `updated_at` y no genera auditoría falsa (RF-15, RF-17).
UPDATE students
SET active = $2
WHERE id = $1
  AND active IS DISTINCT FROM $2
RETURNING id, id_section, first_name, last_name, phone, active,
          created_at, updated_at;