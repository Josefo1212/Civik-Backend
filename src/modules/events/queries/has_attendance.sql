SELECT COUNT(*)::bigint AS cantidad
FROM attendance
WHERE id_event = $1
