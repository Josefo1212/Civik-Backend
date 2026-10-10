SELECT s.id AS id_student,
       s.first_name,
       s.last_name,
       a.state::text AS state
FROM students s
LEFT JOIN attendance a
  ON a.id_student = s.id AND a.id_event = $1
WHERE s.active OR a.id_event IS NOT NULL
ORDER BY lower(s.last_name), lower(s.first_name), s.id
