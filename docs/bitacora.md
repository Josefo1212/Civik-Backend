# Bitácora — Civik Backend

Registro histórico del proyecto, fase por fase. Es el detalle que antes vivía
en `MEMORY.md`; se separó para que `MEMORY.md` quede como estado, decisiones y
aprendizajes vigentes. No se borra: es el rastro de por qué cada cosa es como
es. Orden cronológico.

- **Fase 1 (2026-10-03)** — `shared/audit.rs` y `shared/round.rs`. Base
  recreada (borrada `civik_dev`/`civik_test`, reaplicada la init).
- **Fase 2 (2026-10-06)** — spec 002 (`sections`) aprobada. T1: migración
  `active` + `sections_not_blank`, 7 tests en `tests/sections_migracion.rs`.
  T2: `sections/domain.rs`, 21 tests unitarios, sin base de datos.
- **Fase 2 — T3 (2026-10-06)** — humo de auditoría y `Conflict`: `lib.rs`,
  `tests/audit.rs` (3), `Conflict` + `mapear_violacion` + `From<sqlx::Error>`
  con `tests/conflictos_bd.rs` (3). 42 tests totales, clippy 0/0, fmt limpio.
- **Fase 2 — T4 (2026-10-06)** — alta: `queries.rs` + `create.sql` (`crear`
  sobre `&mut PgTransaction`, auditoría en la misma tx), 4 tests en
  `tests/sections.rs`. 46 tests totales. Enum `shift_type` resuelto con
  `$3::text::shift_type` y `Seccion.shift: String` (aprendizaje sqlx).
- **Fase 2 — T5 + `_health` (2026-10-06)** — edición (`update.sql`, `editar`),
  baja/reactivación (`set_active.sql` idempotente + `get_by_id.sql`),
  auditoría solo cuando el estado cambió de verdad; 7 tests nuevos en
  `tests/sections.rs`. `_health` 503 real con tests `oneshot`
  (`tests/health.rs`, `tower` dev-dep). **55 tests totales**, clippy 0/0.
- **Fase 2 — T6 (2026-10-06)** — `delete.sql` + `eliminar` (auditoría antes
  del borrado), 4 tests en `tests/sections.rs`. **Hallazgo**: el mapeo a
  `Conflict` no funcionaba con el borrado real porque una FK `RESTRICT`
  emite `23001`, no `23503`; `mapear_violacion` cubre ambos y se agregó el
  test de integración real en `tests/conflictos_bd.rs` (4). Savepoints por
  SQL directo (sqlx 0.8 no los expone). **60 tests totales**, clippy 0/0.
- **Fase 2 — T7 (2026-10-06)** — `list_active.sql` + `listar_activas`:
  filtro opcional por grado, omisión de las inactivas y orden canónico
  (`array_position` sobre el array de `Grado::TODOS`, `COALESCE 99`); 4
  tests nuevos en `tests/sections.rs` (orden + RNF-3 estable, filtro por
  grado, get_by_id de inactiva, inexistente). **64 tests totales**, clippy
  0/0.
- **Fase 2 — T8 (2026-10-06)** — `handler.rs` (7 handlers, transacción por
  operación de escritura, `a_not_found` para RowNotFound→404) + rutas en
  `app.rs` + `tower::ServiceExt::oneshot` en `tests/http_sections.rs` (14
  tests HTTP). **Hook**: los tests HTTP hacen `commit` de verdad, así que
  usan subdivisiones únicas por test y limpian con `DELETE` al final; si un
  test falle a media deja una huérfana que puede ensuciar otro listado —
  las aserciones de orden usan posiciones relativas, no el arreglo entero.
  **78 tests totales**, clippy 0/0, base limpia al terminar.
- **Fase 2 — T9 (2026-10-06)** — `#![deny(clippy::unwrap_used,
  clippy::expect_used)]` en `lib.rs` con `#![allow(...)]` solo en
  `main.rs` (y el de `error.rs` para los tests unitarios); se quitó el
  `#![allow(dead_code)]` de `domain.rs`. Clippy 0/0.
- **Fase 2 — T10 (2026-10-06)** — veredicto de los 27 RF en `tasks.md`
  (RF→test→verificación manual), borde HTTP confirmado con `cargo run` +
  `curl` (201/200/204/400/404/409 según el plan §7, JSON `snake_case`
  normalizado), spec 002 con `Estado: implementada`. **Fase 2 cerrada.**
- **Fase 3 — T1/T2 (2026-10-06)** — `students/domain.rs` puro (16 tests) +
  `NotFound(String)`. 95 tests.
- **Fase 3 — T3/T4 (2026-10-06)** — `crear` (verifica sección en la misma
  tx, auditoría "alta"; `ErrorEstudiante` con `thiserror`) y
  `consultar`/`listar_por_seccion`. 102 tests.
- **Fase 3 — T5/T6 (2026-10-06)** — `editar` (valida la sección solo si
  cambia; `students_names_not_blank` se prueba por bypass: 23514) y
  `set_active` idempotente. 119 tests.
- **Fase 3 — T7 (2026-10-06)** — `handler.rs` + rutas (alias
  `estudiantes` en `app.rs` para el clash de nombres) + 9 tests `oneshot`.
  130 tests.
- **Fase 3 — T8 (2026-10-06)** — veredicto de los 22 RF (RF-16 marcado
  parcial hasta la Fase 7; RF-22 por ausencia, `curl -X DELETE` → 405),
  bordes con `cargo run` + `curl` y limpieza SQL directa, spec 003 con
  `Estado: implementada`. **Fase 3 cerrada.**
- **Fase 3 — corrección de la cédula (2026-10-07)** — la spec 003 dijo
  desde la clarificación que "no se guardará cédula" y la implementación la
  agregó de todos modos. Se retiran `national_id` y `uq_students_national_id`
  de `students` (migración `20261007120000_remove_students_national_id`, con
  `db/schema.sql` regenerado); dominio, queries, handler, `mapear_violacion`
  y los tests quedan sin cédula; spec/plan/tareas renumerados a 19 RF sin
  huecos. `students` sin constraints UNIQUE: el único `23505` de la fase es
  `uq_sections_grade_section_shift`. **130 tests**, clippy 0/0, base limpia,
  smoke `curl` sin cédula.
- **Fase 5 (2026-10-09)** — `events` CRUD manual, spec 005 `implementada`
  (RF-1 a RF-13). `domain.rs` puro + `service.rs` (crear/editar/cancelar,
  transacción + auditoría) + `queries/*.sql` + 6 rutas HTTP. Tests:
  `tests/events.rs` (7), `tests/http_events.rs` (9), defensa BD en
  `conflictos_bd.rs`. **Fase 5 cerrada.**
- **Fase 6 (2026-10-09)** — auto + reprogramación, spec 006 `implementada`
  (RF-14 a RF-20). Generador manual idempotente que cubre todo el rango
  (`/lapses/{id}/generar-practicas`); reprogramar cambia `practice_day`
  (`/lapses/{id}/reprogram-practice-day`), mueve pendientes al próximo nuevo
  día y respeta destinos ocupados; auditoría de 1 entrada por operación;
  no-op con el mismo día sin auditoría. 11 tests `tests/events_auto.rs`
  (incluye RF-17 destino ocupado por regular CANCELLED), 5 HTTP +
  23505→409 (mapeo en `shared/error.rs`, test real en `conflictos_bd.rs`).
  Suite completa verde (250 tests), clippy 0/0. **Fase 6 cerrada.**
- **Fase 7 (2026-10-09)** — `attendance` registrar y corregir (sin
  sincronización offline todavía; cubre reglas 7 y 10). spec 007
  **implementada**. Módulo hexagonal parcial (`domain`, `queries/*.sql`
  `list_by_event`/`insert`/`set_state`, `service` con transacción + auditoría,
  `handler`); rutas `POST /events/{id}/attendance` y
  `PATCH /attendance/{id_event}/{id_student}` montadas en `app.rs` con
  `.merge()` (axum 0.8 rechaza `nest("/")`). **Enum `attendance_state`**: los
  macros `query_file_as!` no lo infieren como parámetro → `queries.rs` usa
  `sqlx::query` + `Row::get` y castea `$n::attendance_state`; timestamps como
  `Option<DateTime<Utc>>`. `tests/attendance.rs` (2: idempotencia insert
  duplicado falla, corrección cambia `state` sin borrar fila),
  `tests/http_attendance.rs` (1). Clippy 0/0. **Fase 7 cerrada.**
- **Fase 8 (2026-10-09)** — `grades`, la pieza más sensible, hexagonal
  **completo**: `domain/compute_grade` puro (sin `async`, sin `sqlx`, sin
  `f64`; `rust_decimal::Decimal` para la nota), `ports.rs` (`trait
  GradeRepository`, justifica `async-trait` v0.1.92), `adapter_pg.rs` (sqlx),
  `service.rs` y `handler.rs` (`GET /lapses/{id}/grades/{student}`). Base 20,
  mínimo 0; pesos `PRACTICA_REGULAR=1`, `PRACTICA_EXTRA=0.5`,
  `PRESENTACION=2`; solo `AUSENTE` penaliza; override por (estudiante,
  lapso) prevalece. `tests/grades.rs` (3). Clippy 0/0. **Fase 8 cerrada.**
- **Fase 9 — T0/T1 (2026-10-09)** — spec 009 (`offline`); se confirmó que el
  esquema ya traía `updated_at` en `attendance` y se añadieron los tests base
  de idempotencia/corrección/HTTP. El **protocolo offline real** (detección de
  conflicto por `updated_at`, tolerancia de reloj, contrato idempotente de
  red) **sigue pendiente**: ampliar spec/plan/tasks antes de codificar.
  **Suite completa: 256 tests, clippy 0/0** (con la corrección de aislamiento
  de tests).
- **Fase 9 — aislamiento de tests (2026-10-10)** — la base es única y los
  tests corren en paralelo; al sumar `http_attendance` (commitea de verdad)
  afloraron choques de residuo entre archivos. Se endureció: `http_lapses`
  acota su limpieza a `2050..2090` y excluye lapsos con eventos
  (`NOT EXISTS`); `http_attendance` purga su propio año (2099) y su sección
  (`HAtt`) al arrancar; los listados de `lapses`/`sections` filtran a las
  filas que el propio test creó antes de asertar el orden canónico.
  **256 tests verdes y repetibles** (dos corridas seguidas), clippy 0/0.
- **Reconciliación 007/008 — Fase A (2026-10-10)** — las specs 007 y 008 se
  reescribieron a SDD completo (RF, RNF, casos límite, plan, tasks) y se
  aprobaron; 007 pasó de `implementada` a `aprobada` con nota de
  reconciliación. Se resolvieron dudas de 007: padrón **global**, registro
  **en lote**, baja con fila corregible.
- **Fase B — `attendance` reconciliada (2026-10-10)** — B1–B7. `domain.rs` con
  newtypes `EventId`/`StudentId`, `AttendanceState` (`parse`/`as_str`),
  `ErrorAsistencia` y `puede_escribir_asistencia`; `queries.rs` reescrito con
  `upsert`/`set_state`/`get`/`rows_by_event`/`padron` (se eliminan
  `insert`/`list_by_event`); `service.rs` (`RegistroLote`, `registrar_listado`,
  `listar_padron`, `corregir_asistencia`); `handler.rs` con `POST`/`GET
  /events/{id}/attendance` y `PATCH .../{student}` (ruta corregida). Hallazgo:
  `check_attendance_guards` corre en `INSERT` **y** `UPDATE`, así que el lapso
  `CLOSED` también bloquea la corrección → el service pre-chequea ambos.
  `state::text` hace que sqlx infiera `Option<String>` (igual que
  `events.status`). Tests: `tests/attendance.rs` (6), `http_attendance.rs` (1
  secuencial), `conflictos_bd.rs` +4 (reglas 4/7/10). **275 tests, clippy
  0/0.** Fase B cerrada (B8).
- **Fase C — `grades` reconciliada (2026-10-10)** — T1–T9. `domain.rs` reescrito
  (`Conteos` con `n_eventos_elegibles`, `Nota { SinNota, Valor }`,
  `compute_grade`, `resolver_nota`; `rescale` fija 1 decimal en el cálculo y 2
  en el override). `ports.rs` + `adapter_pg.rs` (delega) + `queries.rs` nuevo
  (funciones sobre `PgExecutor` para poder probarlas dentro de la transacción;
  el adaptador atado al pool no vería datos sin commitear). SQL con
  elegibilidad `SCHEDULED` + (`event_date <= CURRENT_DATE` salvo lapso `CLOSED`)
  y solo `AUSENTE`; `roster.sql` con override como **subconsulta escalar**
  (sqlx no deduce nulabilidad de un `LEFT JOIN`). `service.rs`
  (`notas_de_seccion`, `formatear_nota`); `handler.rs` con
  `GET /lapses/{id}/sections/{id}/grades` (se retira el endpoint por
  estudiante). Tests: `grades.rs` (11: dominio + repo falso + integración
  tx/rollback), `http_grades.rs` (1). Aprendizajes: `query_file!` genera struct
  con campos nombrados (`fila.campo`, no `.get`); agregados inferidos
  `Option`; `Decimal::rescale` muta en el lugar. **298 tests, clippy 0/0.** Fase
  C cerrada.
- **Fase 9 — `offline` implementada (2026-10-10)** — T0–T6. Spec 009 ampliada a
  SDD completo y cerrada la clarificación: tolerancia de reloj **5 min**
  (constante), aplicar no-conflictivos en **una** transacción y devolver
  conflictos por ítem en **`200`**, ruta
  **`POST /events/{id_event}/attendance/sync`**, y `updated_at` siempre del
  servidor. `domain.rs`: `TOLERANCIA_RELOJ`, `VersionActual`,
  `DecisionSincronizacion` y `decidir_sincronizacion` (pura; la igualdad de
  estado gana sobre el reloj). `service.rs`: `LoteSync`/`ItemSync`/
  `ItemResultado`, `sincronizar` (abre tx) y `sincronizar_en_tx` (núcleo
  probado dentro de la tx del test; igual que `queries.rs` en grades, ver los
  datos sin commitear). `CANCELLED`/`CLOSED` rechazan el lote entero (409); la
  baja solo impide **crear**, no corregir. Se auditó con contadores y **solo si
  hubo escrituras** (idempotencia). Tests: `offline.rs` (6, tx/rollback, años
  2010–2015) y `http_offline.rs` (1, año 2016/`WOf1`). Hallazgo:
  `lapses_queries::cerrar` exige pasar por `ACTIVE` (si no, no-op silencioso).
  **312 tests, clippy 0/0.** Fase 9 cerrada.
- **Fase 10 — endurecer tests (2026-10-10)** — T1–T8. Spec 010 aprobada y
  cerrada. `verify-constraints.sql` original (proyecto Node) no se versionó:
  la matriz se reconstruyó desde `db/schema.sql`. Tests nuevos en
  `tests/constraints.rs` (22 casos, tx/rollback con `SAVEPOINT`/`ROLLBACK TO
  SAVEPOINT` y helper `sqlstate`), años `4xxx` y secciones `Z*` para no chocar
  con el EXCLUDE global de `lapses_no_overlap`. Cubre CHECK de `lapses` y
  `events`, triggers de `events` (fecha/lapso abierto/congelación por
  evidencia/no borrado), FK de `events` y `attendance`, inmutabilidad de
  sección, `grade_overrides` (rango 0–20, único por estudiante/lapso, lapso
  abierto, FKs), `audit_logs` append-only, `set_updated_at`, `event_occurred`
  y `week_day_of`. Aprendizajes: un `RAISE EXCEPTION` de trigger deja
  `constraint()` en `None` (solo el SQLSTATE); el borrado del lapso con
  override lo aborta `lapses_delete_guard` (23514) antes que la FK; el host
  **sí** tiene `psql`/`pg_isready` (Postgres nativo en `:5432`, no contenedor).
  **334 tests, clippy 0/0, fmt limpio.** Fase 10 cerrada.
- **Spec 011 — endpoints del front (2026-10-11)** — T1–T5. Cierra la brecha que
  pedía la PWA: `GET /students` (listado global, con `incluir_inactivas`) y
  `GET /sections?incluir_inactivas`, en la misma forma y orden canónico que el
  padrón por sección. `students::queries::listar_todos` + `list_all.sql`;
  `sections::queries::listar_activas` → `listar` (con bandera) y
  `list_active.sql` → `list.sql`, con `($n::boolean OR active)` para no
  duplicar el `ORDER BY` canónico. El parseo del booleano se centralizó en
  `shared::filtro::leer_incluir_inactivas` (reusado por los tres listados), y
  se extrajo de `students::handler::listar`. Aprendizaje: `shared/` gana un
  cuarto archivo cuando hay un uso real compartido (no por anticipado).
  **339 tests, clippy 0/0, fmt limpio.** Spec 011 cerrada.
