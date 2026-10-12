-- list_all.sql
-- Listado global de estudiantes (RF-1, RF-2): todas las secciones juntas, con
-- la misma forma y el mismo orden canónico que `list_by_section.sql`. Por
-- defecto solo los activos; la bandera `$1` incluye también los inactivos,
-- marcando su estado.
SELECT id, id_section, first_name, last_name, phone, active, created_at,
       updated_at
FROM students
WHERE ($1::boolean OR active)
ORDER BY lower(last_name), lower(first_name), id;
