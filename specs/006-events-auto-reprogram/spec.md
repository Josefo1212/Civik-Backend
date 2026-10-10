# Spec 006 — Generación automática y reprogramación

Estado: implementada

## Contexto y objetivo

Portar a dominio de Rust (`events::service` + `events::domain`) lo que hace
un año vivía como `generar_practicas_auto` y `reprogramar_practice_day` en
PL/pgSQL, usando `sqlx::Transaction` para atomicidad (decisión tomada en el
reinicio, ver `AGENTS.md`). Las 10 reglas de dominio siguen siendo
invariantes protegidas por triggers/constraints de la base: el módulo de
Rust decide secuencias de negocio, la base protege contra el bug del
backends.

Las dos decisiones abiertas de la fase se resuelven así (acordadas
2026-10-09):

1. **El generador es manual y cubre todo el rango del lapso.** El instructor
   lo invoca por lapso (`POST /lapses/{id}/generar-practicas`), y el sistema
   crea una práctica `AUTO`/`PRACTICA_REGULAR`/`SCHEDULED` para **cada**
   `practice_day` del lapso, de `start_date` a `end_date`, sin importar si
   esas fechas ya pasaron (la evidencia manda, no el reloj de la máquina:
   constitución, principio 4). Es idempotente: omite los días que ya tienen
   una práctica regular.
2. **Reprogramar = cambiar el día de práctica del lapso.** `POST
   /lapses/{id}/reprogram-practice-day { "nuevo_dia": "THURSDAY" }`
   actualiza `lapses.practice_day` y mueve cada evento pendiente a la fecha
   más próxima con el nuevo día de práctica, dentro del rango del lapso. Los
   eventos con asistencia (congelados) y los `CANCELLED` no se mueven.

## Usuarios

- **Instructor**: único usuario del sistema.

## Historias de usuario

- HU-1. Como instructor, quiero generar las prácticas regulares de todo el
  lapso de un solo golpe, para no crearlas una por una.
- HU-2. Como instructor, quiero cambiar el día de práctica del lapso (p. ej.
  jueves → viernes, por un cambio de horario) y que los eventos pendientes se
  reubiquen solos.
- HU-3. Como instructor, quiero que un evento que ya registró asistencia no
  se mueva: esa práctica ya ocurrió y su fecha es historia.

## Definiciones

- **Práctica regular**: un evento con `type_event = 'PRACTICA_REGULAR'`,
  sea `source = 'AUTO'` o `source = 'MANUAL'`. Según la decisión de dominio
  (`MEMORY.md`), **cada día de práctica tiene como máximo una regular, la
  cree quien la cree**.
- **Evento pendiente** (candidato a moverse): `status = 'SCHEDULED'`, sin
  asistencia registrada, `type_event = 'PRACTICA_REGULAR'` (AUTO o MANUAL).
  Las prácticas extra y las presentaciones **no** se anclan al día de
  práctica (decisión de la fase 5, RF-4), así que no se mueven.

## Requisitos funcionales

- RF-14. CUANDO el instructor invoca `POST /lapses/{id}/generar-practicas`
  con un lapso no `CLOSED`, EL SISTEMA crea un evento
  `AUTO`/`PRACTICA_REGULAR`/`SCHEDULED` (nombre fijo, p. ej. "Práctica
  regular") por cada fecha de `[start_date, end_date]` cuyo día de la semana
  coincide con `lapses.practice_day`, y responde 200 con la lista de eventos
  creados (vacía si no creó ninguno).
- RF-15. SI una fecha ya tiene una práctica regular (AUTO o MANUAL, en
  cualquier estado, incluso `CANCELLED`), ENTONCES EL SISTEMA omite esa fecha
  (no duplica). La operación es idempotente: una segunda llamada crea 0
  eventos.
- RF-16. CUANDO el instructor invoca `POST /lapses/{id}/reprogram-practice-day`
  con `nuevo_dia` ∈ `week_day`, EL SISTEMA actualiza `lapses.practice_day` y
  mueve cada evento pendiente a la fecha más próxima mayor a la suya dentro
  del lapso cuyo día de semana es `nuevo_dia`. La respuesta 200 informa
  `dia_anterior`, `dia_nuevo`, `movidos` y `no_movidos`.
- RF-17. SI la fecha destino de un movimiento ya está ocupada por otra
  práctica regular, ENTONCES EL SISTEMA deja el evento en su fecha y lo suma
  a `no_movidos` (mantiene "una regular por día").
- RF-18. SI ningún evento pendiente puede reubicarse en el rango del lapso
  (no queda un `nuevo_dia` posterior a su fecha), ENTONCES EL SISTEMA lo
  deja en su fecha y lo suma a `no_movidos`.
- RF-19. SI el lapso está `CLOSED`, ENTONCES generar o reprogramar responde
  409. SI `nuevo_dia` no es un día válido, responde 400. Si el lapso no
  existe, 404.
- RF-20. Tanto generar como reprogramar son operaciones **atómicas** (una
  sola transacción) y dejan **una entrada de auditoría** en `audit_logs`
  dentro de esa misma transacción, con los resúmenes de la operación
  (generadas/omitidas; día anterior/nuevo, movidos/no movidos).

## Requisitos no funcionales

- RNF-1. Hexagonal parcial (mismo patrón de `events`): `domain.rs` puro (sin
  `sqlx`, sin `async`), `service.rs` orquesta con `sqlx::Transaction`,
  `queries.rs` + `queries/*.sql` verificado en compilación.
- RNF-2. El catálogo de días y estados se toma de
  `lapses::domain::{DiaSemana, EstadoLapso}`: no hay un segundo mapeo de
  `week_day` en `events` (se elimina el del stub).
- RNF-3. Mensajes en español, identificadores `snake_case` español, sin
  `unwrap`/`expect` fuera del arranque.
- RNF-4. Pruebas con patrón transacción/rollback (integración) y `oneshot`
  (HTTP). Años de lapso exclusivos por test.
- RNF-5. La defensa de la base (triggers, índice único) se respeta y se
  prueba: el 23505 de `uq_events_auto_one_per_day` se mapea a 409 con
  mensaje en español.

## Casos límite

- El lapso recién capturado (`PLANNED`) puede generar prácticas igual que el
  `ACTIVE`; solo el `CLOSED` lo impide.
- Una práctica regular **manual cancelada** ocupa su fecha: el generador la
  omite igual, porque la semana de esa fecha ya tiene una regular (la que se
  suspendió). Para recuperarla, el instructor crea otra manualmente.
- `EXTRA`/`PRESENTACION` sobre un `practice_day` no impiden que el generador
  cree la regular de ese día: la regular suma `1` al peso de la nota y la
  extra `0.5`; pueden convivir en fechas distintas o iguales (son dos
  eventos, y el índice único `uq_events_auto_one_per_day` solo limita
  AUTO-vs-AUTO).
- En la reprogramación el **viernes** (nuevo día) que ya tiene una regular
  (por ejemplo creada a mano antes del cambio) ocupa el destino: el evento
  pendiente que caería ahí se queda donde está y se reporta.
- El evento pendiente cuya próxima fecha del nuevo día queda fuera del lapso
  (última semana con día de práctica agotado) se queda en su fecha: el
  instructor decide después si la cancela o la crea de nuevo.
- Reprogramar con `nuevo_dia` igual al actual es un no-op: 200 con
  `movidos = 0` y **sin** entrada de auditoría (idempotente, sin auditoría
  falsa).
- Auto-generar sobre un lapso que ya tiene sus días completos no toca nada y
  devuelve lista vacía.

## Fuera de alcance

- No hay borrado físico (regla 8): reprogramar mueve fechas, nunca elimina
  eventos "sobrantes"; generar crea, nunca modifica los existentes.
- No hay re-generación "inteligente" (vacaciones, semanas flotantes): el
  generador cubre el rango completo del lapso.
- No hay sincronización offline (fase 9) ni `attendance`/`grades` (fases 7-8)
  acá: solo la orquestación de eventos.

## Criterios de finalización

- RF-14 a RF-20 verificados con tests de integración transacción/rollback en
  `tests/events_auto.rs`; bordes HTTP con `oneshot` en `tests/http_events.rs`;
  defensa de base (23505 y trigger de la regla 1) con SQL directo en
  `tests/conflictos_bd.rs`.
- `cargo test` verde y `cargo clippy --all-targets -D warnings` 0/0.
- Spec marcada `implementada`, `MEMORY.md` actualizado, commit de cierre.

## Dudas abiertas

- Ninguna (las dos abiertas fueron acordadas el 2026-10-09).