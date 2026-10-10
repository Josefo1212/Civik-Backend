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
  **Fase 6 cerrada**.
- **Fases 7 y 8 quedaron marcadas `implementada` pero incumplían sus specs**
  (SQL inline, `GET` padrón faltante, override/elegibilidad ignorados, rutas
  distintas a la spec). El 2026-10-10 se abrió una **reconciliación** contra el
  formato SDD: las specs 007 y 008 se reescribieron y aprobaron (**Fase A**).
  **Fase 7 reconciliada y cerrada (Fase B)**: upsert real por PK, padrón
  (`GET /events/{id}/attendance`), `domain` con newtypes y
  `puede_escribir_asistencia`, `POST` 200 con padrón, corrección anidada en
  `/events/{id}/attendance/{student}`, defensa BD de reglas 4/7/10.
  **Fase 8 (`grades`) reconciliada y cerrada (Fase C)**: hexagonal completo,
  elegibilidad por fecha/lapso, override respeta dos decimales, endpoint por
  lapso y sección. **Fase 9 (`offline`) implementada y cerrada (T0–T6)**:
  `POST /events/{id_event}/attendance/sync`, `decidir_sincronizacion` pura,
  tolerancia de 5 min, reintentos idempotentes y conflictos por ítem en `200`.
  **Suite completa: 312 tests, clippy 0/0.**
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
- **Los tests comparten `civik_dev` y corren en paralelo**: los que usan
  transacción + rollback no ensucian, pero los HTTP hacen `commit` de verdad.
  Un test nunca debe asumir que la tabla solo tiene SUS filas. Reglas que
  surgieron al chocar: (a) cada archivo usa un **año escolar propio** y su
  limpieza acota por ese rango, no por `>= 2050` a lo bruto; (b) al borrar
  residuo de lapsos, excluir los que tienen eventos (`NOT EXISTS`), porque el
  trigger `lapses_delete_guard` emite `23514`; (c) los listados que asertan
  orden canónico **filtran primero a los ids que el test creó** y recién ahí
  comparan; (d) un test HTTP que commitea debe purgar su residuo al arrancar
  (por si una corrida previa falló a mitad), no solo al terminar.

## Próximos pasos

- **Fase 9 — Sincronización offline** (spec 009): **implementada y cerrada**.
  Dominio puro de conflicto, `sincronizar`/`sincronizar_en_tx`, ruta
  `POST /events/{id_event}/attendance/sync`, tests tx/rollback y HTTP.
- **Fase 10 — Endurecer tests**: portar los 76 casos de
  `verify-constraints.sql` a tests de integración (patrón tx/rollback).
- **Fase 11 — Deploy**: Dockerfile + `compose.yaml`, al final.
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
  de tests de abajo).
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
  conflictos por ítem en **`200`**, ruta **`POST /events/{id_event}/attendance/
  sync`**, y `updated_at` siempre del servidor. `domain.rs`:
  `TOLERANCIA_RELOJ`, `VersionActual`, `DecisionSincronizacion` y
  `decidir_sincronizacion` (pura; la igualdad de estado gana sobre el reloj).
  `service.rs`: `LoteSync`/`ItemSync`/`ItemResultado`, `sincronizar` (abre tx) y
  `sincronizar_en_tx` (núcleo probado dentro de la tx del test; igual que
  `queries.rs` en grades, ver los datos sin commitear). `CANCELLED`/`CLOSED`
  rechazan el lote entero (409); la baja solo impide **crear**, no corrigir.
  Se auditó con contadores y **solo si hubo escrituras** (idempotencia). Tests:
  `offline.rs` (6, tx/rollback, años 2010–2015) y `http_offline.rs` (1, año
  2016/`WOf1`). Hallazgo: `lapses_queries::cerrar` exige pasar por `ACTIVE`
  (si no, no-op silencioso). **312 tests, clippy 0/0.** Fase 9 cerrada.
