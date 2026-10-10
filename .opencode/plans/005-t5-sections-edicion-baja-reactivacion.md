# Plan — Fix _health + T5 (edición, baja y reactivación de secciones)

Auditoría aprobada. Pasos en orden, con verificación al final de cada bloque.

## 1. Fix hallazgo #1: `_health` no miente

**`src/app.rs`**: `health_check` hace `match` sobre el resultado del `SELECT 1`:
`Ok(_)` → 200, `Err(e)` → `tracing::warn!` + `503 SERVICE_UNAVAILABLE`.
(Reemplaza el `let _ = ...` que siempre devolvía 200.)

**`Cargo.toml`**: agregar `[dev-dependencies] tower = "0.5"` (ya está en
`Cargo.lock` vía axum; el plan §8.6 lo prevé y no agrega ninguna dependencia
nueva).

**`tests/health.rs`** (nuevo): `mod common;`
- `health_con_la_base_en_pie_responde_200`: pool de `common::pool()` +
  `build_router` + `tower::ServiceExt::oneshot` sobre `GET /_health` → 200.
- `health_sin_base_responde_503`: `PgPoolOptions::connect_lazy` a
  `postgres://civik:civik@127.0.0.1:1/civik_dev` (puerto muerto, falla rápido
  por connection refused) → 503.

Verificación: `cargo test --test health` en verde; `cargo clippy` 0 warnings.

## 2. T5 — edición (`update.sql` + `editar`)

**`queries/update.sql`** (nuevo): `UPDATE sections SET grade_level=$2,
section=$3, shift=$4::text::shift_type WHERE id=$1 RETURNING id, grade_level,
section, active, created_at, updated_at`. `$4` casteado igual que `create.sql`;
RETURNING sin el enum (misma inferencia nulable de T4). 0 filas → RowNotFound.
23505 sube y `AppError::from` lo traduce a Conflict (RF-17).

**`queries.rs`**: `editar(&mut PgTransaction, Uuid, DatosSeccion) ->
sqlx::Result<Seccion>` usando `FilaBase` (6 cols) + rearma `shift` desde
`datos.turno`; auditoría `action = "editar"` con payload `{grado, seccion,
turno}` en la misma tx (RF-27). `datos` llega ya de `preparar_entrada` (misma
normalización que el alta, RF-14).

## 3. T5 — baja/reactivación (`set_active.sql` + `get_by_id.sql`)

**`queries/set_active.sql`** (nuevo): `UPDATE sections SET active=$2 WHERE
id=$1 AND active IS DISTINCT FROM $2 RETURNING id, grade_level, section,
shift::text AS shift, active, created_at, updated_at`. `IS DISTINCT FROM` = la
operación idempotente real (RF-19/21): la fila no se toca, el trigger
`set_updated_at` no dispara, `updated_at` no se mueve.

**`queries/get_by_id.sql`** (nuevo): `SELECT ... shift::text AS shift ... WHERE
id=$1`. Desambigua "no existe" (`RowNotFound`) de "ya estaba así" (fila leída
sin cambios).

**`queries.rs`**:
- `struct FilaConShift` (7 cols, `shift: Option<String>` — el cast se infiere
  nulable aunque la columna sea `NOT NULL`; `.en_seccion()` lo normaliza con
  `unwrap_or_default()` y documenta el porqué).
- `get_by_id<E: PgExecutor>` público: `fetch_optional` → `RowNotFound` si no
  existe. Una sola consulta, por eso el executor genérico es válido acá.
- `set_active(&mut PgTransaction, Uuid, bool) -> sqlx::Result<Seccion>`:
  si el UPDATE devuelve fila → cambio real, auditoría `action = "baja"`/`"reactivar"`
  con payload `{"activo": bool}` (RF-27) y devuelve la fila. Si devuelve 0
  filas → delega en `get_by_id`: `RowNotFound` si no existe, o la fila tal cual
  si ya estaba en el estado pedido (**sin** auditoría: RF-19/21 dicen "no
  modifica nada", no se registra una operación que no ocurrió).

## 4. T5 — tests (tests/sections.rs, al final del archivo)

Usa `mod common`, `crear`, `preparar_entrada`, `editar`, `set_active`,
`get_by_id`, `Seccion`, `AppError`, `sqlx::Row`/`query_scalar`, `Uuid`.

1. `la_edicion_normaliza_igual_que_el_alta` (RF-14): crear `1ro/a/MATUTINO`,
   editar `" 2DO "/"  b "/"vespertino"` → `2do/b/VESPERTINO`, contraste con
   `SELECT` de la fila real + auditoría `action='editar'` (RF-27).
2. `la_edicion_a_una_combinacion_ajena_se_rechaza` (RF-17): crear `1ro/a` y
   `1ro/b`; editar la b a `1ro/a` → `23505` sobre
   `uq_sections_grade_section_shift`, `AppError::Conflict`, mensaje "uso".
3. `operar_sobre_una_seccion_inexistente_falla`: `set_active` y `editar` con
   `Uuid::new_v4()` → `sqlx::Error::RowNotFound` (RF-13 + T8 lo mapea a 404).
4. `la_baja_marca_inactiva_y_es_idempotente` (RF-18/19): tras la baja,
   `set_active(false)` de nuevo devuelve Ok con la misma fila; el UPDATE en
   bruto reporta `rows_affected() == 0` (la prueba honesta de "no modifica
   nada", porque en una misma transacción el trigger pisa `updated_at` con el
   mismo `now()` y no distinguiría) y solo hay 1 entrada `action='baja'`.
5. `la_reactivacion_marca_activa_de_nuevo_y_es_idempotente` (RF-20/21):
   idéntico a 4 con `true`, 1 sola entrada `action='reactivar'`.
6. `editar_y_dar_de_baja_una_seccion_con_estudiantes_los_deja_en_ella`
   (RF-25/26): INSERT directo en `students` (activo e inactivo, porque el
   módulo students es la Fase 3), luego `editar` + `set_active(false)`;
   los 2 alumnos siguen apuntando a `id_section`.

Nota: "editar a vacío/grado/turno inválidos" (RF-15/16) ya está cubierto en los
tests de `domain::preparar_entrada`, que es el punto único que el handler y la
edición comparten con el alta; se documenta en un comentario del test 2.

## Verificación

`cargo test` completo en verde (46 + 2 health + ~6 T5), `cargo clippy` 0/0,
`rustfmt` limpio, marcar T5 `[x]` en `tasks.md`, actualizar `MEMORY.md`, y un
commit por tarea: `Feat:T5 secciones - edicion, baja y reactivacion
idempotentes` (el fix del `_health` entra en el mismo commit si se cierra junto,
o en `Feat:_health 503` — decisión al cerrar).