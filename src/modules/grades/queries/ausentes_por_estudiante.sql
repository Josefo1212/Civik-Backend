-- Ausentes por estudiante y por tipo, ya agregados (RNF-4). Solo cuentan las
-- filas en estado AUSENTE de eventos elegibles (SCHEDULED y, mientras el lapso
-- no esté CLOSED, con fecha ya pasada). JUSTIFICADO y PRESENTE no aparecen.
-- Un estudiante sin filas no sale: su conteo de ausencias es 0 en todos los tipos.
SELECT a.id_student,
       COUNT(*) FILTER (WHERE e.type_event = 'PRACTICA_REGULAR')::bigint AS ausentes_regular,
       COUNT(*) FILTER (WHERE e.type_event = 'PRACTICA_EXTRA')::bigint AS ausentes_extra,
       COUNT(*) FILTER (WHERE e.type_event = 'PRESENTACION')::bigint AS ausentes_presentacion
FROM attendance a
JOIN events e ON e.id = a.id_event
JOIN lapses l ON l.id = e.id_lapse
WHERE e.id_lapse = $1
  AND e.status = 'SCHEDULED'
  AND a.state = 'AUSENTE'
  AND (l.status = 'CLOSED' OR e.event_date <= CURRENT_DATE)
GROUP BY a.id_student
