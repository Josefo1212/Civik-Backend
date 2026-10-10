# Spec 008 — Grades (hexagonal completo)

Estado: implementada

## Objetivo
Implementar módulo grades con arquitectura hexagonal completa: domain puro (compute_grade), ports (trait GradeRepository), adapter_pg (sqlx), service, handler. Calcula nota sobre 20 según fórmula acordada.

## Fórmula (resumen)
- Base 20. valor = 20 - Σ(pesos AUSENTE). Mínimo 0.
- Pesos: PRACTICA_REGULAR=1, PRACTICA_EXTRA=0.5, PRESENTACION=2
- Solo AUSENTE penaliza
- N: eventos SCHEDULED con event_date <= hoy mientras lapso no CLOSED; al cerrar cuenta todos SCHEDULED
- Override (estudiante,lapso) prevalece
