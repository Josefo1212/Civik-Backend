-- N (RF-3, RF-4): cuántos eventos elegibles tiene el lapso. Un evento es
-- elegible si está SCHEDULED y, mientras el lapso no esté CLOSED, si su fecha
-- ya pasó (event_date <= hoy). Con el lapso CLOSED cuentan todos los SCHEDULED,
-- sin filtrar por fecha. N es del lapso, no de un estudiante.
SELECT COUNT(*)::bigint AS total
FROM events e
JOIN lapses l ON l.id = e.id_lapse
WHERE e.id_lapse = $1
  AND e.status = 'SCHEDULED'
  AND (l.status = 'CLOSED' OR e.event_date <= CURRENT_DATE)
