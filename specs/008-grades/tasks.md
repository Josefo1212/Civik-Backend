# Tasks 008 — Grades

Estado: en implementación (reconciliación). Una tarea a la vez: test primero,
`cargo test` verde, `cargo clippy` 0/0, marcar y parar.

## T0
- [x] spec en formato SDD, dudas resueltas, estado `aprobada`
- [x] plan y tasks

## T1. Dominio puro. RF-1, RF-2, RF-5, RF-6, RF-7, RF-8
- Hecho cuando: `domain.rs` define `Conteos`, `Nota`, `compute_grade` y
  `resolver_nota`; los pesos viven en un único lugar; `compute_grade` aplica
  `max(0, 20 − Σ)` con redondeo a un decimal y `N=0 → SinNota`; `resolver_nota`
  deja que el override prevalezca sin redondearlo. Tests unitarios en `src`.

## T2. Puerto y consultas de elegibilidad. RF-3, RF-4, RNF-4, RNF-5
- Hecho cuando: `ports.rs` define `GradeRepository`; `adapter_pg.rs` lo
  implementa con `existe_lapso`, `existe_seccion`, `n_eventos_elegibles` y
  `ausentes_por_estudiante`; los `.sql` aplican `SCHEDULED` +
  (`event_date <= CURRENT_DATE` salvo lapso `CLOSED`) y solo cuentan `AUSENTE`.

## T3. Roster con override. RF-9, RF-12
- Hecho cuando: `roster.sql` devuelve activos de la sección ∪ con asistencia en
  el lapso, con `grade_overrides` vía `LEFT JOIN`, en orden `lower(last_name),
  lower(first_name), id`; los estudiantes sin asistencia salen (N se decide
  aparte).

## T4. Service. RF-5, RF-6, RF-12, RF-13
- Hecho cuando: `service.rs` comprueba existencia (404), trae N, conteos y
  roster, y resuelve la nota por estudiante con `resolver_nota`; el override
  gana; recalcula en cada llamada (no persiste).

## T5. Test del service con repositorio falso. RNF-1, RNF-6
- Hecho cuando: `tests/grades.rs` implementa un `GradeRepository` en memoria y
  verifica el armado de la respuesta (override gana, `SinNota` → `null`) **sin**
  Postgres.

## T6. Endpoint. RF-9, RF-10, RF-11
- Hecho cuando: `GET /lapses/{id_lapse}/sections/{id_section}/grades` responde
  200 con el arreglo ordenado y `nota` string o `null`; lapso o sección
  inexistentes → 404; se retira el endpoint por estudiante.

## T7. Integración. RF-3, RF-4, RF-8, RF-9, RF-13
- Hecho cuando: `tests/grades.rs` (tx/rollback) prueba evento futuro que no
  descuenta con lapso abierto y sí con `CLOSED`; override con 2 decimales gana;
  inactivo con asistencia aparece; nota se recalcula al corregir asistencia.

## T8. Bordes HTTP. RF-10, RF-11
- Hecho cuando: `tests/http_grades.rs` (oneshot) cubre 200 (forma del JSON), 404
  de lapso y de sección, con residuo purgado al arrancar.

## T9. Cierre de la fase
- Hecho cuando: RF-1..RF-13 recorridos y marcados; `cargo test` verde,
  `cargo clippy --all-targets -D warnings` 0/0; `MEMORY.md` actualizado; spec en
  `implementada`; commit de cierre.
