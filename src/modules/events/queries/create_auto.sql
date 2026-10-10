INSERT INTO events (id_lapse, event_date, type_event, source, event_name, status)
VALUES ($1, $2, 'PRACTICA_REGULAR', 'AUTO', $3, 'SCHEDULED')
RETURNING id, id_lapse, event_date, type_event::text AS type_event, source::text AS source, event_name, status::text AS status, cancelled_reason, created_at, updated_at