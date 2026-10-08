-- Listado completo de lapsos (RF-5), en el orden estable que promete RNF-3:
-- año escolar descendente (el vigente — el más nuevo — primero) y, dentro del
-- año, número de lapso ascendente (1er, 2do, 3er). Una columna compuesta
-- igual produciría exactamente el mismo orden entre llamadas; el desc+asc es
-- el que el frontend recorre para saber cuál es el período en curso.
SELECT id, school_year, lapse_number, start_date, end_date,
       practice_day::text AS practice_day, status::text AS status,
       created_at, updated_at
FROM lapses
ORDER BY school_year DESC, lapse_number ASC;