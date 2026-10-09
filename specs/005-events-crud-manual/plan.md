# Plan 005 — Events (CRUD manual primero)

## Decisiones de diseño

- **Hexagonal parcial** (igual que `lapses`): `domain.rs` puro (validaciones, reglas de cancelación/edición). `service.rs` dueño de transacción cuando hay escritura + auditoría (`crear`, `cancelar`, `editar`). `handler.rs` fino.
- **Sin DELETE físico**: solo `cancelar` con motivo. RF-13.
- **Congelación por evidencia (RF-9)**: evento con >=1 asistencia → no cancelable, no editable. Service consulta `has_attendance` antes de cancelar/editar.
- **Práctica regular manual solo en `practice_day` (RF-4)**: service lee lapso (`id_lapse`) y valida: si `type_event == PRACTICA_REGULAR` y `source == MANUAL` → `event_date.weekday()` debe coincidir con `practice_day` del lapso. Pasar datos del lapso al dominio o validar en service (cruzado con otra tabla) — más limpio validar en service tras leer lapso; dominio recibe el dato validado o valida con helper puro si se pasa `DiaSemana` del lapso.
- **Fechas pasadas permitidas (RF-5)**: dominio NO compara con `Utc::now().date_naive()` en creación manual. Solo orden/validez y rango lapso (RF-3).
- **Edición limitada (RF-10)**: solo eventos `MANUAL`, no `CANCELLED`, sin asistencia. Campos: `event_date`, `type_event`, `event_name`.
- **Rango dentro del lapso (RF-3)**: anticipado en service (`event_date >= start AND <= end`). Trigger existente `check_event_date_within_lapse` (23514) sigue como defensa (probado T8).
- **Auditoría (RF-12)**: `crear` (payload con id_lapse, fecha, tipo, nombre, source, status), `cancelar` (agrega motivo y estado), `editar` (agrega valores antes/después o solo estado/cambios relevantes) — seguir patrón lapses (`payload_de` estilo español).

## Modelo y rutas

### Endpoints
- `POST /events` → crear manual (201)
- `GET /events` → listar todos (200), ordenado por fecha asc, created_at asc
- `GET /events?lapse_id={uuid}` → listar por lapso (200)
- `GET /events/{id}` → consultar (200/404)
- `PATCH /events/{id}` → editar manual (200/404/409)
- `POST /events/{id}/cancel` → cancelar con motivo `{motivo}` (200/404/409)

### Estado/Tipos (Postgres)
`event_status`: SCHEDULED/CANCELLED. `type_event`: PRACTICA_REGULAR/PRACTICA_EXTRA/PRESENTACION. `source`: MANUAL/AUTO.

## Mapa RF → tests

T6 (integración tx/rollback): RF-1–RF-6 básicos, RF-8–RF-12, RF-9/11 negativos. T7 HTTP oneshot: mismos bordes. T8 BD: triggers defensa.

## Archivos a crear

- `src/modules/events/domain.rs`
- `src/modules/events/service.rs`
- `src/modules/events/handler.rs`
- `src/modules/events/queries.rs`
- `src/modules/events/queries/*.sql` (create_manual, get_by_id, list_all, list_by_lapse, cancel, update_manual, has_attendance, get_lapse)
- `tests/events.rs` (T6)
- `tests/http_events.rs` (T7)

Rutas agregadas en `src/app.rs` y módulo en `src/modules/events/mod.rs` (mismo patrón lapses).
