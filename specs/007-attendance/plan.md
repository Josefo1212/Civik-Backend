# Plan 007 — Attendance (registrar/corregir)

## Decisiones
- Módulo hexagonal parcial (domain + service + queries + handler). No traits para CRUD básico.
- Modelo existente: attendance (id_event,id_student) estado enum. 
- Reglas: corregir = cambiar state (regla 10), no borrar. No asistencia en CANCELLED (regla 7).
- Transacción para pasar lista en lote (atomicidad parcial). Auditoría requerida.
- Tests integración tx/rollback.

## RF→test
- registrar asistencia en lote
- corregir estado (PUT/PATCH) cambia state, no borra fila
- rechaza evento CANCELLED → 409
- evento con asistencia afecta ocurrencia (regla 5) — ya cubierta por BD, validamos
