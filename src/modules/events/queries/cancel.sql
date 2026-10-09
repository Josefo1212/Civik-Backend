UPDATE events
SET status = 'CANCELLED', cancelled_reason = $2, updated_at = now()
WHERE id = $1 AND status = 'SCHEDULED'
RETURNING id, id_lapse, event_date, type_event::text AS type_event, source::text AS source, event_name, status::text AS status, cancelled_reason, created_at, updated_at
