# Spec 009 — Sincronización offline (attendance)

Estado: aprobada

> Avance: T0/T1 hechos (reconocimiento de `updated_at` y tests base de
> idempotencia/corrección/HTTP). El protocolo de conflicto en sí —detección
> por `updated_at`, tolerancia de reloj y contrato idempotente de red— aún
> no está implementado ni especificado en detalle; esta spec debe ampliarse
> antes de codificar.

## Objetivo
Implementar protocolo de conflicto sobre attendance (idempotencia, updated_at, tolerancia de reloj). Incremento sobre Fase 7.

## Conceptos
- Idempotencia en operaciones batch/patch
- updated_at para detectar conflictos (versionado temporal)
- Tolerancia de reloj
- Tests de integración con tx/rollback
