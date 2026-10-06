-- sections: columna `active` y `CHECK` de no vacío.
--
-- Spec 002 (RF-18, RF-20, RF-24, RNF-1).
--
-- `active` no es un sustituto del DELETE: significa "sigue ofreciéndose", el
-- mismo sentido que ya tiene en `students`. No participa de la unicidad, y por
-- eso el DEFAULT true no abre ninguna excepción a "nada se borra físicamente"
-- (constitución 3): dar de baja conserva la fila y su historial.
--
-- El CHECK existe porque el backend normaliza antes de guardar (RF-2, RF-4),
-- pero una invariante que solo vive en Rust desaparece con el bug que la
-- rompe. `btrim` porque " " no es un dato: es el mismo criterio que ya usan
-- students_names_not_blank y events_name_not_blank.

-- migrate:up

ALTER TABLE sections
  ADD COLUMN active boolean NOT NULL DEFAULT true;

ALTER TABLE sections
  ADD CONSTRAINT sections_not_blank
  CHECK (btrim(grade_level) <> '' AND btrim(section) <> '');

-- migrate:down

-- La constraint primero: una columna no se puede quitar mientras una
-- constraint la use.
ALTER TABLE sections DROP CONSTRAINT sections_not_blank;
ALTER TABLE sections DROP COLUMN active;
