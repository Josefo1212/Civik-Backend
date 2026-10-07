-- Una sección por su identificador, activa o dada de baja (RF-12). La lectura
-- se hace siempre que se quiere el estado completo de una fila: es la vía que
-- usa `queries::set_active` para separar "no existe" (RowNotFound) de "ya
-- estaba así" (la fila, sin cambios). El cast de `shift` cumple el mismo papel
-- que en set_active.sql.
SELECT id, grade_level, section, shift::text AS shift, active, created_at, updated_at
FROM sections
WHERE id = $1;