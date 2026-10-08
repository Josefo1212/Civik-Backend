-- Pre-chequeo del service para RF-18: ¿tiene el lapso algún dato asociado
-- (eventos u overrides) que lo haga no borrable? Es la copia legible de las
-- condiciones que `lapses_delete_guard` evalúa en plpgsql; el service la
-- consulta para responder el 409 entendible ANTES de que el guardarraíl lance
-- el 23514 cuyo mensaje la API no traduce.
--
-- El guardarraíl sigue siendo la invariante: si el service falla, la base no
-- deja borrar un lapso con datos. (La FK RESTRICT de events no se alcanza
-- nunca por DELETE: el guard chequea las mismas filas primero y aborta.)
SELECT EXISTS (SELECT 1 FROM events         WHERE id_lapse = $1)
    OR EXISTS (SELECT 1 FROM grade_overrides WHERE id_lapse = $1) AS tiene_datos;