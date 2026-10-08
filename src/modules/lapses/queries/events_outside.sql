-- Pre-chequeo del service para RF-11: cuántos eventos quedarían fuera del
-- lapso si se movieran sus fechas a [inicio, fin]. Es la copia legible de la
-- condición de `check_lapse_dates_against_events` (el trigger), para que el
-- service pueda rechazar con un 409 entendible antes de que la base lance el
-- 23514 de su defensa. El trigger sigue siendo la invariante: si el service
-- falla, la base no deja mover las fechas.
SELECT count(*) AS cuantos
FROM events
WHERE id_lapse = $1
  AND (event_date < $2 OR event_date > $3);