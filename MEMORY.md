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
  `tests/health.rs`. **Fase 2 cerrada**; sigue la **Fase 3 (`students`)**
  del orden de construcción.
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

## Próximos pasos

- **Fase 3 — `students`**: CRUD directo, mismo patrón que `sections` (spec
  nueva con el skill `sdd`), dependiente de `sections` (FK).
- Los `dead_code` de `shared/*` y el `#[allow(dead_code)]` de
  `sections/domain.rs` los limpia T9.

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
