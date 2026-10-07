# Tareas 003 — Estudiantes

En orden de dependencia. Cada tarea se cierra con un commit y deja la base
limpia (todo lo que crean los tests se descarta con el `rollback`).

## Tareas

- [x] **T1. `domain.rs`: normalización y validación de la entrada.** RF-2,
  RF-6, RF-7, RF-12, RF-13
  - `colapsar_espacios`, `validar_no_vacio`, `validar_largo`,
    `preparar_entrada` y `ErrorEntrada`, puras.
  - Unit tests sin base de datos: colapso de blancos, capitalización
    intacta, nombres vacíos rechazados, largos, cédula/teléfono `""`→`None`,
    y que `preparar_entrada` es el mismo camino para alta y edición.
  - Hecho cuando: `cargo test` en verde solo con estos unit tests,
    `cargo clippy --all-targets -- -D warnings` limpio.

- [x] **T2. Infra de errores: `NotFound(String)` y la cédula como `Conflict`.**
  RF-3, RF-5
  - `src/shared/error.rs`: `AppError::NotFound(String)`; arm
    `(23505, students_national_id_key)` → `Conflict("Ya existe un estudiante
    con esa cédula")`; `mapear_violacion` pasa a `pub(crate)`.
  - `src/modules/sections/handler.rs`: `a_not_found` →
    `NotFound("No se encontró la sección")`.
  - Unit tests nuevos en `error.rs` para el arm de cédula y el `404` con
    mensaje; ajustar los existentes.
  - Hecho cuando: clippy limpio y suite completa en verde (las pruebas de la
    Fase 2 adaptadas).

- [ ] **T3. `create.sql` + `crear`.** RF-1, RF-3, RF-4, RF-5, RF-21
  - `ErrorEstudiante` en `queries.rs`, `verificar_seccion` (existe + activa)
    dentro de la misma tx, `create.sql` con `RETURNING`, auditoría "alta" en
    la misma transacción (RNF-2).
  - Tests integración (patrón transacción/rollback): el alta persiste y
    audita; sección inexistente → `SeccionNoExiste`; sección inactiva →
    `SeccionInactiva`; cédula duplicada → `Conflict` (también contra un
    estudiante inactivo; la cédula no se libera); cédula vacía → `null` y
    dos estudiantes sin cédula conviven.
  - `tests/conflictos_bd.rs`: INSERT duplicado directo → la restricción es
    `students_national_id_key` y se traduce a `Conflict`.
  - Hecho cuando: suite en verde y clippy limpio.

- [ ] **T4. `get_by_id.sql` + `list_by_section.sql`.** RF-8, RF-9, RF-10,
  RF-11
  - `consultar` y `listar_por_seccion(id_seccion, incluir_inactivas)`; la
    lista valida solo existencia de la sección (sección inactiva se lista,
    inexistente → `SeccionNoExiste`).
  - Tests: consulta de activo e inactivo; id inexistente; listado por
    defecto solo activos en orden `lower(apellido), lower(nombre)` (y
    estable); `incluir_inactivas=true` incluye y marca; listado de una
    sección inactiva; listado de sección inexistente.
  - Hecho cuando: suite en verde y clippy limpio.

- [ ] **T5. `update.sql` + `editar`.** RF-12, RF-13, RF-14, RF-15, RF-16
  - Edición con la misma `preparar_entrada` del alta; mueve de sección solo
    si existe y está activa (segundo término de RF-16: el trigger, Fase 7);
    auditoría "editar" en la misma tx.
  - Tests: normaliza igual que el alta; vacíos rechazados; cédula ajena →
    `Conflict`; mover a sección inexistente/inactiva → 404/409; mover a
    sección activa funciona; editar sin cambiar de sección funciona.
  - Hecho cuando: suite en verde y clippy limpio.

- [ ] **T6. `set_active.sql` + baja/reactivación.** RF-17, RF-18, RF-19,
  RF-20, RF-21
  - Idempotencia real (`IS DISTINCT FROM`), auditoría solo si el estado
    cambió de verdad.
  - Tests: baja marca inactivo y lo omite del listado por defecto; bajar dos
    veces no modifica `updated_at` ni audita de más; reactivar lo devuelve
    al listado; reactivar dos veces no modifica nada; estudiante inexistente
    → `EstudianteNoExiste`.
  - Hecho cuando: suite en verde y clippy limpio.

- [ ] **T7. `handler.rs` + rutas en `app.rs` + tests `oneshot`.**
  - `crear`, `listar` (parámetro `incluir_inactivas` validado), `consultar`,
    `editar`, `baja`, `reactivar`; rutas según plan §7; sin ruta de DELETE
    (RF-22).
  - `tests/http_students.rs` (commitean; prefijos UUID únicos + limpieza con
    `DELETE` SQL directo): 201/200/400/404/409 en los seis endpoints, forma
    `snake_case` del JSON, `null` en `national_id`/`phone` cuando no hay.
  - Hecho cuando: suite completa en verde, clippy **0/0**, base `civik_dev`
    sin filas remanentes.

- [ ] **T8. Validación RF por RF, llamada real y cierre de la fase.** RF-16
  (marcar parcial), RF-22 (verificación por no-existencia)
  - Tabla RF → test → verificación en este archivo; bordes confirmados con
    `cargo run` + `curl` sobre los seis endpoints; spec 003 con `Estado:
    implementada`; `MEMORY.md` al día (Fase 3 cerrada, próxima Fase 4).
  - Hecho cuando: los 22 RF tienen veredicto registrado, el `curl` mostró
    los códigos esperados y la fase se da por implementada.

## Notas de ejecución

- **T1 no necesita base de datos** — como en la 002, es la tarea para hacer
  si el entorno está caído.
- **T2 toca código de la Fase 2** (`sections/handler.rs` y pruebas de
  `error.rs`): es un cambio deliberado del plan §8.1, con ajuste explícito
  de los usos existentes.
- **RF-16 tiene dos términos**: el primero (mover de sección sin asistencia)
  se prueba en T5; el segundo (la regla 3 con asistencia) lo garantiza el
  trigger `trg_students_section_immutable` de la init y se ejecutará recién
  en la Fase 7. No es una omisión.
- **RF-22 (sin borrado físico)** se verifica por ausencia: no hay ruta, no
  hay query `delete.sql`, y los tests nunca intentan borrar un estudiante.
- **Los tests HTTP commitean** (el handler commitea): por eso usan datos
  únicos y limpian con SQL directo al final, igual que `http_sections`.
- **Ninguna tarea deja datos en `civik_dev`**: si una corrida deja filas, es
  un error en el test, no en la base.