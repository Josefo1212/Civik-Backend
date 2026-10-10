# Spec 006 — Generación automática y reprogramación

## Objetivo
Portar a dominio de Rust (events::service + domain) lo que antes eran `generar_practicas_auto` y `reprogramar_practice_day` en PL/pgSQL, usando `sqlx::Transaction` para atomicidad. Las invariantes siguen protegidas por triggers/constraints BD.

## RF (requeridos)
- RF-14: Generar prácticas automáticas para un lapso (source='AUTO', type='PRACTICA_REGULAR') solo si lapso está OPEN/PLANEADO según reglas (no CLOSED). No duplicar prácticas ya existentes para ese (lapso, fecha).
- RF-15: Reprogramar un día de práctica (practice_day) del lapso: mueve eventos AUTO/PRACTICA_REGULAR pendientes? según reglas de dominio (skill civik-dominio). Respeta que eventos con asistencia no cambian; eventos que ya ocurrieron/congelados se tratan según reglas.
- RF-16: Operaciones son atómicas (transacción). Auditoría requerida.
- RF-17: Validaciones de negocio puras en domain, orquestación en service.
- RF-18: Respeta restricciones BD existentes.

## RNF
- Hexagonal parcial (mismo patrón events).
- Transacción/rollback en tests integración.
- Errores con código semántico (409/400/404).

## Estado: implementada
implementada
