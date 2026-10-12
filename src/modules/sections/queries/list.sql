-- Listado de secciones (RF-4, RF-5, RF-10, RF-24 y RF-9 a RF-11 de 002).
--
-- `$1` es el filtro opcional por grado: un `NULL` lo desactiva y el parámetro
-- se castea a `text` porque llega como texto desde Rust. `$3` es la bandera
-- `incluir_inactivas`: por defecto (`false`) descarta las secciones dadas de
-- baja; con `true` las incluye, marcadas con su `active`. El filtrado vive en
-- la query (RNF-1: el negocio se resuelve en el servidor, no en el cliente).
--
-- El `ORDER BY` usa `array_position($2::text[], grade_level)`: el array llega
-- construido en Rust desde `Grado::TODOS` (`["1ro","2do",...,"5to"]`), de modo
-- que el orden canónico tiene UNA sola fuente de verdad — el enum — y no se
-- duplica en SQL. `COALESCE(..., 99)` hace que un grado fuera del catálogo
-- nunca rompa el orden estable que promete RNF-3: cae al final, en vez de
-- que `NULL < 5`. El cast de `shift` es el mismo de set_active.sql.
SELECT id, grade_level, section, shift::text AS shift, active, created_at, updated_at
FROM sections
WHERE ($3::boolean OR active)
  AND ($1::text IS NULL OR grade_level = $1)
ORDER BY COALESCE(array_position($2::text[], grade_level), 99), section;
