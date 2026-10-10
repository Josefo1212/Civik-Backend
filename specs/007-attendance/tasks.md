# Tasks 007 — Attendance

Estado: implementada (reconciliación cerrada 2026-10-10). Una tarea a la vez:
test primero, `cargo test` verde, `cargo clippy` 0/0, marcar y parar.

Veredicto: RF-1..RF-16 cumplidos. RF-1/7 (T1, `tests/attendance.rs`); RF-5/RNF-2
(T2, unit); RF-1..7/14/15 (T3); RF-13/14 (T4); RF-8..12/15 (T5); RF-1/8/13/14 (T6,
`tests/http_attendance.rs`); RF-11/12 (T7, `tests/conflictos_bd.rs`); RNF-1/3
respetados (SQL en `.sql`, dominio puro).

## T0
- [x] spec en formato SDD, dudas resueltas, estado `aprobada`
- [x] plan y tasks

## T1. Upsert real del lote. RF-1, RF-7 [x]
- Hecho cuando: `queries/upsert.sql` existe y se usa con `query_file!`;
  re-registrar el mismo par actualiza `state` sin `23505`; el test
  `registrar_idempotente_no_duplica` se reescribe para afirmar idempotencia
  (no fallo), y pasa.

## T2. Dominio: estado y newtypes. RF-5, RNF-2 [x]
- Hecho cuando: `domain.rs` expone `AttendanceState::parse/as_str`,
  `validar_state`, `puede_escribir_asistencia` y los newtypes
  `EventId`/`StudentId`; sus tests unitarios pasan.

## T3. `registrar_listado`: chequeos y padrón. RF-1..RF-7, RF-14, RF-15 [x]
- Hecho cuando: rechaza `CANCELLED` (409), lapso `CLOSED` (409), `state`
  inválido (400), `id_student` inexistente (404 sin escribir nada) e inactivo
  sin fila (409); escribe todo en una transacción; deja **una** entrada de
  auditoría; devuelve el padrón. Tests en `tests/attendance.rs`.

## T4. `listar_padron` + `padron.sql`. RF-13, RF-14 [x]
- Hecho cuando: `GET` devuelve activos ∪ con fila, orden `lower(last_name),
  lower(first_name), id`, `state` o `null`; evento inexistente → 404. Test de
  integración.

## T5. `corregir_asistencia` idempotente. RF-8..RF-12, RF-15 [x]
- Hecho cuando: PATCH cambia solo `state`; sin fila → 404; estado igual → 200
  sin auditoría; con cambio → auditoría con anterior/nuevo. Tests.

## T6. Handler y rutas alineadas. RF-1, RF-8, RF-13, RF-14 [x]
- Hecho cuando: `POST` responde 200 con padrón; `GET` responde 200; `PATCH
  /events/{id_event}/attendance/{id_student}` responde 200 con la fila; el
  `DELETE` responde 405. `tests/http_attendance.rs` actualizado y verde.

## T7. Defensa de base. RF-11, RF-12 [x]
- Hecho cuando: `tests/conflictos_bd.rs` prueba por SQL directo que no se
  registra asistencia en `CANCELLED` (regla 7) y que no se puede borrar ni
  cambiar de identidad una fila (regla 10).

## T8. [x] Cierre de la fase
- Hecho cuando: los RF-1..RF-16 recorridos y marcados en la spec/tasks;
  `cargo test` verde, `cargo clippy --all-targets -D warnings` 0/0;
  `MEMORY.md` actualizado; spec en `implementada`; commit de cierre.
