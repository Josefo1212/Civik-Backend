# Spec 009 — Sincronización offline (attendance)

Estado: implementada

> Avance: T0–T6 hechos. Protocolo de conflicto implementado en
> `attendance/domain.rs` (`TOLERANCIA_RELOJ`, `decidir_sincronizacion`),
> `attendance/service.rs` (`sincronizar` / `sincronizar_en_tx`) y la ruta
> `POST /events/{id_event}/attendance/sync`. RF-1 a RF-10 verificados con
> dominio puro (7 unit), integración tx/rollback (`tests/offline.rs`, 6) y
> borde HTTP `oneshot` (`tests/http_offline.rs`, 1). Suite completa en verde;
> `cargo clippy --all-targets -D warnings` 0/0.

## Contexto y objetivo

El instructor usa la PWA en el aula, a veces sin conexión. Marca asistencia en
IndexedDB y la sincroniza cuando vuelve la red. La Fase 7 ya resuelve el
registro y la corrección **en línea** (upsert idempotente por PK, corrección
sin borrar). Lo que falta es el **protocolo de sincronización**: cómo llegan al
servidor los cambios hechos offline sin duplicarse, sin pisar a ciegas una
versión más nueva, y con una respuesta que le permita al cliente reconciliar.

El problema real no es la concurrencia de muchos usuarios (hay uno solo), sino
los **reintentos y el reloj**: una respuesta que se pierde y el cliente vuelve a
enviar el mismo lote, o dos dispositivos del mismo instructor que marcan el
mismo par (evento, estudiante) con horas distintas.

## Usuarios

- Instructor (único usuario del sistema).

## Historias de usuario

- HU-1. Como instructor, quiero sincronizar al recuperar la conexión el lote de
  marcas hechas offline, sin que se dupliquen.
- HU-2. Como instructor, quiero que reintentar una sincronización cuya respuesta
  se perdió no tenga efectos adversos (idempotencia).
- HU-3. Como instructor, quiero enterarme cuando el servidor ya tiene una marca
  más nueva que la mía, y recibir su estado para decidir qué queda.
- HU-4. Como instructor, quiero que una diferencia menor de reloj entre mi
  dispositivo y el servidor no se interprete como un conflicto.

## Definiciones

- **Sincronización**: envío al servidor de un lote de cambios de asistencia
  hechos (posiblemente) offline.
- **`cambio_en`**: instante en que el cliente hizo el cambio, según **su** reloj
  (RFC 3339). Es el reloj que puede estar desfasado.
- **`updated_at`**: instante de la última escritura de una fila de asistencia,
  puesto por **el servidor** (`now()`). Es la "versión" de la fila.
- **Tolerancia de reloj**: ventana que absorbe el desfase entre el reloj del
  cliente (`cambio_en`) y el del servidor (`updated_at`) y que, dentro de ella,
  no se considera conflicto.
- **Conflicto**: la fila existe en el servidor con un estado **distinto** al
  pedido y su `updated_at` es más nuevo que `cambio_en` más allá de la
  tolerancia (el servidor ya tiene una versión que el cliente no conocía).
- **Reintento**: reenvío de un ítem ya aplicado. Se reconoce porque el estado
  pedido coincide con el actual, y no se vuelve a escribir ni a auditar.

## Requisitos funcionales

- RF-1. CUANDO llega un ítem de sincronización para un par (evento, estudiante)
  sin fila existente, EL SISTEMA crea la fila con el estado pedido.
- RF-2. CUANDO llega un ítem cuyo estado pedido coincide con el estado actual de
  la fila, EL SISTEMA responde éxito idempotente **sin** escribir ni auditar
  (reintento de una sincronización cuya respuesta se perdió).
- RF-3. SI la fila existe con un estado distinto y `cambio_en` no es anterior a
  `updated_at` menos la tolerancia, ENTONCES EL SISTEMA aplica el estado pedido
  (gana el cambio del cliente: es al menos tan nuevo como la versión del
  servidor).
- RF-4. SI la fila existe con un estado distinto y `updated_at` es posterior a
  `cambio_en` más la tolerancia, ENTONCES EL SISTEMA **no** pisa la fila y
  reporta un **conflicto** con el estado actual del servidor (el servidor tiene
  una versión más nueva que el cliente no conocía).
- RF-5. La tolerancia de reloj es una constante del dominio de **5 minutos**,
  no configurable. `cambio_en` ausente se trata como "sin reloj de cliente":
  solo puede crear o ser idempotente, nunca pisar una fila existente con estado
  distinto (equivale a tolerancia negativa).
- RF-6. CUANDO hay al menos un conflicto, EL SISTEMA aplica en **una sola
  transacción** todos los ítems que no son conflicto y devuelve, por ítem, el
  resultado (`creado`, `aplicado`, `sin_cambio` o `conflicto`) y, en los
  conflictos, la fila actual del servidor (`state`, `updated_at`), para que el
  cliente reconcilie. La respuesta es `200` aunque haya conflictos: el
  conflicto no es un error de la petición, es un resultado de la fusión.
- RF-7. Cada ítem que se crea o aplica respeta las mismas reglas de escritura de
  la Fase 7 (evento no `CANCELLED`, lapso no `CLOSED`; ver `civik-dominio`) y
  deja su entrada de auditoría.
- RF-8. La sincronización es idempotente: repetirla no crea filas duplicadas ni
  entradas de auditoría de más.
- RF-9. La respuesta de la sincronización expone el `updated_at` resultante de
  cada ítem aplicado, para que el cliente lo guarde como su nueva versión.
- RF-10. La sincronización agrega **una ruta propia por evento**
  (`POST /events/{id_event}/attendance/sync`); las rutas y contratos de la
  Fase 7 **no cambian**.

## Requisitos no funcionales

- RNF-1. Las reglas de conflicto (`cambio_en` vs `updated_at` ± tolerancia) son
  **puras** y viven en `attendance/domain.rs`, sin `sqlx` ni `async`; el
  servicio solo orquesta.
- RNF-2. SQL en archivos `.sql` verificados en compilación (constitución nº 1);
  nada de SQL inline.
- RNF-3. Mensajes y errores en español; `snake_case` en el dominio.
- RNF-4. Tests con patrón transacción/rollback para la integración, y bordes
  HTTP con `oneshot`; años escolares y secciones exclusivos de este archivo
  (aislamiento de tests, `MEMORY.md`).
- RNF-5. Sin `unwrap`/`expect` fuera del arranque (constitución nº 5).

## Casos límite

- Lote vacío: no-op válido; sin escritura ni auditoría.
- Ítem nuevo (`id_student` sin fila) para evento `CANCELLED` o lapso `CLOSED`:
  rechazado igual que en la Fase 7 (RF-7).
- Ítem sobre una fila con el mismo estado: idempotente, aun si `cambio_en` es
  viejo (gana la igualdad de estado sobre el reloj).
- Fila existente con estado distinto y `cambio_en` apenas anterior a
  `updated_at` (dentro de la tolerancia): se aplica, no es conflicto.
- Fila existente con estado distinto y `updated_at` claramente posterior
  (fuera de la tolerancia): conflicto.
- Reintento tras éxito con la respuesta perdida: idempotente (RF-2), no
  conflicto falso.
- Estudiante dado de baja: registrar fila nueva se rechaza; corregir una fila
  existente se permite (misma regla de la Fase 7).

## Fuera de alcance

- Sincronización bidireccional (traer cambios del servidor al cliente): el
  cliente ya consulta con `GET`; esta spec solo cubre subir cambios.
- Resolución automática de conflictos (reglas de fusión): el servidor informa;
  el instructor decide en la PWA.
- Escritura de otros recursos offline (eventos, notas): solo `attendance`.
- Cola/almacenamiento de reintentos en el servidor: el cliente reintenta; el
  servidor es idempotente.

## Criterios de finalización

- RF-1 a RF-10 verificados: dominio puro de las reglas de conflicto con tests
  unitarios, integración con tx/rollback (crear, idempotencia, aplicar dentro de
  la tolerancia, conflicto fuera de ella, respeto de CANCELLED/CLOSED) y borde
  HTTP con `oneshot`.
- `cargo test` verde y `cargo clippy --all-targets -D warnings` 0/0.
- Spec marcada `implementada`, `MEMORY.md` actualizado y commit de cierre.

## Dudas abiertas

Resueltas en la clarificación del 2026-10-10:

- **Tolerancia de reloj: 5 minutos, constante del dominio.** No configurable:
  absorbe el desfase típico de reloj de un dispositivo sin enmascarar
  conflictos reales. Si algún día molesta, se cambia la constante, no se suma
  configuración.
- **Atomicidad: aplicar los no conflictivos, reportar los conflictos.** Todo lo
  aplicable se escribe en una transacción; los conflictos se devuelven por ítem
  con el estado del servidor. El cliente no pierde el trabajo que sí puede
  aplicar. La respuesta es `200`; el conflicto es un resultado de la fusión, no
  un error de la petición.
- **Ruta por evento:** `POST /events/{id_event}/attendance/sync`, coherente con
  la Fase 7; reutiliza la carga de estado del evento/lapso y su `404`. No se
  agrega una ruta global.
- **Reloj del servidor autoritativo:** `updated_at` siempre lo pone el servidor
  (`now()`); el `cambio_en` del cliente nunca se persiste como `updated_at`,
  solo decide si el ítem pisa la fila o queda en conflicto.
