# Spec 010 — Endurecer tests (invariantes de base)

Estado: implementada

> Avance: T1–T8 hechos. Matriz de invariantes en `tests/constraints.rs` (22
> tests, patrón tx/rollback con `SAVEPOINT`), cubriendo RF-1, RF-2, RF-9 a
> RF-14, RF-16 a RF-18, RF-23, RF-24, RF-26 y RF-30 a RF-37; los RF ya
> cubiertos (RF-3 a RF-8, RF-15, RF-19 a RF-22, RF-25, RF-27 a RF-29) se
> referencian en la cabecera del archivo. Suite completa **334 tests** en
> verde; `cargo clippy --all-targets -- -D warnings` 0/0; `cargo fmt --check`
> limpio.

## Contexto y objetivo

Las 10 reglas de dominio viven en la base como `CHECK`, `UNIQUE`, `EXCLUDE`,
`FK ... RESTRICT` y triggers (`db/schema.sql`): son invariantes que deben
sobrevivir a un bug del backend, y la única forma de confiar en ellas es
probarlas contra Postgres real. Hoy hay cobertura repartida entre varios
archivos (`lapses.rs`, `sections.rs`, `students.rs`, `conflictos_bd.rs`), pero
hay huecos: varios `CHECK` de `events` y `grade_overrides`, el borrado de
eventos (regla 8), la congelación por evidencia (regla 9), la inmutabilidad de
sección (regla 3), `audit_logs` append-only (regla 6) y varias FK `RESTRICT`
nunca se ejercitan.

El objetivo es **cerrar el hueco**: una matriz completa de casos que fije cada
invariante del esquema con un test de integración (patrón
transacción/rollback), de modo que **romper una restricción o un trigger haga
fallar la suite**. Se reconstruye desde `db/schema.sql` porque el
`verify-constraints.sql` original (proyecto Node anterior) no se versionó y no
está disponible.

## Usuarios

- Desarrollador (mantiene el backend). No es una historia de usuario final:
  esta fase protege el trabajo de todas las demás.

## Historias de usuario

- HU-1. Como desarrollador, quiero que cada invariante de la base tenga un
  test que falle si la restricción o el trigger desaparece, para no depender
  de la disciplina al tocar el esquema.
- HU-2. Como desarrollador, quiero que los tests de invariantes estén en un
  solo lugar reconocible, para no duplicar cobertura ni inventar casos al
  agregar una regla.

## Definiciones

- **Invariante**: restricción del esquema (`CHECK`, `UNIQUE`, `EXCLUDE`, `FK`),
  trigger de guarda o función (`assert_lapse_is_open`, `event_occurred`).
- **Caso**: un test de integración que opera contra `civik_dev` dentro de una
  transacción que termina en `rollback`, y que verifica el efecto
  (`Ok`/`Err`, código SQLSTATE y nombre de restricción cuando aplica).
- **Prueba de sensibilidad**: cada caso negativo (debe fallar) se acompaña del
  caso **positivo adyacente** (la operación legal más próxima). Así el test
  prueba que la guarda dispara **solo** ante la violación: si el trigger/CHECK
  desapareciera, el caso negativo pasaría a `Ok` y el assert fallaría — la
  suite es sensible a la ausencia de la invariante.

## Requisitos funcionales

Cada RF fija una invariante. CUANDO se ejecuta la operación indicada, EL
SISTEMA (la base) responde con el error señalado; la operación legal adyacente
responde `Ok` (sensibilidad).

### Lapso (`lapses`)

- RF-1. `end_date < start_date` viola `lapses_dates_order` (`23514`); con
  `end_date = start_date` pasa.
- RF-2. `lapse_number` fuera de `1..=3` viola
  `lapses_number_between_1_and_3` (`23514`); `1` y `3` pasan.
- RF-3. Repetir `(school_year, lapse_number)` viola
  `uq_lapses_year_number` (`23505`).
- RF-4. Dos rangos que se solapan violan `lapses_no_overlap` (`23P01`); rangos
  que solo se tocan en el borde (`end_date + 1 día = start_date`) pasan.
- RF-5. Un segundo lapso `ACTIVE` viola `uq_lapses_single_active` (`23505`).
- RF-6. `UPDATE` sobre un lapso `CLOSED` viola `trg_lapses_closed_frozen`
  (`23514`).
- RF-7. `DELETE` de un lapso `CLOSED`, terminado (`end_date < CURRENT_DATE`), o
  con eventos y/o `grade_overrides` viola `lapses_delete_guard` (`23514`); un
  lapso `PLANNED`, futuro y vacío se puede borrar (excepción de la
  constitución nº 3).
- RF-8. `UPDATE` de fechas que dejaría eventos fuera del lapso viola
  `trg_lapses_dates_against_events` (`23514`); fechas que siguen cubriendo
  todos los eventos pasan.

### Evento (`events`)

- RF-9. `source = 'AUTO'` con `type_event <> 'PRACTICA_REGULAR'` viola
  `events_auto_is_regular_practice` (`23514`).
- RF-10. `status = 'CANCELLED'` sin `cancelled_reason` o con motivo en blanco
  viola `events_cancelled_requires_reason` (`23514`).
- RF-11. `status = 'SCHEDULED'` con `cancelled_reason` no nulo viola
  `events_scheduled_has_no_reason` (`23514`).
- RF-12. `event_name` en blanco viola `events_name_not_blank` (`23514`).
- RF-13. `event_date` fuera de `[start_date, end_date]` del lapso viola
  `trg_events_date_within_lapse` (`23514`); un `AUTO` que no cae en el
  `practice_day` también (`23514`); las fechas de borde pasan.
- RF-14. `INSERT` de un evento en un lapso `CLOSED` viola
  `trg_events_lapse_open` (`23514`).
- RF-15. Dos eventos `AUTO` el mismo día violan `uq_events_auto_one_per_day`
  (`23505`).
- RF-16. `UPDATE` de una columna estructural (`event_date`, `id_lapse`,
  `type_event`, `source`) o pasar a `CANCELLED` cuando el evento ya ocurrió
  (tiene asistencia) viola `trg_events_frozen_by_attendance` (`23514`, regla 9);
  editar una columna no estructural (`observation`, `event_name`) pasa.
- RF-17. `DELETE` de un evento viola `trg_events_no_delete` (`23514`) siempre,
  haya ocurrido o no (regla 8).
- RF-18. Un evento con `id_lapse` inexistente viola `events_id_lapse_fkey`
  (`23503`).

### Asistencia (`attendance`)

- RF-19. Registrar asistencia en un evento `CANCELLED` viola
  `check_attendance_guards` (`23514`, regla 7).
- RF-20. Registrar asistencia en un lapso `CLOSED` viola
  `assert_lapse_is_open` vía `check_attendance_guards` (`23514`, regla 4).
- RF-21. `DELETE` de una fila de asistencia viola `attendance_no_delete`
  (`23514`, regla 10).
- RF-22. `UPDATE` de `id_event` o `id_student` viola
  `attendance_identity_immutable` (`23514`, regla 10); corregir `state` pasa y
  actualiza `updated_at` sin tocar `created_at`.
- RF-23. Repetir el par `(id_event, id_student)` viola `pk_attendance`
  (`23505`).
- RF-24. `DELETE` de un evento con asistencia y de un estudiante con asistencia
  quedan bloqueados por `attendance_id_event_fkey` /
  `attendance_id_student_fkey` (`RESTRICT`, `23001`), como segunda capa tras
  los triggers correspondientes (sensibilidad de la política "nada se borra").

### Estudiante (`students`) y sección (`sections`)

- RF-25. `first_name` o `last_name` en blanco viola `students_names_not_blank`
  (`23514`).
- RF-26. `UPDATE` de `id_section` de un estudiante **con** asistencia viola
  `trg_students_section_immutable` (`23514`, regla 3); sin asistencia pasa.
- RF-27. `DELETE` de una sección con estudiantes viola
  `students_id_section_fkey` (`RESTRICT`, `23001`); una sección vacía se puede
  borrar.
- RF-28. `grade_level` o `section` en blanco violan `sections_not_blank`
  (`23514`).
- RF-29. Repetir `(grade_level, section, shift)` viola
  `uq_sections_grade_section_shift` (`23505`).

### Override de nota (`grade_overrides`)

- RF-30. `value` fuera de `0..=20` viola `grade_overrides_value_range`
  (`23514`); `0` y `20` pasan.
- RF-31. Repetir `(id_student, id_lapse)` viola
  `uq_grade_overrides_student_lapse` (`23505`).
- RF-32. `INSERT` de un override en un lapso `CLOSED` viola
  `trg_grade_overrides_lapse_open` (`23514`).
- RF-33. Un override con `id_lapse` o `id_student` inexistente viola
  `grade_overrides_id_lapse_fkey` / `grade_overrides_id_student_fkey`
  (`23503`); el `DELETE` de un lapso o estudiante con override queda bloqueado
  por esas mismas FK (`RESTRICT`, `23001`).

### Auditoría y funciones auxiliares

- RF-34. `UPDATE` o `DELETE` sobre `audit_logs` violan
  `audit_logs_append_only` (`23514`, regla 6); `INSERT` pasa (append).
- RF-35. `set_updated_at` (triggers `trg_*_updated_at`): un `UPDATE` fija
  `updated_at = now()` del servidor, ignorando cualquier valor enviado por el
  cliente.
- RF-36. `event_occurred(id)` es `false` sin filas de asistencia y `true` con
  al menos una (regla 5: "ocurrió" por evidencia, no por fecha).
- RF-37. `week_day_of(date)` mapea las siete fechas de una semana ISO a su
  `week_day` correcto.

## Requisitos no funcionales

- RNF-1. Todos los casos usan el patrón transacción/rollback (constitución
  nº 6); ninguna prueba hace `commit`.
- RNF-2. Cuando el caso espera un error, aserta **código SQLSTATE** y, si hay
  una restricción con nombre, `constraint()` — no solo `is_err()`.
- RNF-3. Unique/relationships que el backend ya cubre por dominio no se
  duplican a nivel HTTP: esta fase prueba **la base**, con SQL directo donde el
  camino de la app lo impediría antes de llegar a Postgres.
- RNF-4. Aislamiento de tests: años/secciones exclusivos de los archivos
  nuevos (ver `MEMORY.md` y skill `testing-transaccional`); limpieza acotada.
- RNF-5. Sin `unwrap`/`expect` fuera del arranque salvo el
  `#[allow(clippy::unwrap_used)]` de tests.
- RNF-6. Mensajes y nombres en español; SQLSTATE citado en el nombre o el
  comentario del test.

## Casos límite

- Cada caso negativo lleva su **positivo adyacente** (sensibilidad): borde de
  rango de fechas, `lapse_number` `1`/`3`, `value` `0`/`20`, sección vacía
  borrable, estudiante sin asistencia que sí puede cambiar de sección.
- `FK RESTRICT` (`23001`) vs `INSERT` con referencia inexistente (`23503`): el
  mismo `constraint()` con códigos distintos según la operación.
- Doble capa: donde un trigger y una FK protegen lo mismo (`DELETE` de
  evento/lapso con asistencia/eventos), el test documenta cuál dispara primero
  y por qué el otro es la red de seguridad.
- Dentro de una transacción `now()` es constante: la sensibilidad de
  `set_updated_at` no se prueba comparando timestamps entre dos `UPDATE`, sino
  contra el valor enviado por el cliente.

## Fuera de alcance

- Reescritura de tests existentes que ya cubren bien una invariante: si el caso
  ya está en `lapses.rs`/`sections.rs`/`students.rs`/`conflictos_bd.rs` con
  aserto de código y restricción, no se duplica (se referencia).
- Reglas de negocio de Rust ya probadas (`compute_grade`, reprogramación,
  sincronización offline): viven en tests de sus fases, no acá.
- Migraciones y forma del `db/schema.sql`.
- Índices no funcionales (performance): solo los `UNIQUE` que son invariantes.

## Criterios de finalización

- RF-1 a RF-37 verificados por al menos un caso cada uno; la matriz cubre la
  totalidad del esquema (12 `CHECK`, 5 `UNIQUE` incluidas 2 parciales, 1
  `EXCLUDE`, 6 `FK`, 21 triggers).
- Los huecos detectados (RF-9 a RF-17, RF-24, RF-26, RF-32 a RF-37) tienen
  tests nuevos; lo ya cubierto se referencia.
- `cargo test` verde y `cargo clippy --all-targets -D warnings` 0/0.
- Spec marcada `implementada`, `MEMORY.md` actualizado y commit de cierre.

## Dudas abiertas

Resueltas en la clarificación del 2026-10-10:

- **Dónde viven los tests: archivo nuevo `tests/constraints.rs`.** Un solo
  archivo ordenado por tabla. `conflictos_bd.rs` queda intacto (no se reescriben
  pruebas que ya pasan); los casos ya cubiertos se referencian, no se duplican.
- **"Prueba de sensibilidad" = par negativo + positivo adyacente** (ver
  Definiciones). No se agrega un chequeo automático de esquema↔tests.
