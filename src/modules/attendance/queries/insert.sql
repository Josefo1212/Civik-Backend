INSERT INTO attendance (id_event, id_student, state)
VALUES ($1, $2, $3::text::attendance_state)
RETURNING id_event, id_student, state::text AS state, created_at, updated_at
