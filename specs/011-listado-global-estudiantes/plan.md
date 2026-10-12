# Plan 011 — Listado global de estudiantes y secciones inactivas

## Nivel de hexagonalidad

`sections` y `students` son **CRUD directo** (tabla de `AGENTS.md`): no hay
regla de negocio nueva, solo lecturas con un filtro más. Handler → `queries` →
`.sql`, sin `service.rs` ni `domain.rs` nuevos. El parseo del booleano
`incluir_inactivas` no es regla de dominio: se queda en el handler, como ya
está en `students::handler`.

## Archivos que se crean o modifican

| Archivo | Cambio | RF |
|---|---|---|
| `src/modules/students/queries/list_all.sql` | **Nuevo.** Listado global con la misma forma y orden que `list_by_section.sql`, sin el filtro por sección | RF-1, RF-2 |
| `src/modules/students/queries.rs` | Nueva `listar_todos(pool, incluir_inactivas)` | RF-1, RF-2 |
| `src/modules/students/handler.rs` | Nuevo handler `listar_todos`; se extrae el parseo del booleano a un helper `leer_incluir_inactivas` reusado por `listar` y `listar_todos` | RF-1, RF-2, RF-3, RF-8 |
| `src/app.rs` | Ruta `GET /students` | RF-1 |
| `src/modules/sections/queries/list.sql` | **Renombrado** desde `list_active.sql` y ampliado con `$3::boolean` para incluir inactivas | RF-4, RF-5 |
| `src/modules/sections/queries.rs` | `listar_activas` → `listar`, con parámetro `incluir_inactivas` | RF-4, RF-5 |
| `src/modules/sections/handler.rs` | `FiltroGrado` + `incluir_inactivas`; parseo con el mismo helper de `students` | RF-4 a RF-6, RF-8 |
| `tests/students.rs` | Casos de `listar_todos` | RF-1, RF-2, RF-8 |
| `tests/sections.rs` | Ajuste de las llamadas a `listar`; caso de inactivas | RF-4, RF-5 |
| `tests/http_students.rs` | `GET /students` (200, forma, 400) | RF-1, RF-2, RF-3, RF-7 |
| `tests/http_sections.rs` | `GET /sections?incluir_inactivas=true` | RF-4, RF-5, RF-6 |

## Queries `.sql`

`list_all.sql` (students) replica `list_by_section.sql` quitando el
`WHERE id_section = $1`; conserva `($1::boolean OR active)` y el `ORDER BY`
canónico.

`list.sql` (sections) es `list_active.sql` con `WHERE ($3::boolean OR active)`
en lugar del `WHERE active` fijo. El `ORDER BY` y el filtro por grado no
cambian.

## Decisiones técnicas

- **`incluir_inactivas` como `bool` en SQL, no dos queries.** Un solo `.sql`
  con `($n::boolean OR active)` evita duplicar el `ORDER BY` canónico (una
  sola fuente de verdad del orden). Alternativa descartada: dos queries
  separadas, que duplicarían el orden y podrían divergir.
- **El parseo `String` → `bool` se extrae a un helper.** Hoy vive dentro de
  `students::handler::listar`. Al sumar dos usos más (listado global y
  `sections`), un helper compartido evita tres copias del mismo match.
  Alternativa descartada: `Option<bool>` de serde, que responde `400` pero
  rompe el mensaje en español que ya se validó.
- **`list_active.sql` → `list.sql`.** El nombre viejo afirmaba "solo activas",
  algo que deja de ser cierto con la bandera. Renombrar el `.sql` (no es una
  migración; la regla de "no renombrar" aplica a `db/migrations/`) mantiene el
  nombre honesto.
- **Sin cambios de esquema.** El `WHERE active` ya era consultable; solo se
  expone.

## Estrategia de tests

Patrón de `testing-transaccional` (skill): transacción + `rollback`, filtrar
las filas propias del test por `id` porque la base es compartida.

- `tests/students.rs`: `listar_todos` devuelve solo activos por defecto e
  incluye los inactivos con la bandera; dos llamadas dan el mismo orden.
- `tests/sections.rs`: `listar(..., true)` incluye la sección dada de baja;
  `listar(..., false)` no (se ajustan las dos llamadas existentes).
- HTTP (`oneshot`, commitea con prefijo UUID y limpia al final):
  `GET /students?incluir_inactivas=true` la ve marcada; `=si` → `400`;
  `GET /sections?incluir_inactivas=true` la ve marcada.
