# Tasks 007 — Attendance

## T0
- [x] spec aprobada → implementada
- [x] plan, tasks completos

## T1 domain.rs
- [ ] enum AttendanceState, validaciones puras

## T2 queries.rs + SQL
- [ ] list_by_event, set_state, insert_batch

## T3 service.rs
- [ ] registrar_listado(tx/lote)
- [ ] corregir_asistencia
- [ ] auditoría

## T4 tests/attendance.rs (tx/rollback)
- [ ] registra lote OK
- [ ] corrige estado sin borrar
- [ ] rechaza CANCELLED (409)

## T5 HTTP handlers/tests
- [ ] endpoints, tests oneshot

## T6 cerrar fase 7
