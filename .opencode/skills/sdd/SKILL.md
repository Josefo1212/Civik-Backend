---
name: sdd
description: Úsala siempre que trabajes con Spec-Driven Development en Civik (docs/constitution.md o cualquier archivo dentro de specs/) - redactar, revisar o cambiar specs, planes y tareas, o implementar y validar tareas de una spec.
---

# Spec-Driven Development (SDD) — Civik

## Flujo
Constitución → Spec → Clarificación → Plan → Tareas → Implementación → Validación → Cambio.

- Nunca pases a la siguiente fase sin la aprobación explícita del usuario.
- La spec manda: si algo no está en la spec, no se implementa. Si falta una
  decisión, para y pregunta.
- Un cambio de requisitos se hace primero en la spec, luego en el plan y las
  tareas, y por último en el código.
- Cada spec vive en su carpeta: `specs/NNN-nombre/` con `spec.md`, `plan.md`
  y `tasks.md`. `NNN` sigue el número de fase de la tabla "Orden de
  construcción" de `AGENTS.md` cuando aplica (ej. `002-sections`,
  `006-events-auto`).
- Al terminar cada fase, actualiza `MEMORY.md`.
- Toda spec, plan y tarea debe respetar `docs/constitution.md` y, si toca
  dominio, la skill `civik-dominio`.

## Plantilla de spec (spec.md)
```
# Spec NNN — <Nombre>

Estado: borrador | aprobada | implementada

## Contexto y objetivo
## Usuarios
## Historias de usuario
- HU-1. Como <rol>, quiero <acción> para <beneficio>.
## Definiciones (solo si hay términos que puedan interpretarse de varias formas)
## Requisitos funcionales
## Requisitos no funcionales
## Casos límite
## Fuera de alcance
## Criterios de finalización
## Dudas abiertas
- [NECESITA ACLARACIÓN] <duda>
```
La spec describe el QUÉ y el POR QUÉ. Nada de stack, arquitectura ni
nombres de archivos — eso va en el plan.

## Requisitos en EARS (en español)
- RF-x: CUANDO <evento>, EL SISTEMA <respuesta>.
- RF-x: SI <condición no deseada>, ENTONCES EL SISTEMA <respuesta>.
- RF-x: MIENTRAS <estado>, EL SISTEMA <respuesta>.
- RF-x: EL SISTEMA <comportamiento permanente>.

Cada RF debe ser verificable con un test de integración (patrón
transacción/rollback, ver skill `testing-transaccional`) o, si es un borde
HTTP, con una llamada real al endpoint. Nada de "rápido", "robusto" o
"bien" sin un criterio medible.

## Plan (plan.md)
Archivos que se crean/modifican y su responsabilidad · en qué nivel de
hexagonalidad cae el módulo (CRUD directo / parcial / completo, según la
tabla de `AGENTS.md`) · funciones puras de dominio necesarias · las queries
`.sql` necesarias (una por archivo) · decisiones técnicas justificadas (con
su alternativa descartada) · estrategia de tests con el patrón de
`testing-transaccional`. Indica qué RF cubre cada parte.

## Tareas (tasks.md)
```
- [ ] **Tn. <Descripción>.** RF-x, RF-y
  - Hecho cuando: <comprobación verificable>.
```
Máximo 20-30 min por tarea, en orden de dependencia. Si salen más de 10,
propón dividir la spec.

## Implementación
Una sola tarea cada vez: test primero (patrón transacción/rollback,
comprobar que falla), después el código, `cargo test` en verde, `cargo
clippy` sin warnings, marcar la tarea y PARAR. No empezar la siguiente.

## Validación
Recorre la spec RF por RF. Para un RF de dominio, el test de integración es
la prueba. Para un RF de borde HTTP (status code, forma del JSON) que no
quede cubierto por un test, verificalo con una llamada real al endpoint
(`cargo run` + `curl`/`httpie`). Civik es un backend puro: no hay interfaz
que probar acá — eso es `Civik-Frontend`, otro repositorio.
