# Plan 006 — Events auto + reprogramación

## Decisiones
- domain.rs: reglas puras (puede_generarse, fechas válidas, solapamientos) 
- service.rs: `generar_practicas_auto(tx|pool, id_lapse)` y `reprogramar_practice_day(tx|pool, id_lapse, nuevo_dia)` con tx propia, auditoría
- queries.rs: helpers para obtener eventos existentes, crear AUTO, mover fechas
- Tests integración tx/rollback

## Rutas (futuras)
- POST /events/generate-auto
- POST /events/reprogram

## RF→tests
- generar crea eventos AUTO en practice_day
- no duplica si ya existen
- reprograma eventos pendientes sin asistencia
