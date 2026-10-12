# Spec 011 — Listado global de estudiantes y secciones inactivas

Estado: implementada

> Avance: T1–T5 hechos. `GET /students` (global, con `incluir_inactivas`) y
> `GET /sections?incluir_inactivas` implementados; el parseo del booleano se
> centralizó en `shared::filtro`. Tests: 2 en `tests/students.rs`, 1 en
> `tests/sections.rs`, 1 en `tests/http_students.rs` y 1 en
> `tests/http_sections.rs`. Suite completa **339 tests** en verde; `cargo
> clippy --all-targets -- -D warnings` 0/0; `cargo fmt --check` limpio.

## Contexto y objetivo

La PWA (otro repositorio) tiene una vista "todos juntos" que hoy no puede
armar: el único listado de estudiantes es `GET /sections/{id}/students`
(padrón por sección) y `GET /sections` devuelve únicamente las activas. Sin un
listado global, el front no puede reunir a todos los estudiantes, ni alcanzar
los de secciones dadas de baja en una sesión anterior.

Objetivo: exponer un listado global de estudiantes y permitir que los
listados de estudiantes y secciones incluyan las filas dadas de baja, para
que el instructor pueda verlas y reactivarlas.

## Usuarios

- **Instructor** (único usuario final): consulta el padrón completo y las
  secciones dadas de baja desde la PWA.

## Historias de usuario

- HU-1. Como instructor, quiero ver a todos los estudiantes juntos para tener
  el padrón completo sin recorrer sección por sección.
- HU-2. Como instructor, quiero poder incluir o excluir a los estudiantes y
  secciones dados de baja, para reactivar los que di de baja por error.

## Requisitos funcionales

- RF-1: CUANDO el cliente pide `GET /students`, EL SISTEMA responde `200` con
  todos los estudiantes **activos** de todas las secciones, en el orden
  canónico `lower(last_name), lower(first_name), id`.
- RF-2: CUANDO `GET /students?incluir_inactivas=true`, EL SISTEMA incluye
  además a los estudiantes inactivos, cada uno con su `active` en `false`.
- RF-3: SI `incluir_inactivas` en `GET /students` trae un valor distinto de
  `true` o `false`, ENTONCES EL SISTEMA responde `400` con un mensaje que
  nombra el parámetro y el valor recibido.
- RF-4: CUANDO `GET /sections`, EL SISTEMA responde `200` solo con las
  secciones **activas** (comportamiento actual, sin regresión), combinable con
  el filtro opcional `grado`.
- RF-5: CUANDO `GET /sections?incluir_inactivas=true`, EL SISTEMA incluye
  también las secciones dadas de baja, cada una con su `active` en `false`,
  manteniendo el orden canónico y el filtro por grado.
- RF-6: SI `incluir_inactivas` en `GET /sections` trae un valor distinto de
  `true` o `false`, ENTONCES EL SISTEMA responde `400` con el mismo criterio
  de RF-3.
- RF-7: LA forma de cada estudiante y de cada sección en la respuesta es
  idéntica a la que ya devuelven los endpoints existentes (mismos campos,
  `snake_case`, sin campos nuevos).
- RF-8: CUANDO el cliente usa `incluir_inactivas=false` de forma explícita,
  EL SISTEMA se comporta igual que si el parámetro no estuviera (solo
  activos).

## Requisitos no funcionales

- RNF-1: el filtrado (activo/inactivo) ocurre en el servidor, en SQL; el
  cliente no reprocesa la lista.
- RNF-2: el listado global y el de secciones conservan el orden canónico
  determinista ya establecido (segunda llamada idéntica), para que todos los
  dispositivos muestren el mismo orden.

## Casos límite

- `GET /students` sin estudiantes que cumplan el filtro → `200` con `[]`.
- Una sección inactiva cuyos estudiantes activos deben seguir apareciendo en
  `GET /students` (el estado del estudiante manda, no el de su sección).

## Fuera de alcance

- Filtrar `GET /students` por sección, grado o turno (el front solo necesita
  "todos juntos").
- Paginación de cualquier listado.
- Cualquier cambio de esquema: no se toca la base.

## Criterios de finalización

- Tests de integración (patrón transacción/rollback) que cubran RF-1, RF-2,
  RF-4, RF-5 y RF-8, filtrados a las filas propias del test.
- Tests de borde HTTP (`oneshot`) que cubran el `200` y la forma del JSON, y
  el `400` de RF-3/RF-6.
- Suite completa en verde, `cargo clippy --all-targets -- -D warnings` en 0/0
  y `cargo fmt --check` limpio.

## Dudas abiertas

Ninguna. Decisión tomada: se usa `incluir_inactivas` (mismo nombre y mismos
valores que el padrón por sección) en vez de `GET /sections?estado=`, para que
el front tenga un solo parámetro que recordar.
