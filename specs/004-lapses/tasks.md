# Tareas 004 — Lapsos

En orden de dependencia. Cada tarea se cierra con un commit y deja la base
limpia (todo lo que crean los tests de integración se descarta con el
`rollback`; los tests HTTP usan datos únicos y limpian el residuo cuando el
guardarraíl lo permite — ver T7).

## Tareas

- [ ] **T1. `domain.rs`: entrada y transiciones de estado.** RF-1, RF-2, RF-8,
  RF-9, RF-12, RF-14, RF-15, RF-16, RF-17, RF-18
  - `DiaSemana`, `EstadoLapso`, `DatosLapso`, `preparar_entrada` (año
    2000–2100, número 1–3, fechas `NaiveDate` e invertidas, día de la semana
    case-insensitive), `validar_transicion` (Activar/PLANNED, Cerrar/ACTIVE,
    Editar/no-CLOSED, Eliminar/no-CLOSED-y-no-terminado) y `ErrorDominio`.
  - Unit tests sin base de datos: parseo del día, rango del año, número
    fuera de 1–3, fechas invertidas y mal formadas, y cada transición válida
    e inválida.
  - Hecho cuando: `cargo test` en verde solo con estos unit tests,
    `cargo clippy --all-targets -- -D warnings` limpio.

- [ ] **T2. `mapear_violacion`: los cinco arms de $5 del plan.** RF-3, RF-4,
  RF-10, RF-13, RF-18
  - `(23505, uq_lapses_year_number)`, `(23P01, lapses_no_overlap)`,
    `(23505, uq_lapses_single_active)`, `(23001|23503, events_id_lapse_fkey)`,
    `(23001|23503, grade_overrides_id_lapse_fkey)` → `Conflict` con mensaje
    en español; docstring actualizado.
  - Unit tests: cada arm mapea, y `23P01` sin su restricción no mapea.
  - Hecho cuando: clippy limpio y suite completa en verde.

- [ ] **T3. Queries y SQL de lectura/escritura.** RF-1, RF-5, RF-6, RF-7,
  RF-8, RF-10, RF-12, RF-15, RF-19
  - `create.sql`, `get_by_id.sql`, `list.sql`, `update.sql`,
    `set_status.sql`, `events_outside.sql`, `delete.sql`; `queries.rs` con
    `Lapso`, `FilaLapso`+`en_lapso`, y las funciones que además escriben la
    auditoría (alta/editar/activar/cerrar/eliminar) en la misma transacción.
  - Patrón de enums `$n::text::week_day` / `::text::lapse_status` y lecturas
    `practice_day::text AS practice_day` (ver plan §5).
  - Hecho cuando: compila y `cargo clippy` limpio (los tests de T6 y T7
    ejercitan estas queries de verdad).

- [ ] **T4. `service.rs`: orquestación con transacción.** RF-8, RF-9, RF-11,
  RF-12, RF-13, RF-14, RF-15, RF-16, RF-17, RF-18
  - `crear`, `listar`, `consultar`, `editar` (con pre-chequeo de
    `events_outside.sql`), `activar`, `cerrar`, `eliminar` (con la
    distinción 404 vs 409 apoyada en `get_by_id` tras un `UPDATE` sin filas).
  - Cada escritura: `pool.begin()` → queries (+auditoría) → `commit`.
  - Hecho cuando: compila y clippy limpio.

- [ ] **T5. `handler.rs` + rutas en `app.rs` + `mod.rs`.** RF-1…RF-18 (superficie)
  - `EntradaLapso` con los cinco campos como `String` (el dominio valida);
    verbo PATCH para editar; `POST /lapses/{id}/activar` y
    `POST /lapses/{id}/cerrar`; `DELETE /lapses/{id}` → 204.
  - Rutas en `app.rs` con el patrón `{id}` de axum 0.8; `pub mod lapses`.
  - Hecho cuando: compila, clippy limpio, `cargo test` completo en verde.

- [ ] **T6. `tests/lapses.rs`: integración transacción/rollback.** RF-1…RF-19
  - Los casos del mapa del plan §9 (alta, duplicados, solape, listado y su
    orden estable, consultas 404, edición que no toca el estado, edición
    CLOSED, edición hacia combinación ajena y hacia solape, fechas vs
    evento, activación/segundo activo/no planeado, cierre/no activo, los
    tres rechazos de borrado, auditoría de las cinco operaciones).
  - Fechas arbitrarias (sin lapso real en `civik_dev`); defender con las
    filas crudas de la base, no solo con lo que devuelven las funciones.
  - Hecho cuando: todos en verde y `cargo clippy` limpio.

- [ ] **T7. `tests/http_lapses.rs`: bordes HTTP con `oneshot`.** RF-1…RF-18
  (superficie)
  - `school_year` único derivado de UUID, fechas con fin muy futuro (ej.
    2045) y limpieza con `DELETE` — que nunca caiga en el guardarraíl.
  - Cierra el feliz de `cerrar` y los 409 sobre `CLOSED` NO (residuo
    indeletable; ver plan §8.4): esos se prueban en T6. En HTTP: alta 201
    + normalización, los 400 de entrada, duplicado/solape 409, listado y
    vacío, consulta 200/404, editar 200 y duplicado 409 e inexistente 404,
    activar 200, segundo activo 409, activar un activo 409, cerrar un
    planeado 409, eliminar 204 e inexistente 404.
  - Hecho cuando: todos en verde, clippy limpio y `civik_dev` termina sin
    filas nuevas de lapsos.

- [ ] **T8. `tests/conflictos_bd.rs`: defensa en profundidad con SQL directo.**
  RF-4, RF-9, RF-11, RF-13, RF-18
  - `UPDATE` directo de un lapso `CLOSED` → 23514; `DELETE` de un lapso
    `CLOSED` y de uno terminado → 23514; segundo `ACTIVE` directo → 23505 /
    `uq_lapses_single_active`; solape directo → 23P01; mover fechas con un
    evento fuera → 23514; `DELETE` de un lapso con un evento insertado por
    SQL → 23001 / `events_id_lapse_fkey`.
  - Hecho cuando: todos en verde (SAVEPOINT donde la transacción quede
    aborted) y clippy limpio.

- [ ] **T9. Validación RF por RF, llamada real y cierre de la fase.** RF-1…RF-19
  - Tabla RF → test → veredicto en este archivo; bordes confirmados con
    `cargo run` + `curl` sobre los siete endpoints (sin cerrar un lapso:
    residuo indeletable); spec 004 con `Estado: implementada`; `MEMORY.md`
    al día (Fase 4 cerrada, próxima Fase 5); commit de cierre.
  - Hecho cuando: suite completa en verde, clippy 0/0, `curl` confirmado y
    `MEMORY.md` actualizado.

## Notas de ejecución

- **La única fase del plan con `service.rs` antes del handler**: T4 precede a
  T5 porque el service es donde vive la secuencia de negocio que el handler
  solo delega; el handler no decide nada.
- **Residuo de HTTP y el `CLOSED`**: el guardarraíl `lapses_delete_guard`
  impide borrar un lapso ya cerrado o terminado. Por eso el camino feliz de
  `cerrar` y los 409 sobre `CLOSED` se prueban solo en integración (T6), y
  los HTTP de T7 nunca cierran un lapso ni lo dejan sin limpiar. Si una
  corrida deja una fila, es un error del test, no de la base.
- **Fechas sonoras**: ningún test HTTP usa fechas cerca de hoy; `end_date`
  siempre muy futuro para que la limpieza por `DELETE` nunca sea bloqueada.
- **Defensa vs superficie**: los triggers (`trg_lapses_closed_frozen`,
  `trg_lapses_dates_against_events`, `lapses_delete_guard`) no se mapean en
  la API; se prueban con SQL directo en T8 como invariantes que sobreviven a
  un bug del backend.