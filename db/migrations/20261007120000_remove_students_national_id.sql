-- students: se retira la columna `national_id` y su restricción UNIQUE.
--
-- La cédula queda fuera del sistema por decisión de la spec 003 (HU-1 y
-- definiciones: "no se guardará cédula"). El campo llegó a la init como un
-- dato que nunca se pidió; esta migración lo suelta.
--
-- Regla absoluta de este repo: nunca se edita una migración aplicada. El
-- cambio es una migración nueva, y la init queda intacta para quien recreen
-- desde cero (la secuencia completa produce el mismo esquema final).

-- migrate:up
ALTER TABLE students DROP CONSTRAINT uq_students_national_id;
ALTER TABLE students DROP COLUMN national_id;

-- migrate:down
ALTER TABLE students ADD COLUMN national_id varchar(20);
ALTER TABLE students ADD CONSTRAINT uq_students_national_id UNIQUE (national_id);