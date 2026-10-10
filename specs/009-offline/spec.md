# Spec 009 — Sincronización offline (attendance)

Estado: implementada

## Objetivo
Implementar protocolo de conflicto sobre attendance (idempotencia, updated_at, tolerancia de reloj). Incremento sobre Fase 7.

## Conceptos
- Idempotencia en operaciones batch/patch
- updated_at para detectar conflictos (versionado temporal)
- Tolerancia de reloj
- Tests de integración con tx/rollback
