# Tareas 011 — Listado global de estudiantes y secciones inactivas

- [x] **T1. `students::queries::listar_todos` + `list_all.sql`.** RF-1, RF-2
  - Hecho cuando: `listar_todos(&mut tx, false)` devuelve solo activos y
    `listar_todos(&mut tx, true)` incluye los inactivos marcados, con el orden
    canónico. Test en `tests/students.rs` con rollback.
- [x] **T2. Ruta `GET /students` y helper de parseo.** RF-1, RF-2, RF-3, RF-7,
  RF-8
  - Hecho cuando: `students::handler::listar_todos` responde `200` con la
    lista y `400` con `incluir_inactivas=si`; el helper de parseo es el único
    punto que convierte el texto a `bool` (reusado por el padrón por sección).
- [x] **T3. `sections::queries::listar` con `incluir_inactivas`.** RF-4, RF-5
  - Hecho cuando: `list.sql` reemplaza a `list_active.sql` y
    `listar(pool, grado, incluir_inactivas)` incluye o excluye la sección dada
    de baja; se actualizan las llamadas en `tests/sections.rs`.
- [x] **T4. `GET /sections?incluir_inactivas`.** RF-4, RF-5, RF-6, RF-8
  - Hecho cuando: el handler de `sections` lee la bandera con el helper de T2
    y responde `400` con un valor inválido; test de borde HTTP.
- [x] **T5. Cierre.** RF-1 a RF-8
  - Hecho cuando: spec `implementada`, `tasks.md` en `[x]`, `MEMORY.md` y
    `docs/bitacora.md` al día; suite completa verde, `clippy --all-targets --
    -D warnings` en 0/0 y `cargo fmt --check` limpio.
