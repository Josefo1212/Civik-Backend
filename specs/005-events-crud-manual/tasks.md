# Tasks 005 — Events (CRUD manual primero)

## T0. Spec y plan aprobados
- [x] `specs/005-events-crud-manual/spec.md` completo (RF-1..RF-13, RNF-1..RNF-6)
- [x] `plan.md` con decisiones, rutas, mapa RF→test
- [x] `tasks.md` listo
- Estado spec: `implementada`

## T1. `src/modules/events/domain.rs` — reglas puras
- [ ] Enum/validaciones: `TypeEvent`, `EventStatus` mapping texto→enum (puro)
- [ ] `preparar_creacion_manual` valida entrada (nombre trim no vacío, fechas AAAA-MM-DD, tipo válido)
- [ ] `puede_cancelarse(estado, tiene_asistencia)` → Result (RF-9, RF-11)
- [ ] `puede_editarse(estado, source, tiene_asistencia)` → Result (RF-10, RF-11)
- [ ] Helper: `es_practica_regular_manual(type_event, source)` boolean
- [ ] Unit tests en `#[cfg(test)]` módulo (críticos: congelación, edición restringida)

Hecho cuando: compila, `cargo clippy --lib` limpio.

## T2. Queries + SQL (`src/modules/events/queries.rs` + `queries/*.sql`)
- [ ] `queries/create_manual.sql`
- [ ] `queries/get_by_id.sql`
- [ ] `queries/list_all.sql` (ORDER BY event_date ASC, created_at ASC)
- [ ] `queries/list_by_lapse.sql` mismo orden
- [ ] `queries/cancel.sql` (UPDATE status='CANCELLED', motivo_cancelado, updated_at WHERE id=$1 AND status='SCHEDULED' RETURNING *)
- [ ] `queries/update_manual.sql` (UPDATE campos editables + updated_at WHERE id=$1 AND source='MANUAL' AND status!='CANCELLED')
- [ ] `queries/has_attendance.sql` (COUNT(*) asistencia evento)
- [ ] `queries/get_lapse.sql` (SELECT id, school_year, lapse_number, start_date, end_date, practice_day, status FROM lapses WHERE id=$1)

- [ ] `queries.rs`: structs `Event`, `LapseLite`/necesarios, `crear_manual`, `get_by_id`, `listar_all`, `listar_por_lapso`, `cancelar`, `editar_manual`, `tiene_asistencia`, `obtener_lapso`, `payload_*`

Hecho cuando: compila con macros sqlx; `cargo clippy --lib` limpio.

## T3. `src/modules/events/service.rs` — orquestación + tx
- [ ] `EntradaCrearManual { id_lapse, event_date, type_event, event_name }`
- [ ] `EntradaEditarManual { event_date, type_event, event_name }` (opcional)
- [ ] `crear_manual(pool, &entrada)`: tx: obtener lapso → validar RF-3 (rango) → RF-4 (practice_day si PRACTICA_REGULAR MANUAL) → crear + auditar `crear`. Mapear violaciones → AppError.
- [ ] `cancelar(pool, id, motivo)`: tx: get_by_id → tiene_asistencia → puede_cancelarse → UPDATE + auditar `cancelar` (payload con motivo). Respetar idempotencia? si ya CANCELLED, puede_cancelarse devuelve error 409 (RF-11).
- [ ] `editar_manual(pool, id, &entrada)`: tx: get_by_id → tiene_asistencia → puede_editarse → validar RF-3/RF-4 con lapso → UPDATE + auditar `editar`.
- [ ] Lecturas: `consultar`, `listar_*` pueden delegar a queries (o exponer); mantener handler fino.

Hecho cuando: compila, clippy limpio.

## T4. `src/modules/events/handler.rs` + `src/app.rs` + `mod.rs`
- [ ] `mod.rs`: `pub mod domain; pub mod queries; pub mod service; pub mod handler;`
- [ ] `handler.rs`: crear, listar (query lapse_id opcional), consultar, editar (PATCH), cancelar (POST /{id}/cancel)
- [ ] `app.rs`: registrar rutas `/events` y `/events/{id}` y `/events/{id}/cancel`
- [ ] `a_not_found` helper (RowNotFound → 404)

Hecho cuando: rutas montadas, compila, clippy limpio.

## T5. Integración dominio+queries (sanity)
- [ ] `cargo check --all-targets` verde
- [ ] `cargo test --lib` verde (unit tests domain)
- [ ] `cargo clippy --all-targets -D warnings` verde

## T6. `tests/events.rs` — integración tx/rollback
RF-1–RF-6, RF-8–RF-12, casos negativos RF-9, RF-11.
Casos mínimos:
- crear manual válido (PRACTICA_EXTRA) con lapso abierto
- práctica regular manual **en practice_day** OK
- práctica regular manual **fuera de practice_day** → rechazado (409)
- fecha fuera de rango lapso → rechazado (23514 o 409 según prechequeo)
- listar por lapso ordenado
- cancelar SCHEDULED con motivo → status CANCELLED + auditoría con motivo
- cancelar con asistencia → 409 (RF-9)
- cancelar CANCELLED → 409 (RF-11)
- editar MANUAL sin asistencia OK (cambia fecha/tipo/nombre)
- editar con asistencia → 409 (RF-9)
- editar CANCELLED → 409 (RF-11)
- editar evento NO MANUAL (crear AUTO vía SQL directo?) → no editable (RF-10)
- auditoría crear/cancelar/editar presentes

Usar años exclusivos (patrón lapses http), crear lapsos dentro de tx o vía queries; rollback al final.

Hecho cuando: 15–20 tests verdes, clippy limpio.

## T7. `tests/http_events.rs` — oneshot
Mismos bordes HTTP: 201 crear, 200 listar/consultar/editar/cancelar, 400/404/409. Crear lapsos por HTTP o SQL; limpiar eventos CANCELLED no necesario (se quedan). Usar lapse_id real. Probar query `?lapse_id=`.

Hecho cuando: verdes, clippy limpio.

## T8. Ampliar `tests/conflictos_bd.rs` (defensa BD)
- evento fuera de rango lapso → 23514 (`check_event_date_within_lapse` / `trg_events_*` según esquema)
- práctica regular fuera de practice_day? trigger `trg_events_practice_day` aplica a AUTO (RF-6 regla 1)? Verificar: "Un evento `AUTO` debe caer en el `practice_day`". MANUAL: la regla explícita en F5 es práctica regular manual solo en practice_day — puede haber trigger solo para AUTO; validar con schema. Si MANUAL no dispara ese trigger, no forzarlo.
- cancelar/editar con asistencia: defensa service (prechequeo) + trigger de congelación por evidencia (`trg_events_frozen_by_attendance` regla 9)? Existe ese trigger en init? Verificar antes (regla 9: "Un evento que ocurrió queda congelado en sus columnas estructurales"). Si trigger bloquea UPDATE sobre evento con asistencia, probar 23514.

Hecho cuando: 2–4 tests nuevos verdes, clippy limpio.

## T9. Validación y cierre
- [ ] Tabla RF→test en `tasks.md` (veredicto)
- [ ] `curl` real sobre endpoints con lapso creado
- [x] `specs/005-events-crud-manual/spec.md` → `Estado: implementada`
- [ ] `MEMORY.md`: actualizar (Fase 5 cerrada, siguiente Fase 6 `events` automático)
- [ ] `cargo test && cargo clippy --all-targets -D warnings` 0/0
- [ ] Commit de cierre: `Docs/Feat:T9 events-crud-manual - cierre Fase 5`

Hecho cuando: todo verde, docs actualizadas, commit hecho.
