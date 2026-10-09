INSERT INTO events (id_lapse, event_date, type_event, source, event_name, status)
VALUES ($1, $2, $3::text::event_type, 'MANUAL', $4, 'SCHEDULED')
RETURNING id, id_lapse, event_date, type_event::text AS type_event, source::text AS source, event_name, status::text AS status, cancelled_reason, created_at, updated_at
