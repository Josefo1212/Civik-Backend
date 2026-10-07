-- Listado de secciones activas (RF-9, RF-10, RF-11, RF-24).
--
-- `$1` es el filtro opcional por grado: un `NULL` lo desactiva y el parámetro
-- se castea a `text` porque llega como texto desde Rust. La sección inactiva
-- se descarta acá, en la query, para que el handler no tenga que reprocesar
-- (RNF-1: el filtrado de negocio vive en el servidor, no en el cliente).
--
-- El `ORDER BY` usa `array_position($2::text[], grade_level)`: el array llega
-- construido en Rust desde `Grado::TODOS` (`["1ro","2do",...,"5to"]`), de modo
-- que el orden canónico tiene UNA sola fuente de verdad — el enum — y no se
-- duplica en SQL. `COALESCE(..., 99)` hace que un grado fuera del catálogo
-- nunca rompa el orden estable que promete RNF-3: cae al final, en vez de
-- que `NULL < 5`. El cast de `shift` es el mismo de set_active.sql.
SELECT id, grade_level, section, shift::text AS shift, active, created_at, updated_at
FROM sections
WHERE active
  AND ($1::text IS NULL OR grade_level = $1)
ORDER BY COALESCE(array_position($2::text[], grade_level), 99), section;