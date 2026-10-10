# Spec 007 — Asistencia (registrar y corregir)

Estado: borrador

## Contexto y objetivo

La asistencia (`attendance`) es la evidencia que define si un evento ocurrió:
un evento "ocurrió" si tiene al menos una fila de asistencia, no si su fecha ya
pasó (constitución, principio 4). Es además la materia prima de la nota
(fase 8) y la que congela un evento (regla 9) y la sección de un estudiante
(regla 3).

Esta fase construye, sobre `events` y `students` ya terminados, el registro y
la corrección de asistencia **sin sincronización offline** (eso es la fase 9).
El modelo de datos ya existe y no cambia: `attendance` tiene PK
`(id_event, id_student)` —una fila por par— y estado
`PRESENTE | AUSENTE | JUSTIFICADO`, protegida por los triggers de las reglas 4,
7 y 10 y las FKs `RESTRICT` de la regla 8. La base manda; el backend decide la
secuencia y traduce los rechazos a mensajes en español.

## Usuarios

- Instructor (único usuario del sistema).

## Historias de usuario

- HU-1. Como instructor, quiero registrar la asistencia de un evento de una
  sola vez (pasar lista) para no marcar estudiante por estudiante.
- HU-2. Como instructor, quiero corregir el estado de un estudiante en un
  evento cuando me equivoqué, sin borrar nada.
- HU-3. Como instructor, quiero ver el padrón de un evento con el estado de
  cada estudiante para saber a quién falta marcar.
- HU-4. Como instructor, quiero que el sistema me impida registrar asistencia
  en un evento cancelado o en un lapso cerrado.

## Definiciones

- **Padrón de asistencia de un evento**: los estudiantes aplicables al evento
  (ver RF-13) con el estado registrado, o `null` si no están marcados.
- **Sin marcar**: un estudiante sin fila en `attendance` para el evento. No es
  `AUSENTE`: no está evaluado, no penaliza la nota y no cuenta en `N` (fase 8).
- **Registrar**: crear las filas faltantes y actualizar las existentes (upsert)
  para un evento. Una segunda pasada corrige, no duplica.
- **Corregir**: cambiar el `state` de una fila ya existente. Es la única
  modificación permitida (regla 10: la asistencia no se borra, se corrige).
- **JUSTIFICADO**: no penaliza la nota (solo `AUSENTE` lo hace, fase 8); se
  distingue de `PRESENTE` para el registro histórico.

## Requisitos funcionales

- RF-1. CUANDO el instructor registra la asistencia de un evento
  (`POST /events/{id}/attendance`) con una lista de pares
  `id_student`/`state`, EL SISTEMA crea las filas faltantes y actualiza las
  existentes (upsert por PK), y responde 200 con el padrón actualizado.
- RF-2. SI el evento está `CANCELLED`, ENTONCES el registro responde 409
  (regla 7: no se registra asistencia en un evento cancelado).
- RF-3. SI el lapso del evento está `CLOSED`, ENTONCES el registro responde
  409 (regla 4: un lapso cerrado no admite datos nuevos).
- RF-4. SI un `id_student` de la lista no existe, ENTONCES el sistema responde
  404 y no registra ninguna fila.
- RF-5. SI un `state` no está en `PRESENTE | AUSENTE | JUSTIFICADO`, ENTONCES
  el sistema responde 400.
- RF-6. SI un `id_student` de la lista está inactivo y no tiene fila en el
  evento, ENTONCES el sistema responde 409 (a un estudiante de baja no se le
  registra asistencia nueva).
- RF-7. Toda la lista se registra en **una sola transacción**: o se guardan
  todas las filas, o ninguna.
- RF-8. CUANDO el instructor corrige el estado de un par (evento, estudiante)
  (`PATCH /events/{id}/attendance/{student_id}`), EL SISTEMA actualiza
  únicamente `state` y responde 200 con la fila.
- RF-9. SI se corrige un par sin fila registrada, ENTONCES el sistema responde
  404 (la corrección no crea: para eso está el registro).
- RF-10. La corrección es idempotente: si el nuevo estado es igual al actual,
  responde 200 y no deja entrada de auditoría (no hubo cambio real).
- RF-11. EL SISTEMA nunca borra una fila de asistencia (regla 10): no existe
  endpoint de borrado y la base lo impide por trigger.
- RF-12. EL SISTEMA no permite cambiar el evento ni el estudiante de una fila,
  solo el estado (regla 10). La base lo impide por trigger.
- RF-13. CUANDO el instructor consulta el padrón de un evento
  (`GET /events/{id}/attendance`), EL SISTEMA devuelve, en el orden canónico
  de estudiantes (`lower(last_name), lower(first_name), id`), cada estudiante
  **activo** más cada estudiante con fila en ese evento (aunque esté de baja),
  con su `state` o `null`.
- RF-14. SI el evento consultado por el padrón o por el registro no existe,
  ENTONCES responde 404.
- RF-15. Registrar o corregir deja **una sola entrada** de auditoría en
  `audit_logs` dentro de la misma transacción: `registrar_asistencia` (con el
  resumen de creadas/actualizadas) o `corregir_asistencia` (estado anterior y
  nuevo).
- RF-16. EL SISTEMA no mira el reloj: se puede registrar o corregir asistencia
  de un evento con fecha futura o pasada mientras el evento siga `SCHEDULED` y
  el lapso no esté `CLOSED` (constitución, principio 4).

## Requisitos no funcionales

- RNF-1. Hexagonal parcial (mismo patrón de `events`): `domain.rs` puro (sin
  `sqlx`, sin `async`), `service.rs` orquesta con `sqlx::Transaction`,
  `queries.rs` + `queries/*.sql` verificado en compilación.
- RNF-2. El módulo usa newtypes de los ids (`EventId`, `StudentId`): combina
  dos tipos de id en las mismas firmas (AGENTS.md), así que pasar un `Uuid`
  equivocado deja de compilar.
- RNF-3. SQL nunca mapea el enum `attendance_state` como enum de sqlx: se lee
  y escribe como texto (aprendizaje de `sections.shift`, `MEMORY.md`).
- RNF-4. Mensajes en español, identificadores `snake_case`, sin
  `unwrap`/`expect` fuera del arranque.
- RNF-5. Tests de integración con patrón transacción/rollback y bordes HTTP
  con `oneshot`; la defensa real de Postgres (triggers de las reglas 4, 7 y
  10) se prueba con SQL directo en `tests/conflictos_bd.rs`.
- RNF-6. Años de lapso exclusivos por test (2000–2100) para no colisionar con
  los de otras fases.

## Casos límite

- Re-registrar el mismo evento no duplica filas: la PK `(id_event, id_student)`
  manda y el upsert actualiza.
- Un evento con una sola fila ya "ocurrió" (regla 5): desde ahí queda
  congelado (regla 9) y ese estudiante no puede cambiar de sección (regla 3).
- Corregir a `AUSENTE` un evento que estaba todo `PRESENTE` no borra nada:
  cambia el estado y queda auditado.
- Una lista vacía de asistencia es un no-op válido (200 con el padrón), sin
  cambio y sin entrada de auditoría.
- Un estudiante dado de baja después de tener asistencia sigue apareciendo en
  el padrón de ese evento y su estado se puede corregir (RF-13), pero no se le
  registra asistencia nueva (RF-6).
- Registrar dos veces seguidas el mismo par con estados distintos deja solo la
  última: la corrección es un `UPDATE`, no un segundo hecho.
- El estado `JUSTIFICADO` no penaliza la nota pero sí consta como asistencia
  registrada (el evento ocurrió, regla 5).

## Fuera de alcance

- Sincronización offline, idempotencia de red, `updated_at` y tolerancia de
  reloj (fase 9).
- El cálculo de la nota y los overrides (fase 8).
- Borrado de asistencia y cambio de identidad (regla 10): la única escritura
  posterior al alta es corregir el estado.
- Acotar los eventos por sección: el esquema no tiene `id_section` en `events`
  (ver "Dudas abiertas").

## Criterios de finalización

- RF-1 a RF-16 verificados con tests de integración (transacción/rollback),
  bordes HTTP con `oneshot` y defensa de base con SQL directo.
- `cargo test` verde y `cargo clippy --all-targets -D warnings` 0/0.
- Spec marcada `implementada`, `MEMORY.md` actualizado y commit de cierre.

## Dudas abiertas

- [NECESITA ACLARACIÓN] **Alcance del padrón (RF-13).** `events` no tiene
  sección, así que el padrón por defecto es "todos los estudiantes activos"
  (eventos de todo el colegio); el frontend agrupa por sección si quiere.
  ¿Es correcto, o la asistencia debe acotarse por sección (implicaría cambiar
  el esquema y revisar las fases 5–6)?
- [NECESITA ACLARACIÓN] **Forma del registro (RF-1).** Se asumió **alta en
  lote** por evento (un `POST` con la lista), que es lo que sirve para pasar
  lista y para la sincronización offline futura. ¿Se prefiere alta individual
  (una llamada por estudiante)?
- [NECESITA ACLARACIÓN] **Estudiante de baja (RF-6, RF-13).** Se asumió: no se
  le registra asistencia nueva (409), pero su asistencia existente se puede
  corregir y sigue apareciendo en el padrón de ese evento. ¿Correcto?
