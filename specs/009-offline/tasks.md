# Tasks 009 — Offline

Estado: implementada (clarificación cerrada 2026-10-10; cierre 2026-10-10).
Una tarea a la vez: test primero, `cargo test` verde, `cargo clippy` 0/0.

## T0 — hecho
- [x] spec redactada y aprobada (`Estado: implementada`)
- [x] plan/tasks

## T1 — hecho
- [x] revisar attendance para soporte `updated_at`/conflictos
- [x] añadir tests base (idempotencia, corrección, HTTP)

## T2 — hecho. Dominio puro de la decisión de sincronización. RF-1..RF-5
- [x] `attendance/domain.rs`: `TOLERANCIA_RELOJ` (5 min), `VersionActual`,
  `DecisionSincronizacion`, `decidir_sincronizacion` (sin `sqlx`, sin `async`).
- [x] 7 tests unitarios: crear, sin cambio, aplicar dentro y en el borde exacto
  de la tolerancia, conflicto fuera, `cambio_en` ausente, cambio más nuevo.

## T3 — hecho. Service `sincronizar` + ruta. RF-6..RF-10
- [x] `service.rs`: `LoteSync`/`ItemSync`/`ItemResultado`; `sincronizar` abre
  transacción y delega en `sincronizar_en_tx` (probable dentro de la tx del
  test). Valida evento/lapso y estados, valida estudiantes solo al crear, decide
  por ítem, escribe los no-conflicto, audita contadores y devuelve resultados.
- [x] `handler.rs`: `POST /events/{id_event}/attendance/sync` → `200` con la
  lista; sin cambios en las rutas de la Fase 7.

## T4 — hecho. Integración tx/rollback. RF-1..RF-9
- [x] `tests/offline.rs` (6): lote mixto (creado + sin_cambio + aplicado +
  conflicto), `CANCELLED`, `CLOSED`, baja (corrige pero no crea), idempotencia
  sin auditoría de más, estado inválido y lote vacío.

## T5 — hecho. Borde HTTP `oneshot`. RF-6, RF-9, RF-10
- [x] `tests/http_offline.rs` (1): `200` con la clasificación por ítem y su
  `updated_at`, `404` de evento, `400` de estado inválido, `409` de evento
  cancelado; purga con año (2016) y sección (`WOf1`) exclusivos.

## T6 — hecho. Cierre de la fase
- [x] RF-1..RF-10 verificados y marcados.
- [x] `cargo test` verde (312 tests), `cargo clippy --all-targets -D warnings`
  0/0, `cargo fmt --check` limpio.
- [x] `MEMORY.md` actualizado; spec en `implementada`; commit de cierre.
