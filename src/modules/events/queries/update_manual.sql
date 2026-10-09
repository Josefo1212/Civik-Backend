UPDATE events
SET event_date = $2, type_event = $3::text::event_type, event_name = $4, updated_at = now()
WHERE id = $1 AND source = 'MANUAL' AND status <> 'CANCELLED'
RETURNING id, id_lapse, event_date, type_event::text AS type_event, source::text AS source, event_name, status::text AS status, cancelled_reason, created_at, updated_at
