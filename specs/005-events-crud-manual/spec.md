# Spec 005 — Events (CRUD manual primero)

Estado: implementada

## Contexto y objetivo

Los eventos representan las actividades del lapso (clases prácticas, presentaciones). `events` referencia a `lapses`, y `attendance`/`grades` dependen de ellos. Esta fase implementa únicamente la **parte manual** (`source = 'MANUAL'`): crear (presentación, práctica extra, práctica regular manual), consultar/listar, editar (solo eventos MANUAL sin asistencia, no CANCELLED) y **cancelar con motivo**. No incluye `generar_practicas_auto` ni `reprogramar_practice_day` (Fase 6).

Se mantiene la arquitectura hexagonal parcial: `domain.rs` (reglas puras), `service.rs` (orquestación + transacción), `queries.rs` + `queries/*.sql` (SQL verificado en compilación), `handler.rs` (capa fina HTTP). Auditoría obligatoria donde aplique (crear, cancelar).

## Usuarios

- **Instructor**: único usuario del sistema.

## Historias de usuario

- HU-1. Como instructor, quiero registrar un evento manual (PRESENTACION, PRACTICA_EXTRA o PRACTICA_REGULAR con source MANUAL) para documentar actividades no automáticas.
- HU-2. Como instructor, quiero listar/consultar eventos por lapso para ver el calendario.
- HU-3. Como instructor, quiero cancelar un evento SCHEDULED con motivo, dejando registro de la cancelación.
- HU-4. Como instructor, quiero editar un evento MANUAL (sin asistencia, no CANCELLED) para corregir datos erróneos.

## Requisitos funcionales

- RF-1. Crear evento manual: debe aceptar `id_lapse`, `event_date` (AAAA-MM-DD), `type_event` ∈ `{PRACTICA_REGULAR,PRACTICA_EXTRA,PRESENTACION}`, `event_name`, `source = 'MANUAL'`, `status = 'SCHEDULED'`. Nace sin `motivo_cancelado`.
- RF-2. Validación de entrada (dominio): `event_date` formato AAAA-MM-DD, `event_name` no vacío (trim), `type_event` válido, `id_lapse` existente.
- RF-3. Rango dentro del lapso: la fecha del evento debe estar entre `start_date` y `end_date` del lapso (`check_event_date_within_lapse`).
- RF-4. Práctica regular manual debe caer **únicamente** en el `practice_day` del lapso. Práctica extra y presentación **no** están restringidas al practice_day por esta regla (se respeta la semántica: la regular ocurre en día de práctica del lapso; extra/presentación pueden ser en otro día según necesidad). Anticipar en dominio/service con mensaje claro.
- RF-5. Fecha pasada permitida mientras el lapso esté abierto (no CLOSED). No hay comparación con "hoy" en creación manual (mismo criterio que lapses: fechas pasadas aceptadas si válidas y dentro del lapso).
- RF-6. Listar eventos: por lapso (`GET /events?lapse_id=...`) y/o completo (`GET /events`). Orden canónico por `event_date` ascendente, luego por `created_at` ascendente.
- RF-7. Consultar evento por id: `GET /events/{id}` → 200 o 404.
- RF-8. Cancelar evento con motivo: `POST /events/{id}/cancel` body `{motivo}`. Solo permitido si `status == 'SCHEDULED'`. Al cancelar: `status='CANCELLED'`, `motivo_cancelado = motivo` (trim), auditar acción `cancelar` con payload incluyendo motivo y estado.
- RF-9. Congelación por evidencia: **no** se permite cancelar un evento que **ya ocurrió** (tiene al menos una asistencia registrada). Un evento con asistencia queda congelado (no editable, no cancelable). Responder 409 con mensaje claro.
- RF-10. Editar evento MANUAL: solo si `source == 'MANUAL'`, `status != 'CANCELLED'`, **sin asistencia**. Campos editables: `event_date`, `type_event`, `event_name` (según validación). No cambia `id_lapse`, `source`, `status`. Debe seguir RF-3 y RF-4 tras edición. Auditar `editar` si cambia valores.
- RF-11. Eventos CANCELLED: no editables, no cancelables nuevamente. Intento → 409 con mensaje claro.
- RF-12. Auditoría: crear evento (alta), cancelar (con motivo), editar (si aplica). `entity = "events"`, acciones `crear/cancelar/editar`. Payload en español con campos relevantes.
- RF-13. HTTP: respuestas coherentes (201 crear, 200 listar/consultar/cancelar/editar, 204 no usado aquí; errores 400/404/409). No existe `DELETE` físico de eventos (nunca se borra físicamente — regla 8).

## Requisitos no funcionales

- RNF-1. Hexagonal parcial: `domain.rs` puro (sin sqlx/async). `service.rs` orquesta y usa `sqlx::Transaction` cuando hay escritura con auditoría. Lecturas finas pueden ir directo a `queries` desde handler si no hay negocio.
- RNF-2. SQL en archivos `queries/*.sql`, `sqlx::query_file!`/`query_file_as!`. Un archivo por consulta.
- RNF-3. Mensajes en español, identificadores `snake_case` español en dominio/módulo, crates/externos inglés.
- RNF-4. `#![deny(clippy::unwrap_used, clippy::expect_used)]` respetado; errores propagados con `?`, sin panics fuera de arranque.
- RNF-5. Tests con patrón transacción/rollback (integración), HTTP con `oneshot`, defensa BD con SQL directo (T8). Evitar deadlocks: años/lapsos exclusivos por test.
- RNF-6. Anticipar errores de dominio/triggers con mensajes claros antes de que suban (pre-chequeos en service) cuando útil para UX; triggers siguen siendo defensa.
