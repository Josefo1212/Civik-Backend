# Plan 007 — Attendance (registrar, corregir y padrón)

## Nivel de hexagonalidad

Hexagonal **parcial** (mismo patrón que `events`): `domain.rs` puro (sin `sqlx`,
sin `async`), `service.rs` orquesta con `sqlx::Transaction`, `queries.rs` +
`queries/*.sql` verificado en compilación, `handler.rs` HTTP. **Sin traits**: un
solo backend y una sola base para siempre; el desacople formal solo se paga en
`grades` (AGENTS.md).

## Archivos

| Archivo | Responsabilidad |
|---|---|
| `src/modules/attendance/domain.rs` | `AttendanceState` (parse/`as_str`), newtypes `EventId`/`StudentId`, validación de `state` y de si el evento/lapso admiten escritura. Puro. |
| `src/modules/attendance/queries.rs` | Una función por `.sql`; mapea `attendance_state` como texto (no enum de sqlx). |
| `src/modules/attendance/queries/upsert.sql` | Alta/actualización en lote por PK `(id_event,id_student)`. |
| `src/modules/attendance/queries/set_state.sql` | `UPDATE` del estado que devuelve la fila (la idempotencia la decide el service leyendo antes). |
| `src/modules/attendance/queries/get.sql` | Una fila por par (evento, estudiante), o nada. |
| `src/modules/attendance/queries/rows_by_event.sql` | Filas de asistencia de un evento (para el padrón y para comparar). |
| `src/modules/attendance/queries/padron.sql` | Estudiantes activos **∪** con fila en el evento, con `state` o `null`, en orden canónico. |
| `src/modules/attendance/service.rs` | `registrar_listado`, `corregir_asistencia`, `listar_padron`; una transacción por operación; auditoría única. |
| `src/modules/attendance/handler.rs` | Rutas y mapeo a códigos HTTP. |
| `src/app.rs` | Monta el router (`.merge`). |
| `tests/attendance.rs` | Integración tx/rollback de los RF de dominio. |
| `tests/http_attendance.rs` | Bordes HTTP con `oneshot`. |
| `tests/conflictos_bd.rs` | Defensa de base: reglas 7 y 10 con SQL directo. |

## Queries `.sql` (una por consulta, `query_file!`)

- **`upsert.sql`** — `INSERT ... ON CONFLICT (id_event,id_student) DO UPDATE SET
  state = EXCLUDED.state, updated_at = now() RETURNING ...`. Es lo que hace real
  el "crea las filas faltantes y actualiza las existentes" (RF-1) y lo que vuelve
  idempotente el reenvío de un lote (base de la fase 9). Reemplaza el `INSERT`
  plano actual, que fallaba con `23505` al re-registrar.
- **`set_state.sql`** — `UPDATE attendance SET state = $3::attendance_state,
  updated_at = now() WHERE id_event = $1 AND id_student = $2 RETURNING ...`. El
  service lee antes con `get`: si no hay fila es `404` (RF-9); si el estado es el
  mismo, responde la fila sin escribir ni auditar (RF-10); si cambió, actualiza.
- **`get.sql`** — `SELECT ... FROM attendance WHERE id_event = $1 AND id_student
  = $2`; distingue "no existe" de "ya estaba" y da el estado anterior para la
  auditoría.
- **`rows_by_event.sql`** — `SELECT id_student, state::text, ... FROM attendance
  WHERE id_event = $1`.
- **`padron.sql`** — RF-13: `(estudiantes activos de la base) UNION (estudiantes
  con fila en el evento)`, `LEFT JOIN attendance`, orden `lower(last_name),
  lower(first_name), id`, con `state` o `NULL`.

El enum `attendance_state` se sigue leyendo/escribiendo **como texto** y con
`$n::attendance_state` en el `CAST` (RNF-3): los macros de sqlx no infieren el
enum propio (`MEMORY.md`).

## Funciones puras de dominio

- `AttendanceState::parse(&str) -> Option<AttendanceState>` y `as_str` — RF-5.
- `validar_state(&str) -> Result<AttendanceState, ErrorAsistencia>` — RF-5.
- `puede_escribir_asistencia(estado_evento, estado_lapso) -> Result<(),
  ErrorAsistencia>` — RF-2 y RF-3 (evento `CANCELLED` y lapso `CLOSED`), tanto
  para el alta como para la corrección.
- Newtypes `EventId(Uuid)` / `StudentId(Uuid)` (RNF-2): `attendance` combina dos
  tipos de id en las mismas firmas; el compilador evita pasar `id_student` donde
  va `id_event`.
- Reutiliza `lapses::domain::EstadoLapso` y `events::domain::EstadoEvento`.

## Service

- **`registrar_listado(pool, EventId, Lote) -> Padron`** (RF-1..RF-7, RF-15):
  1. Abrir transacción.
  2. Cargar el evento (`events::queries::get_by_id`); inexistente → `404` (RF-14).
  3. Cargar el lapso del evento; `CANCELLED` → `409` (RF-2), `CLOSED` → `409` (RF-3).
  4. Validar cada `state` (RF-5, `400`) y cada `id_student` (RF-4, `404` **antes**
     de escribir nada).
  5. Estudiantes inactivos **sin** fila previa → `409` (RF-6); con fila, se permite.
  6. `upsert` cada par **en la misma transacción** (RF-7).
  7. Una sola entrada de auditoría `registrar_asistencia` con el resumen
     creadas/actualizadas (RF-15).
  8. `commit` y devolver el **padrón** (RF-1).
- **`corregir_asistencia(pool, EventId, StudentId, &str) -> Attendance`**
  (RF-8..RF-12): valida `state`, carga el evento (404), rechaza `CANCELLED`
  (409), llama `set_state`; sin fila → `404` (RF-9); sin cambio real → devuelve
  la fila actual y **no** audita (RF-10); con cambio → auditoría `corregir_asistencia`
  con estado anterior/nuevo y `commit` (RF-15).
- **`listar_padron(pool, EventId) -> Padron`** (RF-13): evento inexistente → 404;
  si no, `padron.sql`.

La escritura **no mira el reloj** (RF-16): solo importan el estado del evento y
el del lapso.

## Handler y rutas (alineadas a la spec)

- `POST /events/{id_event}/attendance` → **200** con el padrón (la spec RF-1 dice
  200; el código actual devolvía 201 vacío).
- `GET /events/{id_event}/attendance` → 200 con el padrón (RF-13/RF-14).
- `PATCH /events/{id_event}/attendance/{id_student}` → 200 con la fila (RF-8).
  **Cambio de ruta:** hoy la corrección vive en `/attendance/{id_event}/{id_student}`;
  la spec dice `/events/{id}/attendance/{student_id}`, así que se mueve y se
  actualiza `tests/http_attendance.rs` (anidar la corrección bajo el evento deja
  el recurso coherente y coincide con la spec).

## Decisiones técnicas (con alternativa descartada)

- **Upsert por `ON CONFLICT`** en vez de "leer y decidir" en Rust: la PK manda y
  la base resuelve la carrera; el upsert es atómico y deja el reenvío idempotente
  (necesario para la fase 9). Descartado: `SELECT` + `INSERT`/`UPDATE` desde Rust,
  que tiene una ventana de carrera y más idas a la base.
- **Corrección idempotente leyendo antes** (`get` + comparación en Rust): da el
  `404` (RF-9), el "sin cambio" (RF-10) y el estado anterior para la auditoría
  (RF-15) en un solo lugar y sin una sentencia con `IS DISTINCT FROM` cuyo `None`
  habría que desenredar después.
- **Padrón como SQL, no en Rust**: `UNION` de activos con quienes tienen fila y
  `ORDER BY` canónico; es una consulta, no orquestación.
- **Newtypes solo en `attendance`**: es el módulo que mezcla `id_event` e
  `id_student` (AGENTS.md); en `sections` siguen siendo `Uuid` sueltos.

## Estrategia de tests

- **Dominio puro** (sin base): `domain.rs` con tests unitarios en `src` (parse de
  estados, validaciones, `puede_escribir_asistencia`).
- **Integración tx/rollback** (`tests/attendance.rs`): upsert idempotente, alta
  en lote atómica, corrección sin borrar, `state` inválido, estudiante inactivo,
  lapso cerrado. Todo con `pool.begin()` + `rollback`.
- **HTTP `oneshot`** (`tests/http_attendance.rs`): 200 del padrón, 404 de evento
  y estudiante, 400 de estado, 409 de `CANCELLED`/`CLOSED`/inactivo, 405 en
  `DELETE` (no existe borrado). Como hacen `commit`, el test purga su residuo al
  arrancar (año y sección exclusivos; ver `MEMORY.md`).
- **Defensa de base** (`tests/conflictos_bd.rs`): regla 7 (no `PRESENTE` en
  `CANCELLED`) y regla 10 (no borrar/renumerar identidad) con SQL directo.
- **Años exclusivos** (RNF-6 de la spec): cada test usa su año escolar para no
  chocar con otras fases.

## RF → dónde se prueba

- RF-1, RF-7: `tests/attendance.rs` (upsert, atomicidad) + HTTP 200.
- RF-2, RF-3: `tests/attendance.rs` (409) + `conflictos_bd.rs` (regla 7).
- RF-4, RF-14: HTTP (404 de estudiante y de evento).
- RF-5: dominio + HTTP (400).
- RF-6: `tests/attendance.rs` (409 de inactivo).
- RF-8, RF-9, RF-10: `tests/attendance.rs` (corrección) + HTTP (200/404).
- RF-11, RF-12: `conflictos_bd.rs` (regla 10, SQL directo) + 405 del `DELETE`.
- RF-13: `tests/attendance.rs` + HTTP (orden y `null`).
- RF-15: `tests/attendance.rs` (una sola entrada, y ninguna si no hubo cambio).
- RF-16: `tests/attendance.rs` (fecha futura/pasada se acepta).
