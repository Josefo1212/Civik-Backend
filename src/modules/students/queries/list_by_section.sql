-- list_by_section.sql
-- El padrón de una sección (RF-6 y RF-7). Por defecto solo los activos; la
-- bandera `$2` incluye también los inactivos, marcando su estado.
--
-- El orden es `lower(last_name), lower(first_name)`: los nombres se guardan
-- tal como los escribió el instructor (puede haber mayúsculas mezcladas) y el
-- padrón debe ordenarse igual en todos los dispositivos (RNF-3). El `, id`
-- final rompe empates de nombre + apellido idénticos de forma determinista.
SELECT id, id_section, first_name, last_name, phone, active, created_at,
       updated_at
FROM students
WHERE id_section = $1
  AND ($2::boolean OR active)
ORDER BY lower(last_name), lower(first_name), id;