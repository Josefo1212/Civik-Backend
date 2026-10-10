# MEMORY.md — Civik Backend

Memoria del proyecto entre sesiones. Máximo ~50 líneas: resume o elimina lo
que ya no aporte. Al convertirse en regla permanente, se mueve a
`AGENTS.md` o a la skill `civik-dominio`, no se queda acá.

## Estado actual

- Proyecto reiniciado desde cero (Node/TS → Rust + hexagonal proporcional).
  `Cargo.toml` y código real (commit `1cd5daa`): `main.rs`, `app.rs`,
  `config/database.rs`, `shared/{error,audit,round}.rs`, `GET /_health`.
  **Fases 0 y 1** de "Orden de construcción" (`AGENTS.md`) completas.
- **Fase 2 — `sections`**, primer uso de SDD. Spec 002 **implementada**
  (27 RF con veredicto en `tasks.md`, T1–T10 hechas; bordes HTTP además
  confirmados con `cargo run` + `curl` — T10).
  migración `20261006010939_sections_active_and_not_blank` (`active`,
  `sections_not_blank`) con 7 tests en `tests/sections_migracion.rs`;
  `sections/domain.rs` con `Grado`/`Turno`, `preparar_entrada` y
  `ErrorDominio`, con 21 tests unitarios sin base de datos; la prueba de
  humo de auditoría + `Conflict` (`src/lib.rs`, `tests/audit.rs`,
  `AppError::Conflict` + `mapear_violacion` + `From<sqlx::Error>` en
  `src/shared/error.rs`, con errores Postgres reales en
  `tests/conflictos_bd.rs`); el CRUD en `sections/queries.rs` +
  `queries/{create,update,get_by_id,set_active,delete,list_active}.sql`:
  `crear`, `editar`, `get_by_id`, `set_active` (idempotente con
  `IS DISTINCT FROM`), `eliminar` (auditoría antes del borrado, 4 tests) y
  `listar_activas` (filtro opcional por grado; orden canónico con
  `array_position($2::text[], grade_level)` + `COALESCE(..., 99)`, el array
  sale de `Grado::TODOS`), todos con auditoría en la misma tx, 19 tests en
  `tests/sections.rs`; `handler.rs` con los 7 endpoints montados en
  `app.rs` (`POST/GET /sections`, `GET/PUT/DELETE /sections/{id}`,
  `POST /sections/{id}/baja|reactivar`) y sus bordes HTTP probados con
  `oneshot` en `tests/http_sections.rs` (14). **78 tests**, clippy **0/0**.
  `_health` ya no miente: responde **503** real cuando la base cae (antes
  `let _ =` y siempre 200), probado con `tower::ServiceExt::oneshot` en
  `tests/health.rs`. **Fase 2 cerrada**.
- **Fase 3 — `students`**, con SDD (spec 003 **implementada**, 19 RF con
  veredicto en `tasks.md`). Incluye la **corrección retroactiva**: la primera
  implementación agregó cédula (`national_id`) que la spec ya había dejado
  fuera; se retiró con la migración `20261007120000_remove_students_national_id`
  y `students` quedó **sin constraints UNIQUE**. T1: `domain.rs`
  (`colapsar_espacios`, `preparar_entrada`, `ErrorEntrada`, 16 unit tests).
  T2: `AppError::NotFound` pasa a `NotFound(String)`;
  `sections/handler.rs` ajustado. T3–T6: `queries.rs` con `ErrorEstudiante`
  (`EstudianteNoExiste`, `SeccionNoExiste`, `SeccionInactiva`, `Sqlx`)
  `esta` derivado con `thiserror` (por el `?` de los tests) + 5 `.sql`
  (`create`, `get_by_id`, `list_by_section`, `update`, `set_active`):
  `crear`, `consultar`, `listar_por_seccion` (orden
  `lower(last_name), lower(first_name), id`, bandera `$2::boolean OR active`),
  `editar` (valida la sección **solo cuando cambia**, RF-12 literal) y
  `set_active` idempotente; 26 tests en `tests/students.rs`. T7: `handler.rs`
  (6 endpoints en `app.rs`; sin `DELETE` → `curl -X DELETE` responde 405,
  RF-19) + 9 tests `oneshot` en `tests/http_students.rs` (commitean, limpian
  con `DELETE` SQL directo). **130 tests**, clippy **0/0**. **Fase 3
  cerrada**.
- **Fase 4 (`lapses`)**: hexagonal parcial (domain+service+queries, 7 rutas HTTP). T1–T8 completos: 23 tests integración (`tests/lapses.rs`), 21 HTTP (`tests/http_lapses.rs`), 6 defensa BD (`tests/conflictos_bd.rs`). **Total +50 tests lapses**, suite completa verde, clippy 0/0. **Fase 4 cerrada**.
- **Fase 5 (`events` CRUD manual)**: spec 005 **implementada** (RF-1 a RF-13).
  `domain.rs` (reglas puras, `preparar_creacion_manual`, `puede_cancelarse`,
  `puede_editarse`), `service.rs` con transacción (crear/editar/cancelar +
  auditoría), `queries/*.sql`, `handler.rs` + 6 rutas HTTP en `app.rs`; 7
  tests en `tests/events.rs`, 9 HTTP en `tests/http_events.rs`, defensa BD en
  `tests/conflictos_bd.rs` (23514 rango, práctica regular manual en
  practice_day). **Fase 5 cerrada**.
- **Fase 6 (`events` auto + reprogramación)**: spec 006 **implementada**
  (RF-14 a RF-20). Decisiones acordadas 2026-10-09: **generador manual que
  cubre todo el rango** (`POST /lapses/{id}/generar-practicas`, idempotente,
  omite cualquier `PRACTICA_REGULAR` existente AUTO o MANUAL) y
  **reprogramar = cambiar `lapses.practice_day`**
  (`POST /lapses/{id}/reprogram-practice-day { "nuevo_dia" }`, mueve
  pendientes al próximo nuevo día, respeta destinos ocupados, `movidos`/
  `no_movidos`). Rutas bajo `/lapses/{id}/...`; auditoría 1 entrada por
  operación (no por evento); no-op con el mismo día sin auditoría falsa.
  11 tests `tests/events_auto.rs` + 5 HTTP `tests/http_events.rs` +
  `dos_auto_el_mismo_dia_emiten_unicidad_23505` en `tests/conflictos_bd.rs`.
  Mapeo `uq_events_auto_one_per_day` (23505) → 409 en `shared/error.rs`.
  **Fase 6 cerrada**; sigue **Fase 7 (`attendance`)**.
- Esquema en `db/migrations/` ya corregido: sin `notes`/`school_term`, sin
  orquestación en PL/pgSQL. Las 10 reglas son constraints/triggers.
- `AGENTS.md` + skill `civik-dominio` documentan arquitectura, stack,
  entornos, testing y la fórmula de notas. Leerlos antes de proponer nada.

## Decisiones (y por qué)

- Una sola base (`civik_dev`), sin `civik_test`: los tests usan transacción +
  rollback, nunca `commit`.
- `DATABASE_URL` unificada: la necesita `sqlx` para verificar consultas en
  compilación, y evita que runtime y compile-time difieran.
- Orquestación de prácticas automáticas en `events::service` (Rust), no en
  PL/pgSQL: `sqlx::Transaction` da la misma atomicidad sin mezclar negocio con
  invariantes.
- Constitución en `docs/constitution.md`, no en la raíz: coincide con la ruta
  que usan el skill `sdd` y los comandos `/sdd-*`.
- `db/schema.sql` **se versiona**: es contra lo que se leen los diffs de
  esquema, no un artefacto local descartable.
- **El peso de una nota depende de `type_event`, no del `source`**: una
  práctica regular **manual** pesa `1`, igual que una AUTO (`PRACTICA_EXTRA`
  → 0.5, `PRESENTACION` → 2).
- **Una práctica regular manual puede crearse con fecha pasada** mientras el
  lapso esté abierto (cubre la primera clase, que se pierde porque la app
  aún no está lista), pero **solo en el `practice_day` del lapso** — mismo
  espíritu de la regla 1, que la base solo exige para `AUTO` y el dominio de
  Rust valida para `MANUAL`. `EXTRA`/`PRESENTACION` no tienen restricción de
  día. La única frontera temporal real es el lapso `CLOSED`.
- **`generar_practicas_auto` omite los `practice_day` que ya tengan una
  práctica regular (AUTO *o* MANUAL)**: cada día de práctica tiene como
  máximo una regular, la cree quien la cree. `uq_events_auto_one_per_day`
  solo protege entre AUTO; esta regla cierra el hueco entre una regular
  creada a mano y la del generador. **Decidido (spec 006, 2026-10-09):** el
  generador es manual, idempotente y cubre todo el rango del lapso; no mira
  el reloj (puede crear fechas pasadas mientras el lapso no esté `CLOSED`).

## Aprendizajes y errores a evitar

- **El host no tiene `pg_dump`** (ni sudo/apt/dpkg) y `dbmate up` descarta el
  volcado **en silencio**; lo compensa un envoltorio `~/.local/bin/pg_dump`
  que delega en el contenedor `pruebapostgres-postgres-1`. **Fuera del repo**:
  sin él, en otra máquina `db/schema.sql` desaparece sin error → verificarlo
  tras cada migración. Alternativa probada y **no instalada**: `pg_dump` 18.6
  del paquete Arch `postgresql-libs` (no `postgresql`, que es solo servidor),
  que funciona sin sudo con `LD_LIBRARY_PATH` a su `libpq.so.5`.
- `dbmate rollback` + `up` no devuelve el esquema byte a byte: las columnas
  re-agregadas quedan al final de la lista.
- `validar_largo` cuenta **caracteres, no bytes**: `varchar(10)` de Postgres
  cuenta caracteres, y medir en bytes rechazaría datos que la base sí acepta.
- **`Value::default()` de serde_json es `Null`, no `{}`**: para bindear
  payloads opcionales a `data jsonb NOT NULL DEFAULT '{}'` hay que construir
  `Value::Object(Map::new())`, no usar `unwrap_or_default()`.
- `rustfmt` dado un archivo con `mod` (p. ej. `src/lib.rs`) reformatea por
  **cascada sus submódulos**: al formatear `lib.rs` quedaron `app.rs` y otros
  formateados sin tocarlos explícitamente. No es un cambio misterioso.
- **sqlx no mapea enums propios en los macros de compilación** (ni como
  parámetro ni en columnas de `RETURNING`; los casts se infieren además como
  nulables). `sections.shift` es el enum `shift_type` y se resuelve así: el
  parámetro se bindea como text y se castea en SQL (`$3::text::shift_type`,
  Postgres revalida el catálogo), el `RETURNING` omite la columna del enum, y
  la fila `Seccion.shift` es `String`. No pelear con `#[sqlx(type_name)]` en
  macros: no termina de funcionar y ensucia el dominio con sqlx.
- **Dentro de una transacción, `now()` es constante**, así que el trigger
  `set_updated_at` pisa `updated_at` con el mismo valor en cada UPDATE: no se
  puede probar "no modifica nada" (RF-19/21) comparando timestamps. La prueba
  honesta es `rows_affected() == 0` sobre el UPDATE en bruto.
- **En un test que usa `connect_lazy`, acotar `acquire_timeout`**: el pool
  espera 30 s por defecto a que la conexión (muerta) se establezca; con 3 s
  el test de `_health` 503 baja de 30 s a instantáneo.
- **`sqlx::query_as` con una sola columna no existe**: para `SELECT ... RETURNING id`
  de un solo `Uuid` hay que usar `query_scalar`, no un `query_as` de un campo.
- **Un `DELETE` contra una FK `ON DELETE RESTRICT` emite `23001`
  (restrict_violation), NO `23503`**: el `23503` es de INSERT (NO ACTION).
  `mapear_violacion` matchea `students_id_section_fkey` con ambos códigos; se
  cazó porque un test **real** de borrado (T6) devolvió 500 donde se esperaba
  409 — los tests unitarios con códigos a mano no alcanzan si no hay un test
  de integración que ejercite la consulta real. Lección: el mapeo de
  conflictos se prueba contra Postgres de verdad, no solo en memoria.
- **Postgres aborta la transacción al primer error** y ningún query posterior
  funciona hasta `ROLLBACK`. Para verificar el estado tras un fallo esperado
  en un test, el intento va dentro de un savepoint. **sqlx 0.8 no expone
  savepoints**: se hacen con SQL directo (`SAVEPOINT x` / `ROLLBACK TO
  SAVEPOINT x`) sobre la misma transacción.
- Un error Postgres también puede dejar `query_scalar` sin deducir el tipo de
  salida: anotar `let id: Uuid = query_scalar(...)` explícitamente.
- **El host no tiene `psql`** (sí `docker`): el contenedor
  `pruebapostgres-postgres-1` es quien lo ofrece a través de
  `docker exec ... psql -U josefo -d civik_dev`.
- **`psql -c` con varias sentencias es una sola transacción**: si la
  segunda falla, la primera se revierte en silencio. Para borrados de
  limpieza conviene un `-c` por sentencia.
- **`updated_at` es `TIMESTAMPTZ`**: para leerlo hay que mapear
  `chrono::DateTime<Utc>`; `NaiveDateTime` falla con "not compatible".
- **Un requisito fuera del alcance se cumple desde la primera implementación**:
  la spec 003 declaró "sin cédula" y aun así la implementación añadió
  `national_id`. La lección es doble: las anotaciones de la spec se respetan
  (y el propio SDD exige que un cambio de requisitos vaya primero a la spec),
  y una columna además de negocio no se agrega solo porque "se pueda" — cada
  campo del esquema tiene un requisito detrás que lo justifique.

## Próximos pasos

- **Fase 7 — `attendance`**: registrar y corregir asistencia (sin
  sincronización todavía); cubre reglas 7 y 10. Depende de `events` y
  `students` ya terminados.
- Los `dead_code` de `shared/*` los limpia el cierre de la fase en curso.

## Bitácora

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

- **Fase 7 (2026-10-09)** — `attendance` registrar/corregir (sin sincronización offline). Módulo hexagonal parcial: `domain`, `queries/*.sql` (list/insert/set_state), `service` (transacción + auditoría), `handler` (POST /events/{id}/attendance, PATCH /attendance/{id}/{student}) montado en `app.rs`. Tests `tests/attendance.rs` (2), `tests/http_attendance.rs` (1). Defensa con triggers BD existente. Suite verde, clippy 0/0. **Fase 7 cerrada.**
- **Fase 8 (2026-10-09)** — `grades` hexagonal completo: `domain/compute_grade` puro (sin f64, rust_decimal), `ports` trait + `adapter_pg` (sqlx), `service`, `handler` GET /lapses/{id}/grades/{student}. Tests unitarios `tests/grades.rs` (3). Suite verde, clippy 0/0. **Fase 8 cerrada.**
- **Fase 9 (2026-10-09)** — Sincronización offline (attendance): revisión de soporte `updated_at`, tests de idempotencia, corrección sin borrar fila, test HTTP. Esquema ya tenía `updated_at`; añadido `tests/http_attendance.rs` (1). **Fase 9 en progreso/T1 completo.** Suite verde (core), clippy 0/0.
