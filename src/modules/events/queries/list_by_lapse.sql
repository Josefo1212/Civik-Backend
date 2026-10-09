SELECT id, id_lapse, event_date, type_event::text AS type_event, source::text AS source, event_name, status::text AS status, cancelled_reason, created_at, updated_at
FROM events
WHERE id_lapse = $1
ORDER BY event_date ASC, created_at ASC
