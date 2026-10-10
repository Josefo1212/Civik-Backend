# Plan 009 — Offline

## Nivel de hexagonalidad

**Parcial** (igual que `attendance` en la Fase 7, `AGENTS.md`): la regla de
conflicto es una función **pura** en `attendance/domain.rs`; el service orquesta
la transacción y el SQL vive en `queries/*.sql`. No hay `trait`/adaptador: hay un
solo backend y una sola base, y el desacople formal no aporta acá.

## Reloj y tolerancia

- `updated_at` lo pone siempre el servidor (trigger `set_updated_at` en UPDATE,
  `DEFAULT now()` en INSERT). El `cambio_en` del cliente **no** se persiste.
- `TOLERANCIA_RELOJ = 5 minutos`, constante del dominio (`chrono::Duration`).
- Regla: un ítem con estado distinto al actual se aplica si
  `cambio_en >= actual.updated_at - TOLERANCIA_RELOJ`; si no, es conflicto.

## Archivos

| Archivo | Responsabilidad |
|---|---|
| `src/modules/attendance/domain.rs` | `DecisionSincronizacion` (`Crear`/`Aplicar`/`SinCambio`/`Conflicto`), `VersionActual`, `decidir_sincronizacion`, `TOLERANCIA_RELOJ`. Puro, sin `sqlx`. |
| `src/modules/attendance/service.rs` | `LoteSync`/`ItemSync` (entrada), `ItemResultado` (salida), `sincronizar`: carga estado del evento/lapso, valida, decide por ítem, escribe los no-conflicto en **una** transacción, audita y devuelve los resultados. |
| `src/modules/attendance/handler.rs` | `POST /events/{id_event}/attendance/sync` → `200` con `Vec<ItemResultado>`. |
| `src/modules/attendance/queries.rs` | Se reutiliza `get`, `rows_by_event`, `upsert`, `set_state`, `padron`; no se agrega SQL nuevo. |
| `tests/offline.rs` | Integración tx/rollback: crear, idempotencia, aplicar dentro de la tolerancia, conflicto fuera, reglas CANCELLED/CLOSED, baja. |
| `tests/http_offline.rs` | Borde HTTP con `oneshot`: `200` con conflictos reportados, `404` de evento, `400` de estado inválido, `409` de evento cancelado. |

## Modelo de dominio (puro)

```rust
const TOLERANCIA_RELOJ: Duration = Duration::minutes(5);

struct VersionActual<'a> {
    state: Option<&'a str>,
    updated_at: DateTime<Utc>,
}

enum DecisionSincronizacion {
    Crear,      // no hay fila
    SinCambio,  // el estado pedido ya es el actual (reintento)
    Aplicar,    // hay fila con otro estado, pero dentro de la tolerancia
    Conflicto,  // hay fila con otro estado y el servidor va más adelante
}

fn decidir_sincronizacion(
    pedido: &str,
    actual: Option<VersionActual<'_>>,
    cambio_en: Option<DateTime<Utc>>,
) -> DecisionSincronizacion;
```

- Sin fila → `Crear`.
- Con fila e `state` igual → `SinCambio` (gana la igualdad sobre el reloj).
- Con fila y estado distinto: si `cambio_en` está presente y
  `cambio_en >= updated_at - TOLERANCIA_RELOJ` → `Aplicar`; en cualquier otro
  caso (sin `cambio_en`, o fuera de la ventana) → `Conflicto`.

## Contrato de red

`POST /events/{id_event}/attendance/sync`

```json
{ "items": [ { "id_student": "...", "state": "AUSENTE", "cambio_en": "2026-10-10T13:00:00Z" } ] }
```

- `cambio_en` es opcional (RFC 3339). Ítems nuevos pueden omitirlo.

Respuesta `200`:

```json
[
  { "id_student": "...", "resultado": "creado",    "state": "AUSENTE", "updated_at": "..." },
  { "id_student": "...", "resultado": "sin_cambio", "state": "PRESENTE", "updated_at": "..." },
  { "id_student": "...", "resultado": "conflicto",  "state": "PRESENTE", "updated_at": "..." }
]
```

- Evento inexistente → `404`; estado inválido → `400`; evento `CANCELLED` o
  lapso `CLOSED` → `409` (nada se escribe, igual que la Fase 7).

## Decisiones técnicas (con alternativa descartada)

- **Regla pura en el dominio, no en SQL**: la comparación `cambio_en` vs
  `updated_at ± tolerancia` depende de dos relojes y de una ventana; probarla
  sin Postgres es lo que garantiza que los bordes (tolerancia exacta, ausencia
  de `cambio_en`) queden cubiertos. Descartado: un `WHERE updated_at <= $x` en
  SQL, que esconde la regla y obliga a una base para probarla.
- **Varios `.sql` existentes, ninguno nuevo**: la sincronización reutiliza
  `get`/`upsert`/`set_state`; el conflicto se decide en Rust antes de escribir.
  Descartado: un `INSERT ... ON CONFLICT ... WHERE` que no puede expresar la
  clasificación por ítem (`sin_cambio` vs `conflicto`).
- **Una transacción para todo lo aplicable**: coherente con la Fase 7 y con la
  decisión de atomicidad. Descartado: una transacción por ítem, que dejaría
  estados intermedios si el proceso muere.
- **Auditoría con contadores, no por ítem**: una entrada por sincronización
  (`creados`, `aplicados`, `sin_cambio`, `conflictos`); el detalle vive en la
  respuesta. Descartado: una entrada por ítem, que infla el log sin aportar.

## Estrategia de tests

- **Dominio puro** (unit en `domain.rs`): `Crear`, `SinCambio`, `Aplicar` justo
  en el borde de la tolerancia, `Conflicto` fuera, `cambio_en` ausente con fila
  distinta → `Conflicto`.
- **Integración `tx`/`rollback`** (`tests/offline.rs`): lote con un nuevo, uno
  idempotente, uno aplicable y uno conflictivo en la misma llamada; verificar
  que los tres primeros se escriben y el conflictivo no; reglas `CANCELLED`/
  `CLOSED` (rechazo global) y estudiante de baja.
- **HTTP `oneshot`** (`tests/http_offline.rs`): `200` con la clasificación por
  ítem, `404` de evento, `400` de estado inválido, `409` de evento cancelado.
  Purga su residuo al arrancar (año y sección exclusivos).
- **Años exclusivos** por archivo (aislamiento de tests, `MEMORY.md`).

## RF → dónde se prueba

- RF-1: integración (crear) + dominio (`Crear`).
- RF-2: integración (reintento) + dominio (`SinCambio`).
- RF-3: integración (dentro de la tolerancia) + dominio (`Aplicar`).
- RF-4: integración (conflicto) + dominio (`Conflicto`).
- RF-5: dominio (constante + `cambio_en` ausente).
- RF-6: integración (lote mixto) + HTTP (forma de la respuesta).
- RF-7: integración (`CANCELLED`/`CLOSED`/baja).
- RF-8: integración (repetir no duplica ni audita de más).
- RF-9: HTTP (cada ítem trae su `updated_at`).
- RF-10: HTTP (la ruta de la Fase 7 sigue respondiendo igual).
