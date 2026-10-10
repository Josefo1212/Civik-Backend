UPDATE attendance
SET state = $3::text::attendance_state, updated_at = NOW()
WHERE id_event = $1 AND id_student = $2
RETURNING id_event, id_student, state::text AS state, created_at, updated_at
