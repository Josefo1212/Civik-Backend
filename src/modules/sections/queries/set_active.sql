-- Baja y reactivación (RF-18 a RF-21). El `active IS DISTINCT FROM $2` hace la
-- operación realmente idempotente: si la sección ya está en el estado pedido,
-- la condición no la actualiza, no se dispara trg_sections_updated_at y
-- `updated_at` queda intacto — exactamente lo que RF-19 y RF-21 prometen con
-- "no modifica nada". El flujo que distingue "no existe" de "ya estaba así"
-- vive en `queries::set_active`, apoyado en `get_by_id.sql`.
--
-- A diferencia de create.sql/update.sql, este RETURNING sí trae `shift`, porque
-- acá Rust no conoce el turno: se lee con `shift::text AS shift` y se mapea a
-- `Option<String>` (los casts se infieren nulables en el macro; la columna es
-- NOT NULL, así que nunca llega NULL en práctica).
UPDATE sections
SET active = $2
WHERE id = $1 AND active IS DISTINCT FROM $2
RETURNING id, grade_level, section, shift::text AS shift, active, created_at, updated_at;