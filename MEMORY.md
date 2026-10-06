# MEMORY.md — Civik Backend

Memoria del proyecto entre sesiones. Máximo ~50 líneas: resume o elimina lo
que ya no aporte. Al convertirse en regla permanente, se mueve a
`AGENTS.md` o a la skill `civik-dominio`, no se queda acá.

## Estado actual

- Proyecto reiniciado desde cero (Node/TS → Rust + hexagonal proporcional).
  `Cargo.toml` y código real (commit `1cd5daa`): `main.rs`, `app.rs`,
  `config/database.rs`, `shared/{error,audit,round}.rs`, `GET /_health`.
  **Fases 0 y 1** de "Orden de construcción" (`AGENTS.md`) completas.
- **Fase 2 — `sections`**, primer uso de SDD. Spec 002 **aprobada**
  (`specs/002-sections/`: 27 RF, plan, tasks T1–T10). **T1, T2 y T3 hechas**:
  migración `20261006010939_sections_active_and_not_blank` (`active`,
  `sections_not_blank`) con 7 tests en `tests/sections_migracion.rs`;
  `sections/domain.rs` con `Grado`/`Turno`, `preparar_entrada` y
  `ErrorDominio`, con 21 tests unitarios sin base de datos; y la prueba de
  humo de auditoría + `Conflict`: `src/lib.rs` (crate lib+binary), smoke test
  de `write_audit_log` en `tests/audit.rs` (3 tests), y
  `AppError::Conflict` + `mapear_violacion` + `From<sqlx::Error>` en
  `src/shared/error.rs`, probados con errores Postgres reales en
  `tests/conflictos_bd.rs`. **42 tests**, clippy **0/0**, crate
  `rustfmt`-limpio. Sigue **T4**: CRUD completo de secciones (RF 1–6).
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

## Próximos pasos

- **T4**: CRUD de secciones (RF 1–6): handlers + queries + SQL verificado en
  compilación, con el 409/`Conflict` listo para el duplicado (RF 8).
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
