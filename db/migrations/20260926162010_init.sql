-- Esquema de la base de datos de Civik.
--
-- Un solo archivo, aplicado en orden. Va de menos a mas: los tipos, las
-- tablas, despues las reglas que se apoyan en ellas. Un evento "ocurrio" si
-- tiene al menos una fila en attendance, no si su fecha ya paso: asi un
-- evento previsto que se suspendio nunca queda congelado.
--
-- Todo corre dentro de la transaccion de dbmate. No se usa CREATE INDEX
-- CONCURRENTLY porque no se puede ejecutar dentro de una transaccion, y con el
-- volumen objetivo (<=300 estudiantes) un CREATE INDEX normal es instantaneo.
--
-- CAMBIOS respecto a la version anterior de esta migracion (reinicio del
-- backend a Rust + hexagonal), hechos ACA porque esta migracion todavia no
-- estaba versionada en ningun entorno real:
--   1. Se elimina la tabla `notes` y el tipo `note_kind`. No correspondian a
--      ninguna decision de dominio acordada.
--   2. Se elimina `lapses.school_term`. Era 100% derivable de `lapse_number`
--      (1->'1er', 2->'2do', 3->'3er'); guardar los dos exigia un CHECK extra
--      solo para que no se desincronizaran. El texto se calcula en Rust,
--      igual que ya se decidio para el estado COMPLETED de `events`.
--   3. Se elimina la orquestacion del calendario automatico
--      (`generar_practicas_auto`, `reprogramar_practice_day` y su helper
--      `_generar_practicas_en_rango`). Esa logica es secuencia de negocio
--      ("cancelar estos, crear aquellos, con este motivo"), no una
--      invariante que deba sobrevivir a un bug del backend, asi que se
--      mueve al dominio de Rust (`events::service`), que logra la misma
--      atomicidad con una transaccion de sqlx. En la base quedan solo las
--      invariantes que protegen esas operaciones (regla 1, regla 9, el
--      indice `uq_events_auto_one_per_day`), no la orquestacion.

-- migrate:up

-- =========================================================================
-- ESQUEMA BASE
-- =========================================================================
CREATE TYPE shift_type AS ENUM ('MATUTINO', 'VESPERTINO');
CREATE TYPE week_day AS ENUM ('MONDAY', 'TUESDAY', 'WEDNESDAY', 'THURSDAY', 'FRIDAY', 'SATURDAY', 'SUNDAY');
CREATE TYPE lapse_status AS ENUM ('PLANNED', 'ACTIVE', 'CLOSED');
CREATE TYPE event_type AS ENUM ('PRACTICA_REGULAR', 'PRACTICA_EXTRA', 'PRESENTACION');
CREATE TYPE event_source AS ENUM ('AUTO', 'MANUAL');
CREATE TYPE event_status AS ENUM ('SCHEDULED', 'CANCELLED');
CREATE TYPE attendance_state AS ENUM ('PRESENTE', 'AUSENTE', 'JUSTIFICADO');

CREATE FUNCTION set_updated_at() RETURNS trigger AS $$
BEGIN
  NEW.updated_at := now();
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TABLE sections (
  id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  grade_level varchar(30)  NOT NULL,
  section     varchar(10)  NOT NULL,
  shift       shift_type   NOT NULL,
  created_at  timestamptz  NOT NULL DEFAULT now(),
  updated_at  timestamptz  NOT NULL DEFAULT now(),
  CONSTRAINT uq_sections_grade_section_shift UNIQUE (grade_level, section, shift)
);

CREATE TABLE students (
  id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  id_section  uuid         NOT NULL REFERENCES sections(id) ON DELETE RESTRICT,
  national_id varchar(20),
  first_name  varchar(50)  NOT NULL,
  last_name   varchar(50)  NOT NULL,
  phone       varchar(30),
  active      boolean      NOT NULL DEFAULT true,
  created_at  timestamptz  NOT NULL DEFAULT now(),
  updated_at  timestamptz  NOT NULL DEFAULT now(),
  CONSTRAINT uq_students_national_id UNIQUE (national_id),
  CONSTRAINT students_names_not_blank CHECK (btrim(first_name) <> '' AND btrim(last_name) <> '')
);

CREATE INDEX idx_students_section_active ON students (id_section, active);
CREATE INDEX idx_students_last_first_name ON students (last_name, first_name);

-- lapses (lapsos): school_term se elimino a proposito (ver cabecera). El
-- numero de lapso es la unica fuente de verdad; el texto '1er'/'2do'/'3er'
-- se calcula en Rust cuando haga falta mostrarlo.
CREATE TABLE lapses (
  id           uuid          PRIMARY KEY DEFAULT gen_random_uuid(),
  school_year  int           NOT NULL,
  lapse_number smallint      NOT NULL,
  start_date   date          NOT NULL,
  end_date     date          NOT NULL,
  practice_day week_day      NOT NULL,
  status       lapse_status  NOT NULL DEFAULT 'PLANNED',
  created_at   timestamptz   NOT NULL DEFAULT now(),
  updated_at   timestamptz   NOT NULL DEFAULT now(),
  CONSTRAINT uq_lapses_year_number UNIQUE (school_year, lapse_number),
  CONSTRAINT lapses_number_between_1_and_3 CHECK (lapse_number BETWEEN 1 AND 3),
  CONSTRAINT lapses_dates_order CHECK (end_date >= start_date),
  CONSTRAINT lapses_no_overlap EXCLUDE USING gist (daterange(start_date, end_date, '[]') WITH &&)
);

CREATE UNIQUE INDEX uq_lapses_single_active ON lapses ((true)) WHERE status = 'ACTIVE';

CREATE TABLE events (
  id               uuid          PRIMARY KEY DEFAULT gen_random_uuid(),
  id_lapse         uuid          NOT NULL REFERENCES lapses(id) ON DELETE RESTRICT,
  event_date       date          NOT NULL,
  type_event       event_type    NOT NULL,
  source           event_source  NOT NULL,
  status           event_status  NOT NULL DEFAULT 'SCHEDULED',
  event_name       varchar(150)  NOT NULL,
  observation      text,
  cancelled_reason text,
  created_at       timestamptz   NOT NULL DEFAULT now(),
  updated_at       timestamptz   NOT NULL DEFAULT now(),
  CONSTRAINT events_name_not_blank CHECK (btrim(event_name) <> ''),
  CONSTRAINT events_cancelled_requires_reason
    CHECK (status <> 'CANCELLED' OR (cancelled_reason IS NOT NULL AND btrim(cancelled_reason) <> '')),
  CONSTRAINT events_scheduled_has_no_reason
    CHECK (status <> 'SCHEDULED' OR cancelled_reason IS NULL)
);

-- Invariante que protege al futuro generador de Rust: sin importar en que
-- lenguaje viva la orquestacion, dos AUTO el mismo dia en el mismo lapso
-- nunca deben poder existir.
CREATE UNIQUE INDEX uq_events_auto_one_per_day ON events (id_lapse, event_date) WHERE source = 'AUTO';
CREATE INDEX idx_events_lapse_date ON events (id_lapse, event_date);

CREATE FUNCTION check_event_date_within_lapse() RETURNS trigger AS $$
DECLARE
  v_start date;
  v_end   date;
BEGIN
  SELECT start_date, end_date INTO v_start, v_end FROM lapses WHERE id = NEW.id_lapse;

  IF NEW.event_date < v_start OR NEW.event_date > v_end THEN
    RAISE EXCEPTION 'event_date % está fuera del lapso % (%)', NEW.event_date, NEW.id_lapse, v_start
      USING ERRCODE = 'check_violation';
  END IF;

  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_events_date_within_lapse
  BEFORE INSERT OR UPDATE OF event_date, id_lapse ON events
  FOR EACH ROW EXECUTE FUNCTION check_event_date_within_lapse();

CREATE FUNCTION check_lapse_dates_against_events() RETURNS trigger AS $$
DECLARE
  v_outside int;
BEGIN
  IF NEW.start_date = OLD.start_date AND NEW.end_date = OLD.end_date THEN
    RETURN NEW;
  END IF;

  SELECT count(*) INTO v_outside
  FROM events
  WHERE id_lapse = NEW.id
    AND (event_date < NEW.start_date OR event_date > NEW.end_date);

  IF v_outside > 0 THEN
    RAISE EXCEPTION 'no se pueden mover las fechas: % evento(s) quedarían fuera del lapso', v_outside
      USING ERRCODE = 'check_violation';
  END IF;

  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_lapses_dates_against_events
  BEFORE UPDATE OF start_date, end_date ON lapses
  FOR EACH ROW EXECUTE FUNCTION check_lapse_dates_against_events();

CREATE TABLE attendance (
  id_event   uuid             NOT NULL REFERENCES events(id) ON DELETE CASCADE,
  id_student uuid             NOT NULL REFERENCES students(id) ON DELETE RESTRICT,
  state      attendance_state NOT NULL,
  created_at timestamptz      NOT NULL DEFAULT now(),
  updated_at timestamptz      NOT NULL DEFAULT now(),
  CONSTRAINT pk_attendance PRIMARY KEY (id_event, id_student)
);

CREATE INDEX idx_attendance_student ON attendance (id_student);

CREATE TABLE grade_overrides (
  id         uuid         PRIMARY KEY DEFAULT gen_random_uuid(),
  id_student uuid         NOT NULL REFERENCES students(id) ON DELETE CASCADE,
  id_lapse   uuid         NOT NULL REFERENCES lapses(id) ON DELETE CASCADE,
  value      numeric(4,2) NOT NULL,
  reason     text,
  created_at timestamptz  NOT NULL DEFAULT now(),
  updated_at timestamptz  NOT NULL DEFAULT now(),
  CONSTRAINT uq_grade_overrides_student_lapse UNIQUE (id_student, id_lapse),
  CONSTRAINT grade_overrides_value_range CHECK (value >= 0 AND value <= 20)
);

-- La tabla `notes` (notas) se eliminó del diseño: no correspondía a ninguna
-- decisión de dominio acordada y no se sabe su origen. Si en el futuro hace
-- falta un registro de observaciones libres por estudiante, se agrega con
-- una migración nueva y una decisión de dominio explícita, no se reintroduce
-- por arrastre.

CREATE TABLE audit_logs (
  id         bigint      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  entity     text        NOT NULL,
  entity_id  uuid        NOT NULL,
  action     text        NOT NULL,
  data       jsonb       NOT NULL DEFAULT '{}'::jsonb,
  created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX idx_audit_logs_entity ON audit_logs (entity, entity_id, created_at DESC);

CREATE TRIGGER trg_sections_updated_at    BEFORE UPDATE ON sections        FOR EACH ROW EXECUTE FUNCTION set_updated_at();
CREATE TRIGGER trg_students_updated_at    BEFORE UPDATE ON students        FOR EACH ROW EXECUTE FUNCTION set_updated_at();
CREATE TRIGGER trg_lapses_updated_at      BEFORE UPDATE ON lapses          FOR EACH ROW EXECUTE FUNCTION set_updated_at();
CREATE TRIGGER trg_events_updated_at      BEFORE UPDATE ON events          FOR EACH ROW EXECUTE FUNCTION set_updated_at();
CREATE TRIGGER trg_attendance_updated_at  BEFORE UPDATE ON attendance      FOR EACH ROW EXECUTE FUNCTION set_updated_at();
CREATE TRIGGER trg_grades_updated_at      BEFORE UPDATE ON grade_overrides FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- =========================================================================
-- REGLAS DE DOMINIO (defensa en profundidad)
-- =========================================================================
-- Las 10 reglas de dominio del proyecto, expresadas como constraints, indices
-- y triggers. Se apoyan en las tablas de arriba; no las reemplazan.
--
-- Principio rector: un hecho se protege por la evidencia que lo respalda, no
-- por la fecha. Un evento "ocurrio" si tiene al menos una fila en attendance.
-- Asi un evento previsto que se suspendio (lluvia, acto escolar) nunca quedo
-- congelado artificialmente y se puede cancelar sin problema.
--
-- Nota sobre el reinicio: estos triggers son invariantes ("esto no puede
-- pasar aunque el backend tenga un bug"), y por eso se quedan en la base sin
-- importar el lenguaje del backend. Lo que se movio a Rust fue la
-- orquestacion (generar_practicas_auto, reprogramar_practice_day), que es
-- secuencia de negocio, no invariante.
--
-- ---------------------------------------------------------------------------
-- Funciones compartidas
-- ---------------------------------------------------------------------------

-- EXTRACT(ISODOW FROM date) devuelve 1=lunes ... 7=domingo, que es exactamente
-- el orden del enum week_day. Por eso el mapeo es directo. Se mantiene en la
-- base porque el trigger de la regla 1 la necesita para validar el
-- practice_day; el dominio de Rust puede tener su propia versión para lógica
-- de aplicación, pero la que protege la invariante es esta.
CREATE FUNCTION week_day_of(d date) RETURNS week_day AS $$
  SELECT (ARRAY['MONDAY', 'TUESDAY', 'WEDNESDAY', 'THURSDAY',
                'FRIDAY', 'SATURDAY', 'SUNDAY']::week_day[])
         [EXTRACT(ISODOW FROM d)::int];
$$ LANGUAGE sql IMMUTABLE;

-- La unica definicion de "este evento ocurrio". STABLE y sin trigger, para
-- poder llamarla desde cualquier constraint, consulta o test.
CREATE FUNCTION event_occurred(p_id_event uuid) RETURNS boolean AS $$
  SELECT EXISTS (SELECT 1 FROM attendance WHERE id_event = p_id_event);
$$ LANGUAGE sql STABLE;

CREATE FUNCTION assert_lapse_is_open(p_id_lapse uuid) RETURNS void AS $$
DECLARE
  v_status lapse_status;
BEGIN
  SELECT status INTO v_status FROM lapses WHERE id = p_id_lapse;

  IF v_status = 'CLOSED' THEN
    RAISE EXCEPTION 'el lapso % está CLOSED: no admite datos nuevos', p_id_lapse
      USING ERRCODE = 'check_violation';
  END IF;
END $$ LANGUAGE plpgsql;

-- ---------------------------------------------------------------------------
-- Regla 1: un evento AUTO debe caer en el practice_day de su lapso
-- ---------------------------------------------------------------------------
-- Se extiende la funcion existente en vez de crear otra: misma firma
-- (RETURNS trigger) asi que el trigger trg_events_date_within_lapse sigue
-- enlazado. Abajo se recrea el trigger para anadir 'source' a la lista de
-- columnas que lo disparan. Esta invariante es la que hace segura, desde el
-- lado de la base, la generacion automatica que ahora vive en Rust: sin
-- importar como Rust arme el INSERT, si un AUTO no cae en el practice_day, la
-- base lo rechaza igual.
CREATE OR REPLACE FUNCTION check_event_date_within_lapse() RETURNS trigger AS $$
DECLARE
  v_start    date;
  v_end      date;
  v_practice week_day;
BEGIN
  SELECT start_date, end_date, practice_day INTO v_start, v_end, v_practice
  FROM lapses
  WHERE id = NEW.id_lapse;

  IF NEW.event_date < v_start OR NEW.event_date > v_end THEN
    RAISE EXCEPTION 'event_date % está fuera del lapso % (%)', NEW.event_date, NEW.id_lapse, v_start
      USING ERRCODE = 'check_violation';
  END IF;

  IF NEW.source = 'AUTO' AND week_day_of(NEW.event_date) <> v_practice THEN
    RAISE EXCEPTION 'un evento AUTO del lapso % debe caer en su practice_day %, pero % es %',
      NEW.id_lapse, v_practice, NEW.event_date, week_day_of(NEW.event_date)
      USING ERRCODE = 'check_violation';
  END IF;

  RETURN NEW;
END $$ LANGUAGE plpgsql;

DROP TRIGGER trg_events_date_within_lapse ON events;

CREATE TRIGGER trg_events_date_within_lapse
  BEFORE INSERT OR UPDATE OF event_date, id_lapse, source ON events
  FOR EACH ROW EXECUTE FUNCTION check_event_date_within_lapse();

-- ---------------------------------------------------------------------------
-- Regla 2: source AUTO implica PRACTICA_REGULAR
-- ---------------------------------------------------------------------------
ALTER TABLE events
  ADD CONSTRAINT events_auto_is_regular_practice
  CHECK (source <> 'AUTO' OR type_event = 'PRACTICA_REGULAR');

-- ---------------------------------------------------------------------------
-- Regla 3: un estudiante con asistencia no cambia de seccion
-- ---------------------------------------------------------------------------
CREATE FUNCTION check_student_section_immutable() RETURNS trigger AS $$
DECLARE
  v_count int;
BEGIN
  IF NEW.id_section IS NOT DISTINCT FROM OLD.id_section THEN
    RETURN NEW;
  END IF;

  SELECT count(*) INTO v_count FROM attendance WHERE id_student = OLD.id;

  IF v_count > 0 THEN
    RAISE EXCEPTION
      'no se puede cambiar la sección del estudiante %: tiene % asistencia(s) registrada(s)',
      OLD.id, v_count
      USING ERRCODE = 'check_violation';
  END IF;

  RETURN NEW;
END $$ LANGUAGE plpgsql;

CREATE TRIGGER trg_students_section_immutable
  BEFORE UPDATE OF id_section ON students
  FOR EACH ROW EXECUTE FUNCTION check_student_section_immutable();

-- ---------------------------------------------------------------------------
-- Reglas 4 y 5: un lapso CLOSED es terminal
-- ---------------------------------------------------------------------------
-- 4: no se mueven fechas ni practice_day una vez cerrado.
-- 5: no se reabre (CLOSED -> PLANNED/ACTIVE).
-- Ambas salen del mismo trigger porque CLOSED no tiene salida.
CREATE FUNCTION lapses_closed_are_frozen() RETURNS trigger AS $$
BEGIN
  IF OLD.status = 'CLOSED' THEN
    RAISE EXCEPTION 'el lapso % está CLOSED y no se puede modificar (operación %)', OLD.id, TG_OP
      USING ERRCODE = 'check_violation';
  END IF;

  RETURN NEW;
END $$ LANGUAGE plpgsql;

CREATE TRIGGER trg_lapses_closed_frozen
  BEFORE UPDATE ON lapses
  FOR EACH ROW EXECUTE FUNCTION lapses_closed_are_frozen();

-- Regla 4 (cont.): las tablas hijas no admiten datos nuevos en un lapso CLOSED.
CREATE FUNCTION check_event_lapse_open() RETURNS trigger AS $$
BEGIN
  PERFORM assert_lapse_is_open(NEW.id_lapse);
  RETURN NEW;
END $$ LANGUAGE plpgsql;

CREATE TRIGGER trg_events_lapse_open
  BEFORE INSERT ON events
  FOR EACH ROW EXECUTE FUNCTION check_event_lapse_open();

-- Reglas 4 y 7 en un solo trigger. El orden importa: primero se valida que el
-- evento no este CANCELLED y despues que el lapso no este CLOSED. Asi el caso
-- de prueba "asistencia en cancelado" usa un lapso abierto y el de "lapso
-- CLOSED" un evento SCHEDULED, y cada uno aisla una sola causa.
CREATE FUNCTION check_attendance_guards() RETURNS trigger AS $$
DECLARE
  v_status   event_status;
  v_id_lapse uuid;
BEGIN
  SELECT e.status, e.id_lapse INTO v_status, v_id_lapse
  FROM events e
  WHERE e.id = NEW.id_event;

  IF v_status = 'CANCELLED' THEN
    RAISE EXCEPTION 'no se puede registrar asistencia en el evento %, está CANCELLED', NEW.id_event
      USING ERRCODE = 'check_violation';
  END IF;

  PERFORM assert_lapse_is_open(v_id_lapse);

  RETURN NEW;
END $$ LANGUAGE plpgsql;

CREATE TRIGGER trg_attendance_guards
  BEFORE INSERT OR UPDATE ON attendance
  FOR EACH ROW EXECUTE FUNCTION check_attendance_guards();

CREATE FUNCTION check_grade_override_lapse_open() RETURNS trigger AS $$
BEGIN
  PERFORM assert_lapse_is_open(NEW.id_lapse);
  RETURN NEW;
END $$ LANGUAGE plpgsql;

CREATE TRIGGER trg_grade_overrides_lapse_open
  BEFORE INSERT ON grade_overrides
  FOR EACH ROW EXECUTE FUNCTION check_grade_override_lapse_open();

-- ---------------------------------------------------------------------------
-- Regla 6: audit_logs es append-only
-- ---------------------------------------------------------------------------
-- No se protege con permisos: la app comparte la misma conexion que el
-- negocio. Ojo: TRUNCATE no dispara triggers de fila, asi que un TRUNCATE de
-- limpieza en un entorno de pruebas sigue funcionando.
CREATE FUNCTION audit_logs_append_only() RETURNS trigger AS $$
BEGIN
  RAISE EXCEPTION 'audit_logs es append-only: no se permite %', TG_OP
    USING ERRCODE = 'check_violation';
END $$ LANGUAGE plpgsql;

CREATE TRIGGER trg_audit_logs_append_only
  BEFORE UPDATE OR DELETE ON audit_logs
  FOR EACH ROW EXECUTE FUNCTION audit_logs_append_only();

-- ---------------------------------------------------------------------------
-- Regla 9: un evento que ya ocurrio queda congelado
-- ---------------------------------------------------------------------------
-- Se congelan las columnas que forman parte del historial y del calculo de N.
-- event_name y observation quedan fuera a proposito: son etiqueta y
-- anotacion. source tambien se congela: cambiarlo despues convertiria un
-- evento manual en uno regenerable por la logica de reprogramacion (ahora en
-- Rust, pero la invariante sigue siendo responsabilidad de la base).
CREATE FUNCTION check_event_frozen_by_attendance() RETURNS trigger AS $$
BEGIN
  IF event_occurred(OLD.id) AND (
       NEW.event_date IS DISTINCT FROM OLD.event_date
    OR NEW.id_lapse     IS DISTINCT FROM OLD.id_lapse
    OR NEW.type_event   IS DISTINCT FROM OLD.type_event
    OR NEW.source       IS DISTINCT FROM OLD.source
    OR (OLD.status <> 'CANCELLED' AND NEW.status = 'CANCELLED')
  ) THEN
    RAISE EXCEPTION 'el evento % ya ocurrió (tiene asistencia): no se puede modificar', OLD.id
      USING ERRCODE = 'check_violation';
  END IF;

  RETURN NEW;
END $$ LANGUAGE plpgsql;

CREATE TRIGGER trg_events_frozen_by_attendance
  BEFORE UPDATE ON events
  FOR EACH ROW EXECUTE FUNCTION check_event_frozen_by_attendance();

-- ---------------------------------------------------------------------------
-- Regla 10: la asistencia no se borra, se corrige
-- ---------------------------------------------------------------------------
-- Con esto la regla 9 es permanente: como no se puede borrar asistencia, un
-- evento que ocurrio queda congelado para siempre. Un DELETE manual por error
-- ya no puede "desocurrir" un evento ni abrir la puerta a borrarlo.
CREATE FUNCTION attendance_no_delete() RETURNS trigger AS $$
BEGIN
  RAISE EXCEPTION
    'no se puede borrar la asistencia: corregí el estado con UPDATE (queda auditado)'
    USING ERRCODE = 'check_violation';
END $$ LANGUAGE plpgsql;

CREATE TRIGGER trg_attendance_no_delete
  BEFORE DELETE ON attendance
  FOR EACH ROW EXECUTE FUNCTION attendance_no_delete();

-- La correccion es solo del estado. Mover la asistencia a otro evento o a
-- otro estudiante no es corregir: es reescribir el pasado, y ademas liberaria
-- al evento viejo de la regla 9, dejando congelado el nuevo por un hecho que
-- nunca ocurrio en el.
CREATE FUNCTION attendance_identity_immutable() RETURNS trigger AS $$
BEGIN
  RAISE EXCEPTION
    'no se puede cambiar el evento ni el estudiante de la asistencia (% / %): solo se corrige el estado',
    OLD.id_event, OLD.id_student
    USING ERRCODE = 'check_violation';
END $$ LANGUAGE plpgsql;

CREATE TRIGGER trg_attendance_identity_immutable
  BEFORE UPDATE OF id_event, id_student ON attendance
  FOR EACH ROW EXECUTE FUNCTION attendance_identity_immutable();

-- ---------------------------------------------------------------------------
-- Regla 8: nada se borra fisicamente
-- ---------------------------------------------------------------------------
-- Tres capas, de la mas explicita a la mas general:
--
--   1. Triggers: dan el mensaje util ("cancelalo con un motivo", "tiene 3
--      eventos"). Cubren los casos que RESTRICT no puede ver, porque no hay
--      filas hijas que lo disparen: un evento sin asistencia y un lapso
--      recien capturado.
--   2. RESTRICT en todas las FKs: convierte el borrado en cascada en un error
--      en vez de una perdida silenciosa de datos.
--   3. Nada de ON DELETE CASCADE en ninguna FK.
--
-- El orden importa: los triggers van primero para que el error que ve el
-- usuario sea el trigger y no un "violates foreign key constraint" sin
-- contexto. Las FK quedan como red de seguridad: si alguien quita un trigger,
-- la FK sigue impidiendo el borrado en cascada.
--
-- Eventos: jamas se borran, ni siquiera los que aun no ocurrieron. Se
-- cancelan con motivo y se crea uno nuevo (orquestado desde Rust; la base
-- solo impide el borrado).
CREATE FUNCTION events_no_delete() RETURNS trigger AS $$
DECLARE
  v_ocurrio boolean;
BEGIN
  v_ocurrio := event_occurred(OLD.id);

  RAISE EXCEPTION
    'no se puede borrar el evento % (%) [%]: nada se borra físicamente; se cancela con un motivo y se crea otro',
    OLD.id, OLD.event_name,
    CASE WHEN v_ocurrio THEN 'ya ocurrió' ELSE 'aún no ocurrió' END
    USING ERRCODE = 'check_violation';
END $$ LANGUAGE plpgsql;

CREATE TRIGGER trg_events_no_delete
  BEFORE DELETE ON events
  FOR EACH ROW EXECUTE FUNCTION events_no_delete();

-- Lapses: un error de captura recien hecho SI se puede deshacer, pero solo si
-- no deja rastro. Tres condiciones, todas necesarias:
--   - no CLOSED: un lapso cerrado es historial;
--   - no terminado (end_date >= hoy): un lapso que ya ocurrio es historial,
--     aunque no tenga eventos;
--   - vacio: sin eventos ni overrides (ya no se cuentan notas: la tabla no
--     existe).
CREATE FUNCTION lapses_delete_guard() RETURNS trigger AS $$
DECLARE
  v_eventos   int;
  v_overrides int;
BEGIN
  IF OLD.status = 'CLOSED' THEN
    RAISE EXCEPTION
      'no se puede borrar el lapso % (%/%): está CLOSED y es un registro histórico',
      OLD.id, OLD.school_year, OLD.lapse_number
      USING ERRCODE = 'check_violation';
  END IF;

  IF OLD.end_date < CURRENT_DATE THEN
    RAISE EXCEPTION
      'no se puede borrar el lapso % (%/%): terminó el % y ya es parte del historial',
      OLD.id, OLD.school_year, OLD.lapse_number, OLD.end_date
      USING ERRCODE = 'check_violation';
  END IF;

  SELECT count(*) INTO v_eventos   FROM events         WHERE id_lapse = OLD.id;
  SELECT count(*) INTO v_overrides FROM grade_overrides WHERE id_lapse = OLD.id;

  IF v_eventos + v_overrides > 0 THEN
    RAISE EXCEPTION
      'no se puede borrar el lapso % (%/%): tiene % evento(s) y % override(s) asociados',
      OLD.id, OLD.school_year, OLD.lapse_number, v_eventos, v_overrides
      USING ERRCODE = 'check_violation';
  END IF;

  RETURN OLD;
END $$ LANGUAGE plpgsql;

CREATE TRIGGER trg_lapses_delete_guard
  BEFORE DELETE ON lapses
  FOR EACH ROW EXECUTE FUNCTION lapses_delete_guard();

-- RESTRICT en todas las FKs. Con CASCADE, borrar un evento borraba la
-- asistencia en silencio y borrar un estudiante se llevaba sus overrides.
ALTER TABLE attendance DROP CONSTRAINT attendance_id_event_fkey;
ALTER TABLE attendance ADD CONSTRAINT attendance_id_event_fkey
  FOREIGN KEY (id_event) REFERENCES events(id) ON DELETE RESTRICT;

ALTER TABLE grade_overrides DROP CONSTRAINT grade_overrides_id_student_fkey;
ALTER TABLE grade_overrides ADD CONSTRAINT grade_overrides_id_student_fkey
  FOREIGN KEY (id_student) REFERENCES students(id) ON DELETE RESTRICT;

ALTER TABLE grade_overrides DROP CONSTRAINT grade_overrides_id_lapse_fkey;
ALTER TABLE grade_overrides ADD CONSTRAINT grade_overrides_id_lapse_fkey
  FOREIGN KEY (id_lapse) REFERENCES lapses(id) ON DELETE RESTRICT;

-- migrate:down

-- El orden de esto no es libre:
--   1. Las tablas, con CASCADE: se llevan los triggers de fila y los indices.
--   2. Las funciones de disparador, que ya no tienen trigger que las
--      referencie.
--   3. Las funciones comunes.
--   4. Los tipos, al final.

DROP TABLE IF EXISTS attendance;
DROP TABLE IF EXISTS grade_overrides;
DROP TABLE IF EXISTS events;
DROP TABLE IF EXISTS audit_logs;
DROP TABLE IF EXISTS lapses;
DROP TABLE IF EXISTS students;
DROP TABLE IF EXISTS sections CASCADE;

DROP FUNCTION IF EXISTS attendance_identity_immutable();
DROP FUNCTION IF EXISTS attendance_no_delete();
DROP FUNCTION IF EXISTS audit_logs_append_only();
DROP FUNCTION IF EXISTS check_attendance_guards();
DROP FUNCTION IF EXISTS check_event_date_within_lapse();
DROP FUNCTION IF EXISTS check_event_frozen_by_attendance();
DROP FUNCTION IF EXISTS check_event_lapse_open();
DROP FUNCTION IF EXISTS check_grade_override_lapse_open();
DROP FUNCTION IF EXISTS check_lapse_dates_against_events();
DROP FUNCTION IF EXISTS check_student_section_immutable();
DROP FUNCTION IF EXISTS events_no_delete();
DROP FUNCTION IF EXISTS lapses_closed_are_frozen();
DROP FUNCTION IF EXISTS lapses_delete_guard();
DROP FUNCTION IF EXISTS set_updated_at();

DROP FUNCTION IF EXISTS assert_lapse_is_open(uuid);
DROP FUNCTION IF EXISTS event_occurred(uuid);
DROP FUNCTION IF EXISTS week_day_of(date);

DROP TYPE IF EXISTS attendance_state;
DROP TYPE IF EXISTS event_status;
DROP TYPE IF EXISTS event_source;
DROP TYPE IF EXISTS event_type;
DROP TYPE IF EXISTS lapse_status;
DROP TYPE IF EXISTS week_day;
DROP TYPE IF EXISTS shift_type;
