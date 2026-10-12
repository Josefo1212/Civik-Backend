# MEMORY.md — Civik Backend

Memoria del proyecto entre sesiones. Es un **snapshot corto** (≈50 líneas).
Las reglas permanentes viven en otro lado (ver abajo), no acá. Al convertirse
algo en regla permanente, se mueve a su documento y se quita de este archivo.

## Estado actual

- Rust + axum + sqlx sobre PostgreSQL 18, hexagonal proporcional.
- **Fases 0–10 del "Orden de construcción" completas**: bootstrap, `shared`,
  `sections`, `students`, `lapses`, `events` (manual + automático),
  `attendance`, `grades`, `offline` y endurecer tests (invariantes de base).
- Specs 002–011 en `specs/` con `Estado: implementada`.
- Suite: **339 tests** verdes; `cargo clippy --all-targets -- -D warnings` en
  0/0; `cargo fmt --check` limpio.
- Spec 011 (endpoint pendiente del front): `GET /students` global con
  `incluir_inactivas` y `GET /sections?incluir_inactivas`, ya implementados.

## Próximo

- **Fase 11** — Deploy: Dockerfile + `compose.yaml`, al final.

## Pendientes menores

- Limpiar los `dead_code` de `shared/*` al cerrar la fase en curso.

## Dónde vive cada cosa

| Tema | Documento |
|---|---|
| Arquitectura, stack, convenciones, decisiones y aprendizajes | `AGENTS.md` |
| Constitución SDD | `docs/constitution.md` |
| Specs / planes / tareas | `specs/NNN-*/` |
| Historia fase por fase | `docs/bitacora.md` |
| Fórmula de notas y reglas de dominio | skill `civik-dominio` |
| Patrón de test transaccional y aislamiento | skill `testing-transaccional` |
