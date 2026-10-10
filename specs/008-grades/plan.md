# Plan 008 — Grades

## Arquitectura
hexagonal completo: domain/compute_grade (puro), ports.rs (trait), adapter_pg.rs (sqlx), service.rs (orquestación), handler.rs (HTTP), mod.rs

## Decisiones
- Nunca f64 para notas (usar rust_decimal con feature sqlx). Redondeo centralizado.
- Repositorio trae conteos agregados; dominio calcula.
