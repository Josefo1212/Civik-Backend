\restrict dbmate

-- Dumped from database version 18.6 (Debian 18.6-1.pgdg13+2)
-- Dumped by pg_dump version 18.6 (Debian 18.6-1.pgdg13+2)

SET statement_timeout = 0;
SET lock_timeout = 0;
SET idle_in_transaction_session_timeout = 0;
SET transaction_timeout = 0;
SET client_encoding = 'UTF8';
SET standard_conforming_strings = on;
SELECT pg_catalog.set_config('search_path', '', false);
SET check_function_bodies = false;
SET xmloption = content;
SET client_min_messages = warning;
SET row_security = off;

--
-- Name: public; Type: SCHEMA; Schema: -; Owner: -
--

-- *not* creating schema, since initdb creates it


--
-- Name: SCHEMA public; Type: COMMENT; Schema: -; Owner: -
--

COMMENT ON SCHEMA public IS '';


--
-- Name: attendance_state; Type: TYPE; Schema: public; Owner: -
--

CREATE TYPE public.attendance_state AS ENUM (
    'PRESENTE',
    'AUSENTE',
    'JUSTIFICADO'
);


--
-- Name: event_source; Type: TYPE; Schema: public; Owner: -
--

CREATE TYPE public.event_source AS ENUM (
    'AUTO',
    'MANUAL'
);


--
-- Name: event_status; Type: TYPE; Schema: public; Owner: -
--

CREATE TYPE public.event_status AS ENUM (
    'SCHEDULED',
    'CANCELLED'
);


--
-- Name: event_type; Type: TYPE; Schema: public; Owner: -
--

CREATE TYPE public.event_type AS ENUM (
    'PRACTICA_REGULAR',
    'PRACTICA_EXTRA',
    'PRESENTACION'
);


--
-- Name: lapse_status; Type: TYPE; Schema: public; Owner: -
--

CREATE TYPE public.lapse_status AS ENUM (
    'PLANNED',
    'ACTIVE',
    'CLOSED'
);


--
-- Name: shift_type; Type: TYPE; Schema: public; Owner: -
--

CREATE TYPE public.shift_type AS ENUM (
    'MATUTINO',
    'VESPERTINO'
);


--
-- Name: week_day; Type: TYPE; Schema: public; Owner: -
--

CREATE TYPE public.week_day AS ENUM (
    'MONDAY',
    'TUESDAY',
    'WEDNESDAY',
    'THURSDAY',
    'FRIDAY',
    'SATURDAY',
    'SUNDAY'
);


--
-- Name: assert_lapse_is_open(uuid); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.assert_lapse_is_open(p_id_lapse uuid) RETURNS void
    LANGUAGE plpgsql
    AS $$
DECLARE
  v_status lapse_status;
BEGIN
  SELECT status INTO v_status FROM lapses WHERE id = p_id_lapse;

  IF v_status = 'CLOSED' THEN
    RAISE EXCEPTION 'el lapso % está CLOSED: no admite datos nuevos', p_id_lapse
      USING ERRCODE = 'check_violation';
  END IF;
END $$;


--
-- Name: attendance_identity_immutable(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.attendance_identity_immutable() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
  RAISE EXCEPTION
    'no se puede cambiar el evento ni el estudiante de la asistencia (% / %): solo se corrige el estado',
    OLD.id_event, OLD.id_student
    USING ERRCODE = 'check_violation';
END $$;


--
-- Name: attendance_no_delete(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.attendance_no_delete() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
  RAISE EXCEPTION
    'no se puede borrar la asistencia: corregí el estado con UPDATE (queda auditado)'
    USING ERRCODE = 'check_violation';
END $$;


--
-- Name: audit_logs_append_only(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.audit_logs_append_only() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
  RAISE EXCEPTION 'audit_logs es append-only: no se permite %', TG_OP
    USING ERRCODE = 'check_violation';
END $$;


--
-- Name: check_attendance_guards(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.check_attendance_guards() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
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
END $$;


--
-- Name: check_event_date_within_lapse(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.check_event_date_within_lapse() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
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
END $$;


--
-- Name: check_event_frozen_by_attendance(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.check_event_frozen_by_attendance() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
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
END $$;


--
-- Name: check_event_lapse_open(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.check_event_lapse_open() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
  PERFORM assert_lapse_is_open(NEW.id_lapse);
  RETURN NEW;
END $$;


--
-- Name: check_grade_override_lapse_open(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.check_grade_override_lapse_open() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
  PERFORM assert_lapse_is_open(NEW.id_lapse);
  RETURN NEW;
END $$;


--
-- Name: check_lapse_dates_against_events(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.check_lapse_dates_against_events() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
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
$$;


--
-- Name: check_student_section_immutable(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.check_student_section_immutable() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
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
END $$;


--
-- Name: event_occurred(uuid); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.event_occurred(p_id_event uuid) RETURNS boolean
    LANGUAGE sql STABLE
    AS $$
  SELECT EXISTS (SELECT 1 FROM attendance WHERE id_event = p_id_event);
$$;


--
-- Name: events_no_delete(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.events_no_delete() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
DECLARE
  v_ocurrio boolean;
BEGIN
  v_ocurrio := event_occurred(OLD.id);

  RAISE EXCEPTION
    'no se puede borrar el evento % (%) [%]: nada se borra físicamente; se cancela con un motivo y se crea otro',
    OLD.id, OLD.event_name,
    CASE WHEN v_ocurrio THEN 'ya ocurrió' ELSE 'aún no ocurrió' END
    USING ERRCODE = 'check_violation';
END $$;


--
-- Name: lapses_closed_are_frozen(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.lapses_closed_are_frozen() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
  IF OLD.status = 'CLOSED' THEN
    RAISE EXCEPTION 'el lapso % está CLOSED y no se puede modificar (operación %)', OLD.id, TG_OP
      USING ERRCODE = 'check_violation';
  END IF;

  RETURN NEW;
END $$;


--
-- Name: lapses_delete_guard(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.lapses_delete_guard() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
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
END $$;


--
-- Name: set_updated_at(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.set_updated_at() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
  NEW.updated_at := now();
  RETURN NEW;
END;
$$;


--
-- Name: week_day_of(date); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.week_day_of(d date) RETURNS public.week_day
    LANGUAGE sql IMMUTABLE
    AS $$
  SELECT (ARRAY['MONDAY', 'TUESDAY', 'WEDNESDAY', 'THURSDAY',
                'FRIDAY', 'SATURDAY', 'SUNDAY']::week_day[])
         [EXTRACT(ISODOW FROM d)::int];
$$;


SET default_tablespace = '';

SET default_table_access_method = heap;

--
-- Name: attendance; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.attendance (
    id_event uuid NOT NULL,
    id_student uuid NOT NULL,
    state public.attendance_state NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: audit_logs; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.audit_logs (
    id bigint NOT NULL,
    entity text NOT NULL,
    entity_id uuid NOT NULL,
    action text NOT NULL,
    data jsonb DEFAULT '{}'::jsonb NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: audit_logs_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

ALTER TABLE public.audit_logs ALTER COLUMN id ADD GENERATED ALWAYS AS IDENTITY (
    SEQUENCE NAME public.audit_logs_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);


--
-- Name: events; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.events (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    id_lapse uuid NOT NULL,
    event_date date NOT NULL,
    type_event public.event_type NOT NULL,
    source public.event_source NOT NULL,
    status public.event_status DEFAULT 'SCHEDULED'::public.event_status NOT NULL,
    event_name character varying(150) NOT NULL,
    observation text,
    cancelled_reason text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT events_auto_is_regular_practice CHECK (((source <> 'AUTO'::public.event_source) OR (type_event = 'PRACTICA_REGULAR'::public.event_type))),
    CONSTRAINT events_cancelled_requires_reason CHECK (((status <> 'CANCELLED'::public.event_status) OR ((cancelled_reason IS NOT NULL) AND (btrim(cancelled_reason) <> ''::text)))),
    CONSTRAINT events_name_not_blank CHECK ((btrim((event_name)::text) <> ''::text)),
    CONSTRAINT events_scheduled_has_no_reason CHECK (((status <> 'SCHEDULED'::public.event_status) OR (cancelled_reason IS NULL)))
);


--
-- Name: grade_overrides; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.grade_overrides (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    id_student uuid NOT NULL,
    id_lapse uuid NOT NULL,
    value numeric(4,2) NOT NULL,
    reason text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT grade_overrides_value_range CHECK (((value >= (0)::numeric) AND (value <= (20)::numeric)))
);


--
-- Name: lapses; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.lapses (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    school_year integer NOT NULL,
    lapse_number smallint NOT NULL,
    start_date date NOT NULL,
    end_date date NOT NULL,
    practice_day public.week_day NOT NULL,
    status public.lapse_status DEFAULT 'PLANNED'::public.lapse_status NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT lapses_dates_order CHECK ((end_date >= start_date)),
    CONSTRAINT lapses_number_between_1_and_3 CHECK (((lapse_number >= 1) AND (lapse_number <= 3)))
);


--
-- Name: schema_migrations; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.schema_migrations (
    version character varying NOT NULL
);


--
-- Name: sections; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.sections (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    grade_level character varying(30) NOT NULL,
    section character varying(10) NOT NULL,
    shift public.shift_type NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    active boolean DEFAULT true NOT NULL,
    CONSTRAINT sections_not_blank CHECK (((btrim((grade_level)::text) <> ''::text) AND (btrim((section)::text) <> ''::text)))
);


--
-- Name: students; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.students (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    id_section uuid NOT NULL,
    first_name character varying(50) NOT NULL,
    last_name character varying(50) NOT NULL,
    phone character varying(30),
    active boolean DEFAULT true NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT students_names_not_blank CHECK (((btrim((first_name)::text) <> ''::text) AND (btrim((last_name)::text) <> ''::text)))
);


--
-- Name: audit_logs audit_logs_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.audit_logs
    ADD CONSTRAINT audit_logs_pkey PRIMARY KEY (id);


--
-- Name: events events_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.events
    ADD CONSTRAINT events_pkey PRIMARY KEY (id);


--
-- Name: grade_overrides grade_overrides_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.grade_overrides
    ADD CONSTRAINT grade_overrides_pkey PRIMARY KEY (id);


--
-- Name: lapses lapses_no_overlap; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.lapses
    ADD CONSTRAINT lapses_no_overlap EXCLUDE USING gist (daterange(start_date, end_date, '[]'::text) WITH &&);


--
-- Name: lapses lapses_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.lapses
    ADD CONSTRAINT lapses_pkey PRIMARY KEY (id);


--
-- Name: attendance pk_attendance; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.attendance
    ADD CONSTRAINT pk_attendance PRIMARY KEY (id_event, id_student);


--
-- Name: schema_migrations schema_migrations_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.schema_migrations
    ADD CONSTRAINT schema_migrations_pkey PRIMARY KEY (version);


--
-- Name: sections sections_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.sections
    ADD CONSTRAINT sections_pkey PRIMARY KEY (id);


--
-- Name: students students_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.students
    ADD CONSTRAINT students_pkey PRIMARY KEY (id);


--
-- Name: grade_overrides uq_grade_overrides_student_lapse; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.grade_overrides
    ADD CONSTRAINT uq_grade_overrides_student_lapse UNIQUE (id_student, id_lapse);


--
-- Name: lapses uq_lapses_year_number; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.lapses
    ADD CONSTRAINT uq_lapses_year_number UNIQUE (school_year, lapse_number);


--
-- Name: sections uq_sections_grade_section_shift; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.sections
    ADD CONSTRAINT uq_sections_grade_section_shift UNIQUE (grade_level, section, shift);


--
-- Name: idx_attendance_student; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_attendance_student ON public.attendance USING btree (id_student);


--
-- Name: idx_audit_logs_entity; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_audit_logs_entity ON public.audit_logs USING btree (entity, entity_id, created_at DESC);


--
-- Name: idx_events_lapse_date; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_events_lapse_date ON public.events USING btree (id_lapse, event_date);


--
-- Name: idx_students_last_first_name; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_students_last_first_name ON public.students USING btree (last_name, first_name);


--
-- Name: idx_students_section_active; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_students_section_active ON public.students USING btree (id_section, active);


--
-- Name: uq_events_auto_one_per_day; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX uq_events_auto_one_per_day ON public.events USING btree (id_lapse, event_date) WHERE (source = 'AUTO'::public.event_source);


--
-- Name: uq_lapses_single_active; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX uq_lapses_single_active ON public.lapses USING btree ((true)) WHERE (status = 'ACTIVE'::public.lapse_status);


--
-- Name: attendance trg_attendance_guards; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_attendance_guards BEFORE INSERT OR UPDATE ON public.attendance FOR EACH ROW EXECUTE FUNCTION public.check_attendance_guards();


--
-- Name: attendance trg_attendance_identity_immutable; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_attendance_identity_immutable BEFORE UPDATE OF id_event, id_student ON public.attendance FOR EACH ROW EXECUTE FUNCTION public.attendance_identity_immutable();


--
-- Name: attendance trg_attendance_no_delete; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_attendance_no_delete BEFORE DELETE ON public.attendance FOR EACH ROW EXECUTE FUNCTION public.attendance_no_delete();


--
-- Name: attendance trg_attendance_updated_at; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_attendance_updated_at BEFORE UPDATE ON public.attendance FOR EACH ROW EXECUTE FUNCTION public.set_updated_at();


--
-- Name: audit_logs trg_audit_logs_append_only; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_audit_logs_append_only BEFORE DELETE OR UPDATE ON public.audit_logs FOR EACH ROW EXECUTE FUNCTION public.audit_logs_append_only();


--
-- Name: events trg_events_date_within_lapse; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_events_date_within_lapse BEFORE INSERT OR UPDATE OF event_date, id_lapse, source ON public.events FOR EACH ROW EXECUTE FUNCTION public.check_event_date_within_lapse();


--
-- Name: events trg_events_frozen_by_attendance; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_events_frozen_by_attendance BEFORE UPDATE ON public.events FOR EACH ROW EXECUTE FUNCTION public.check_event_frozen_by_attendance();


--
-- Name: events trg_events_lapse_open; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_events_lapse_open BEFORE INSERT ON public.events FOR EACH ROW EXECUTE FUNCTION public.check_event_lapse_open();


--
-- Name: events trg_events_no_delete; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_events_no_delete BEFORE DELETE ON public.events FOR EACH ROW EXECUTE FUNCTION public.events_no_delete();


--
-- Name: events trg_events_updated_at; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_events_updated_at BEFORE UPDATE ON public.events FOR EACH ROW EXECUTE FUNCTION public.set_updated_at();


--
-- Name: grade_overrides trg_grade_overrides_lapse_open; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_grade_overrides_lapse_open BEFORE INSERT ON public.grade_overrides FOR EACH ROW EXECUTE FUNCTION public.check_grade_override_lapse_open();


--
-- Name: grade_overrides trg_grades_updated_at; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_grades_updated_at BEFORE UPDATE ON public.grade_overrides FOR EACH ROW EXECUTE FUNCTION public.set_updated_at();


--
-- Name: lapses trg_lapses_closed_frozen; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_lapses_closed_frozen BEFORE UPDATE ON public.lapses FOR EACH ROW EXECUTE FUNCTION public.lapses_closed_are_frozen();


--
-- Name: lapses trg_lapses_dates_against_events; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_lapses_dates_against_events BEFORE UPDATE OF start_date, end_date ON public.lapses FOR EACH ROW EXECUTE FUNCTION public.check_lapse_dates_against_events();


--
-- Name: lapses trg_lapses_delete_guard; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_lapses_delete_guard BEFORE DELETE ON public.lapses FOR EACH ROW EXECUTE FUNCTION public.lapses_delete_guard();


--
-- Name: lapses trg_lapses_updated_at; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_lapses_updated_at BEFORE UPDATE ON public.lapses FOR EACH ROW EXECUTE FUNCTION public.set_updated_at();


--
-- Name: sections trg_sections_updated_at; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_sections_updated_at BEFORE UPDATE ON public.sections FOR EACH ROW EXECUTE FUNCTION public.set_updated_at();


--
-- Name: students trg_students_section_immutable; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_students_section_immutable BEFORE UPDATE OF id_section ON public.students FOR EACH ROW EXECUTE FUNCTION public.check_student_section_immutable();


--
-- Name: students trg_students_updated_at; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_students_updated_at BEFORE UPDATE ON public.students FOR EACH ROW EXECUTE FUNCTION public.set_updated_at();


--
-- Name: attendance attendance_id_event_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.attendance
    ADD CONSTRAINT attendance_id_event_fkey FOREIGN KEY (id_event) REFERENCES public.events(id) ON DELETE RESTRICT;


--
-- Name: attendance attendance_id_student_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.attendance
    ADD CONSTRAINT attendance_id_student_fkey FOREIGN KEY (id_student) REFERENCES public.students(id) ON DELETE RESTRICT;


--
-- Name: events events_id_lapse_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.events
    ADD CONSTRAINT events_id_lapse_fkey FOREIGN KEY (id_lapse) REFERENCES public.lapses(id) ON DELETE RESTRICT;


--
-- Name: grade_overrides grade_overrides_id_lapse_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.grade_overrides
    ADD CONSTRAINT grade_overrides_id_lapse_fkey FOREIGN KEY (id_lapse) REFERENCES public.lapses(id) ON DELETE RESTRICT;


--
-- Name: grade_overrides grade_overrides_id_student_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.grade_overrides
    ADD CONSTRAINT grade_overrides_id_student_fkey FOREIGN KEY (id_student) REFERENCES public.students(id) ON DELETE RESTRICT;


--
-- Name: students students_id_section_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.students
    ADD CONSTRAINT students_id_section_fkey FOREIGN KEY (id_section) REFERENCES public.sections(id) ON DELETE RESTRICT;


--
-- PostgreSQL database dump complete
--

\unrestrict dbmate


--
-- Dbmate schema migrations
--

INSERT INTO public.schema_migrations (version) VALUES
    ('20260926162010'),
    ('20261006010939'),
    ('20261007120000');
