-- Eliminación física limitada a secciones sin estudiantes (RF-22, RF-23). No
-- lleva filtro por `active` a propósito: una sección dada de baja que nunca
-- tuvo estudiantes también se elimina (caso límite de la spec). Lo único que
-- bloquea es la FK `students_id_section_fkey` (23503), y `AppError::from` lo
-- traduce a `Conflict` ("la sección tiene estudiantes").
--
-- El RETURNING con `shift::text AS shift` conserva la fila que desaparece: es
-- contra lo que `eliminar` verifica que el borrado sí ocurrió (el enum no se
-- mapea en sqlx, por eso el cast; ver `set_active.sql`).
DELETE FROM sections
WHERE id = $1
RETURNING id, grade_level, section, shift::text AS shift, active, created_at, updated_at;