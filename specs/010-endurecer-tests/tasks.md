# Tareas 010 — Endurecer tests (invariantes de base)

Una tarea = test primero (ver que falla si la invariante no existiera),
después el aserto contra la base real. Máx. 20-30 min cada una.

- [x] **T1. Andamiaje de `tests/constraints.rs` + reglas de `lapses`.** RF-1, RF-2
  - Hecho cuando: existe el archivo con `mod common`, tipo `Resultado`, helpers
    (`montar_lapso`, `intento` con savepoint) y los casos de
    `lapses_dates_order` (end<start falla, end=start pasa) y
    `lapses_number_between_1_and_3` (0 y 4 fallan, 1 y 3 pasan). `cargo test
    --test constraints` verde.

- [x] **T2. CHECK de `events`.** RF-9, RF-10, RF-11, RF-12
  - Hecho cuando: AUTO con `type_event <> PRACTICA_REGULAR`, CANCELLED sin
    motivo / con motivo en blanco, SCHEDULED con `cancelled_reason`, y
    `event_name` en blanco fallan con `23514` y su `constraint()`; el caso legal
    de cada uno pasa.

- [x] **T3. Triggers y FK de `events`.** RF-13, RF-14, RF-16, RF-17, RF-18
  - Hecho cuando: `event_date` fuera de rango y AUTO fuera de `practice_day`
    fallan (`23514`); `INSERT` en lapso CLOSED falla; `UPDATE` de columna
    estructural o a CANCELLED con asistencia falla y editar `observation` pasa;
    `DELETE` de evento falla siempre (`23514`); `id_lapse` inexistente falla
    (`23503`).

- [x] **T4. Asistencia.** RF-23, RF-24
  - Hecho cuando: repetir el par `(id_event, id_student)` falla (`23505`,
    `pk_attendance`); `DELETE` de evento con asistencia y de estudiante con
    asistencia quedan bloqueados por las FK (`23001`), documentando la doble
    capa con los triggers ya cubiertos.

- [x] **T5. Inmutabilidad de sección.** RF-26
  - Hecho cuando: cambiar `id_section` de un estudiante **con** asistencia
    falla (`23514`, `trg_students_section_immutable`) y **sin** asistencia pasa.

- [x] **T6. Override de nota.** RF-30, RF-31, RF-32, RF-33
  - Hecho cuando: `value` -1 y 21 fallan (`23514`, `grade_overrides_value_range`)
    y 0/20 pasan; `(id_student, id_lapse)` duplicado falla (`23505`); override
    en lapso CLOSED falla (`23514`, `trg_grade_overrides_lapse_open`); FK
    inexistente falla (`23503`) y `DELETE` de lapso/estudiante con override
    queda bloqueado (`23001`).

- [x] **T7. Auditoría y funciones auxiliares.** RF-34, RF-35, RF-36, RF-37
  - Hecho cuando: `UPDATE`/`DELETE` de `audit_logs` fallan (`23514`,
    `audit_logs_append_only`) e `INSERT` pasa; `set_updated_at` pisa
    `updated_at` con `now()` ignorando el valor del cliente; `event_occurred`
    es falso sin asistencia y verdadero con ella; `week_day_of` mapea los siete
    días ISO.

- [x] **T8. Cierre.** criterios de finalización
  - Hecho cuando: `cargo test` verde (suite completa), `cargo clippy
    --all-targets -D warnings` 0/0, `cargo fmt` limpio, spec marcada
    `implementada`, `tasks.md` con todo `[x]` y `MEMORY.md` actualizado.
