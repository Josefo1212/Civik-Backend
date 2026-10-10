-- Roster de una sección en un lapso (RF-9): los estudiantes activos de la
-- sección más los que tengan asistencia en el lapso aunque estén de baja
-- (decisión de la clarificación), con su override si lo tienen, en el orden
-- canónico de estudiantes.
--
-- El override se trae como subconsulta escalar y no con `LEFT JOIN`: la
-- subconsulta que no encuentra fila se infiere nullable, mientras que una
-- columna `NOT NULL` del lado nulo de un `LEFT JOIN` la infiere no nula (sqlx
-- no analiza la direccionalidad del join) y rompería al decodificar el `NULL`.
SELECT s.id AS id_student,
       s.first_name,
       s.last_name,
       (SELECT g.value
        FROM grade_overrides g
        WHERE g.id_student = s.id AND g.id_lapse = $1) AS override_value
FROM students s
WHERE s.id_section = $2
  AND (
    s.active
    OR EXISTS (
      SELECT 1
      FROM attendance a
      JOIN events e ON e.id = a.id_event
      WHERE a.id_student = s.id AND e.id_lapse = $1
    )
  )
ORDER BY lower(s.last_name), lower(s.first_name), s.id
