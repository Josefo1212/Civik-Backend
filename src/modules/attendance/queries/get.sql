SELECT id_event, id_student, state::text AS state, created_at, updated_at
FROM attendance
WHERE id_event = $1 AND id_student = $2
