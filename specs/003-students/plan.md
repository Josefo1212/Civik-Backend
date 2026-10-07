# Plan 003 — Estudiantes

Plan de implementación de `specs/003-students/spec.md`. La spec manda: aquí
solo va el CÓMO, y cada decisión se justifica con la alternativa que se
descartó.

## 1. Qué se construye

El módulo `students` completo: alta individual dentro de una sección,
listado de una sección (activos por defecto, con opción de incluir
inactivos), consulta por identificador, edición (incluido el cambio de
sección), baja lógica y reactivación idempotentes. Sin eliminación física:
la constitución nº 3 no contempla excepción para estudiantes (a diferencia
de lapsos y secciones), y RF-22 lo fija en la spec.

No hay migración nueva: la tabla `students` ya existe en la init —incluidos
`active`, `uq_students_national_id`, `students_names_not_blank` y los dos
índices— y nada de esta spec la cambia.

## 2. Nivel de hexagonalidad

`AGENTS.md` clasifica `students` como **CRUD directo** y esta fase lo
mantiene: no hay secuencia de negocio multi-tabla que orquestar (eso vive en
`events` y `attendance`). El módulo queda así:

- `domain.rs`: normalización y validación de la entrada, puras, sin `async`
  ni `sqlx` — el mismo lugar que en `sections`. Esto **no** lo sube de
  nivel: es limpieza de entrada, no una capa intermedia (no hay `service.rs`,
  no hay puertos).
- `queries.rs`: funciones de acceso a datos, una por `.sql`, con la auditoría
  dentro de la misma transacción (RNF-2).
- `handler.rs`: capa fina, `State`/`Path`/`Query`/`Json`, códigos de estado.

## 3. Archivos

### Crear

| Archivo | Responsabilidad |
|---|---|
| `src/modules/students/mod.rs` | Módulo: `domain`, `queries`, `handler` |
| `src/modules/students/domain.rs` | `preparar_entrada`, `ErrorEntrada`: colapso de espacios, obligatorios, largos. Puro |
| `src/modules/students/handler.rs` | Handlers HTTP: `crear`, `listar`, `consultar`, `editar`, `baja`, `reactivar` |
| `src/modules/students/queries.rs` | Acceso a datos + `ErrorEstudiante` (sección inexistente/inactiva) |
| `src/modules/students/queries/create.sql` | RF-1, RF-4, RF-5 |
| `src/modules/students/queries/get_by_id.sql` | RF-10, RF-11 |
| `src/modules/students/queries/list_by_section.sql` | RF-8, RF-9, RNF-3 |
| `src/modules/students/queries/update.sql` | RF-12 a RF-16 |
| `src/modules/students/queries/set_active.sql` | RF-17 a RF-20 |
| `tests/students.rs` | Integración, patrón transacción/rollback |
| `tests/http_students.rs` | Bordes HTTP con `oneshot` (commiten; §8.7) |

### Modificar

| Archivo | Cambio |
|---|---|
| `src/modules/mod.rs` | Agregar `pub mod students;` |
| `src/shared/error.rs` | `AppError::NotFound` pasa a llevar mensaje (§8.1); nuevo arm en `mapear_violacion` para la cédula (§8.2) |
| `src/modules/sections/handler.rs` | `a_not_found` usa el nuevo `NotFound("No se encontró la sección")` |
| `src/app.rs` | Rutas de `students` (§7) |
| `tests/conflictos_bd.rs` | Cédula duplicada real → `students_national_id_key` (§8.2) |

## 4. Funciones puras de dominio (`domain.rs`)

| Función | RF |
|---|---|
| `colapsar_espacios(&str) -> String` | RF-6 |
| `validar_no_vacio(&str, &str) -> Result<(), ErrorEntrada>` | RF-2, RF-13 |
| `validar_largo(&str, usize, &str) -> Result<(), ErrorEntrada>` | caso límite de largos |
| `preparar_entrada(id_seccion, first, last, national_id, phone) -> Result<DatosEstudiante, ErrorEntrada>` | RF-2, RF-6, RF-7, RF-12, RF-13 |

`colapsar_espacios` usa `split_whitespace().join(" ")`: recorta los extremos
**y** colapsa cualquier runa de blancos (espacios, tabs, saltos) a un solo
espacio, sin tocar la capitalización (decisión de clarificación). Es distinta
de `sections::normalizar_texto` (que solo recorta extremos), por eso no se
comparte.

`preparar_entrada` es el punto único (RF-12: la edición normaliza y valida
igual que el alta). Orden: colapsar → no vacío → largo → opcionales
(el `None`/`""` de cédula y teléfono terminan en `None`; RF-7).

`ErrorEntrada` es un enum `thiserror` en español: `Obligatorio(String)` y
`LargoExcedido(String, usize)`.

## 5. Queries y su estructura

Cada `.sql` se carga con `query_file_as!` (constitución nº 1). Todas
devuelven el mismo conjunto de columnas; como ninguna expresión usa enums
de Postgres ni casts (a diferencia de `shift` en `sections`), un solo
struct `Estudiante` (`FromRow` + `Serialize`) sirve para todo:

```
id, id_section, national_id|Option, first_name, last_name, phone|Option,
active, created_at, updated_at
```

```sql
-- create.sql                              RF-1, RF-4, RF-5
INSERT INTO students (id_section, first_name, last_name, national_id, phone)
VALUES ($1, $2, $3, $4, $5)
RETURNING id, id_section, national_id, first_name, last_name, phone,
          active, created_at, updated_at;
```

```sql
-- list_by_section.sql                     RF-8, RF-9, RNF-3
SELECT id, id_section, national_id, first_name, last_name, phone, active,
       created_at, updated_at
FROM students
WHERE id_section = $1
  AND ($2::boolean OR active)
ORDER BY lower(last_name), lower(first_name), id;
```

```sql
-- get_by_id.sql                           RF-10, RF-11
SELECT id, id_section, national_id, first_name, last_name, phone, active,
       created_at, updated_at
FROM students
WHERE id = $1;
```

```sql
-- update.sql                              RF-12 a RF-16
UPDATE students
SET id_section = $2, first_name = $3, last_name = $4,
    national_id = $5, phone = $6
WHERE id = $1
RETURNING id, id_section, national_id, first_name, last_name, phone,
          active, created_at, updated_at;
```

```sql
-- set_active.sql                          RF-17 a RF-20
UPDATE students
SET active = $2
WHERE id = $1
  AND active IS DISTINCT FROM $2
RETURNING id, id_section, national_id, first_name, last_name, phone,
          active, created_at, updated_at;
```

Tres comentarios que no son evidentes:

- **`lower(last_name), lower(first_name)`** en el orden (RF-8): los nombres
  se guardan tal como se escriben (puede haber mayúsculas y minúsculas
  mezcladas), y el orden del padrón debe ser estable (RNF-3) e
  independiente de cómo capitalizó el instructor. El `, id` final rompe
  empates de nombre+apellido idénticos de forma determinista. El índice
  `idx_students_last_first_name` no se aprovecha (la función `lower` lo
  inhabilita); con ≤300 estudiantes no se justifica un índice funcional.
- **`$2::boolean OR active`** en `list_by_section.sql` da las dos formas del
  listado con un solo archivo (RF-8 por defecto, RF-9 con la bandera). El
  mismo patrón del filtro opcional por grado de `sections`.
- **`active IS DISTINCT FROM $2`** en `set_active.sql`: la idempotencia real
  (RF-18/RF-20), igual que en `sections` — no modifica `updated_at` ni
  escribe auditoría falsa en una llamada repetida.

### La sección: chequeo previo dentro de la misma transacción

`queries.rs` expone `ErrorEstudiante`:

```rust
pub enum ErrorEstudiante {
    EstudianteNoExiste,
    SeccionNoExiste,
    SeccionInactiva,
    Sqlx(sqlx::Error),
}
```

`crear` y `editar` hacen, **dentro de `&mut PgTransaction`** (no en el
handler), el chequeo de la sección con `SELECT active FROM sections WHERE id
= $1`: `None` → `SeccionNoExiste` (404), `Some(false)` → `SeccionInactiva`
(409). `listar_por_seccion` usa una variante solo de existencia (no de
activo): listar los estudiantes de una sección dada de baja es legítimo
(caso límite de la spec), solo se exige que exista.

Que el chequeo viva en la query (y no en el handler) es lo que hace que los
tests de integración lo ejerciten dentro de la transacción descartable,
igual que las demás invariantes. La auditoría se escribe en la misma
transacción y el `23505` de la cédula sube con `?` hasta
`From<sqlx::Error>`.

## 6. `AppError` y su ampliación

### 6.1 `NotFound` pasa a llevar mensaje

`AppError::NotFound` es hoy un variante unitaria ("Recurso no encontrado").
La spec exige mensajes que distinguen la causa: RF-3 habla de "la sección no
existe" y RF-11 de "no se encontró el estudiante". Se cambia a
`NotFound(String)`. Esto toca `sections/handler.rs` (`a_not_found` →
`NotFound("No se encontró la sección")`) y los tests unitarios de
`error.rs`, y mejora también los 404 de la Fase 2.

### 6.2 La cédula: la restricción manda (mismo criterio que `sections` §8.3)

No hay `SELECT` previo de "¿existe esta cédula?": se inserta y el `23505`
sobre `uq_students_national_id` se traduce. Se agrega el arm a
`mapear_violacion`:

| Código | Restricción | Se traduce a |
|---|---|---|
| `23505` | `uq_students_national_id` | `Conflict` — "Ya existe un estudiante con esa cédula" |

`tests/conflictos_bd.rs` gana un test con un INSERT duplicado directo que
verifica que la restricción real es la esperada. Nota: el `23503` sobre
`students_id_section_fkey` ya está mapeado en `error.rs` con el mensaje "La
sección tiene estudiantes", que corresponde al borrado de secciones (RF-23
de la 002); para el INSERT con sección inexistente no se usa, porque
`crear`/`editar` preverifican la sección y el `23503` por esa vía nunca
llega.

`mapear_violacion` pasa de `fn` privada a `pub(crate)` para poder usarla en
los tests de `conflictos_bd.rs` si hiciera falta; el arm nuevo se cubre con
un test unitario en `error.rs`, como los demás.

## 7. Endpoints y forma del JSON

| Método | Ruta | Éxito | Errores |
|---|---|---|---|
| `POST` | `/sections/{id}/students` | `201` + estudiante | `400`, `404` (sección), `409` (sección inactiva · cédula) |
| `GET` | `/sections/{id}/students?incluir_inactivas=true` | `200` + lista | `400` (parámetro), `404` (sección) |
| `GET` | `/students/{id}` | `200` + estudiante | `404` |
| `PUT` | `/students/{id}` | `200` + estudiante | `400`, `404` (estudiante · sección), `409` (cédula · sección inactiva) |
| `POST` | `/students/{id}/baja` | `200` + estudiante | `404` |
| `POST` | `/students/{id}/reactivar` | `200` + estudiante | `404` |

Forma del estudiante, en `snake_case` (constitución nº 7): `id`,
`id_section`, `national_id` (o `null`), `first_name`, `last_name`, `phone`
(o `null`), `active`, `created_at`, `updated_at`.

Dos decisiones de superficie:

- **El alta no lleva `id_section` en el cuerpo.** La ruta ya lo declara
  (`POST /sections/{id}/students`); pedirlo además en el body crea dos
  versiones de la misma verdad que pueden divergir. La edición sí lo lleva,
  porque su ruta (`PUT /students/{id}`) no lo contiene y el cambio de
  sección es parte de la edición (RF-15).
- **`incluir_inactivas` se lee como `Option<String>` y se valida a mano.**
  `Option<bool>` de `serde` rechazaría valores fuera de `true`/`false`, pero
  el rechazo de extracción de `axum` no compila contra `AppError` sin
  conversiones extra; parsear el string y responder `400` con el mensaje
  correcto es más barato y más controlado. Los demás `Query` del proyecto
  usan `String` por lo mismo.

## 8. Decisiones técnicas

### 8.1 `NotFound(String)` en vez de dos variantes "no encontrado"

Elegido: unificar la variante con mensaje. Descartado: agregar
`NotFoundSeccion` / `NotFoundEstudiante` por separado — dos variantes que
difieren solo en el string del mensaje apagan el propósito de la
especificidad tipada.

### 8.2 La sección inactiva es regla de la API, no invariante de la base

El `active` de `sections` no está protegido por una constraint que impida
insertar estudiantes en una sección inactiva: no existe, y no debe crearse
una para esto (es una regla de negocio de oferta, no una invariante de
integridad). Por eso el chequeo previo `SELECT active FROM sections ...` es
la única vía, a diferencia del duplicado de cédula donde la constraint da la
última palabra. La carrera entre el chequeo y el insert es irrelevante (un
solo usuario) y, si se da, la consecuencia es una sección inactiva con un
cupo nuevo, corregible con la reactivación.

### 8.3 Un solo struct `Estudiante` para todo

Elegido: un struct con `FromRow` + `Serialize`. Descartado: separar fila
cruda/`FilaBase` como en `sections` — ahí era obligatorio porque el enum
`shift_type` no se mapea en las macros; `students` no tiene ningún enum ni
cast, y subdividir el struct sería ceremonia sin costo que pagar.

### 8.4 Idempotencia y auditoría, mismo criterio que `sections`

`set_active` solo audita cuando el estado cambió de verdad (RF-21 no quiere
entradas falsas, y RF-18/RF-20 prometen "no modifica nada"). La distinción
entre "no existía" (404) e "idempotente" (200) usa `get_by_id` posterior,
igual que `sections::set_active`.

### 8.5 Tests HTTP con `oneshot` y limpieza de datos

Mismo criterio que la 002 §8.6: `tower::ServiceExt::oneshot` sobre el
`Router` real. Pero `students` **no expone DELETE**, así que la limpieza
pos-test de `tests/http_students.rs` (que commitea) se hace con un `DELETE`
SQL directo dentro del propio test sobre las filas creadas. No es un cambio
de comportamiento de la API: es higiene de la prueba en la base de
desarrollo, y cada test usa prefijos de UUID únicos en nombres y cédulas
para no borrar filas de otro test.

### 8.6 La regla 3 (sección inmutable con asistencia) se delega al trigger

RF-16 tiene dos partes: hoy no hay asistencia, así que la primera ("MIENTRAS
no tenga asistencia, se permite cambiar la sección") se prueba de verdad; la
segunda ("una vez que tenga asistencia, la regla 3 lo impide") la garantiza
`trg_students_section_immutable` de la init, y no se puede ejercitar hasta
la Fase 7. En la validación RF por RF, RF-16 queda marcado como "primera
cláusula probada; segunda cubierta por el trigger en la Fase 7".

## 9. Mapa RF → prueba

| RF | Qué la prueba demuestra | Tipo |
|---|---|---|
| RF-1 | El alta persiste la fila y devuelve el id | integración |
| RF-2 | Nombres vacíos o de solo espacios se rechazan | integración |
| RF-3 | Alta en sección inexistente → se informa | integración |
| RF-4 | Alta en sección inactiva → se informa | integración |
| RF-5 | Cédula duplicada se rechaza, también contra un inactivo | integración |
| RF-6 | `" Ana    María "` → `Ana María`; cédula recortada | integración |
| RF-7 | Cédula vacía/ausente → `null`; dos sin cédula conviven | integración |
| RF-8 | Listado por defecto: solo activos, orden `lower(apellido), lower(nombre)` | integración |
| RF-9 | `incluir_inactivas`: devuelve también inactivos con su estado | integración |
| RF-10 | Consulta por id devuelve sección y estado | integración |
| RF-11 | Id inexistente responde que no se encontró | integración |
| RF-12 | La edición normaliza y valida igual que el alta | integración |
| RF-13 | Editar a nombre/apellido vacíos se rechaza | integración |
| RF-14 | Editar a una cédula ajena se rechaza | integración |
| RF-15 | Mover a sección inexistente/inactiva se rechaza; a activa funciona | integración |
| RF-16 | (primer término probado; segundo, trigger en Fase 7) | integración |
| RF-17 | La baja marca inactivo y lo omite del listado por defecto | integración |
| RF-18 | Bajar dos veces: `200` y `updated_at` no cambia | integración |
| RF-19 | La reactivación deja `active = true` y vuelve al listado | integración |
| RF-20 | Reactivar dos veces: `200` sin cambios | integración |
| RF-21 | Cada operación escribe su entrada de auditoría | integración |
| RF-22 | No existe ruta ni query de borrado; los tests no la ejercitan | verificación |
| RNF-3 | El orden del listado es estable entre llamadas | integración |
| — | Códigos de estado y forma del JSON (`snake_case`, `null` en cédula/teléfono) | HTTP (`oneshot`) |
| — | Los mismos bordes contra el servidor real | `curl` en validación |

## 10. Bloqueos de entorno

Ninguno nuevo. La base ya tiene la tabla, el container Postgres corre, y las
precondiciones que documentaba el plan 002 (§10) están resueltas.

## 11. Orden de ejecución

1. `domain.rs` y sus unit tests de normalización, sin base de datos.
2. Infra de errores: `NotFound(String)` + arm de cédula en `mapear_violacion`
   + ajuste de `sections` y tests de `error.rs`.
3. `create.sql` + `ErrorEstudiante` + `verificar_seccion` + `crear` (auditoría
   "alta" en la misma tx) con tests de integración; test de cédula real en
   `conflictos_bd.rs`.
4. `get_by_id.sql` + `list_by_section.sql` + `consultar`/`listar_por_seccion`
   con tests.
5. `update.sql` + `editar` (mueve de sección, cédula ajena, auditoría) con
   tests.
6. `set_active.sql` + `baja`/`reactivar` idempotentes con tests.
7. `handler.rs` + rutas en `app.rs` + tests `oneshot` HTTP.
8. Validación RF por RF (RF-16 parcial, RF-22 por no-existencia), llamada
   real con `curl`, `Estado: implementada`, `MEMORY.md`.

## 12. Riesgos

| Riesgo | Mitigación |
|---|---|
| El campo `national_id` opcional confunde `NULL` con `""` en la edición | `preparar_entrada` normaliza `""`/`None` → `null` (RF-7); el test de "silenciar cédula" lo cubre |
| Cambiar `NotFound` a variante con mensaje rompe pruebas de la Fase 2 | Se ajustan los dos usos de `sections`, los unit tests de `error.rs` y los `oneshot` que comparen el 404 |
| Los tests HTTP dejan huérfanas en `civik_dev` (commitean) | Prefijos UUID únicos + `DELETE` SQL directo de limpieza; mismo patrón de `http_sections` |
| El orden por `lower()` ignora el índice `idx_students_last_first_name` | Aceptado: ≤300 estudiantes; un índice funcional sería optimización prematura |
| RF-16 (segundo término) queda sin prueba hasta la Fase 7 | Marcado explícito en la validación; el trigger ya existe en la init |