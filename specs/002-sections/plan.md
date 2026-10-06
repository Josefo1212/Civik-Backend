# Plan 002 — Secciones

Plan de implementación de `specs/002-sections/spec.md`. La spec manda: aquí
solo va el CÓMO, y cada decisión se justifica con la alternativa que se
descartó.

## 1. Qué se construye

El módulo `sections` completo: alta, listado (completo y por grado),
consulta por identificador, edición, baja lógica, reactivación y eliminación
física limitada a secciones vacías. Más la migración que agrega `active` y el
`CHECK` de no vacío que `sections` no tiene.

## 2. Nivel de hexagonalidad — desvío de la tabla de `AGENTS.md`

`AGENTS.md` clasifica `sections` como **CRUD directo** (*"no hay regla de
negocio compleja que proteger"*). La spec invalidó esa premisa: el grado es un
catálogo cerrado de cinco valores (RF-7), el turno tiene forma canónica
(RF-3), el grado y la subdivisión se normalizan (RF-2, RF-4), y las
combinaciones duplicadas se rechazan con un mensaje propio (RF-8, RF-17).

Por eso `sections` sube a **hexagonal parcial**: funciones puras en
`domain.rs`, sin `async` ni `sqlx`, y las queries directas en `queries.rs`.

Lo que **no** se hace, y es deliberado:

- **No hay `trait SectionRepository`.** La constitución nº 2 reserva el
  desacople formal para `grades`. Un trait acá no tendría segunda
  implementación ni en el futuro previsto, y `AGENTS.md` lo prohíbe
  explícitamente.
- **No hay `service.rs`.** No hay orquestación de varias tablas ni
  transacción de negocio que justifique una capa. `events` y `attendance` sí
  la tendrán; `sections` no.

Consecuencia: la lógica de negocio es *pura y testeable sin base de datos*,
pero las queries siguen siendo funciones planas sobre `PgPool`. Es
exactamente el punto medio que la constitución llama "hexagonal proporcional".

## 3. Archivos

### Crear

| Archivo | Responsabilidad |
|---|---|
| `src/modules/mod.rs` | Declara `pub mod sections;` |
| `src/modules/sections/mod.rs` | Módulo: `domain`, `queries`, `handler` |
| `src/modules/sections/domain.rs` | `Grado`, `Turno`, normalización y validación puras. Sin `sqlx`, sin `async` |
| `src/modules/sections/handler.rs` | Handlers HTTP: `State`, `Path`, `Json`, códigos de estado |
| `src/modules/sections/queries.rs` | Funciones de acceso a datos. Una por `.sql` |
| `src/modules/sections/queries/create.sql` | RF-1 |
| `src/modules/sections/queries/list_active.sql` | RF-9, RF-10, RF-11, RF-24 |
| `src/modules/sections/queries/get_by_id.sql` | RF-12, RF-13, y el desambiguado de RF-19/RF-21 |
| `src/modules/sections/queries/update.sql` | RF-14 a RF-17 |
| `src/modules/sections/queries/set_active.sql` | RF-18 a RF-21 |
| `src/modules/sections/queries/delete.sql` | RF-22, RF-23 |
| `tests/sections.rs` | Tests de integración, patrón transacción/rollback |
| `db/migrations/<timestamp>_sections_active_and_not_blank.sql` | RF-18, RF-20, RF-27 |

### Modificar

| Archivo | Cambio |
|---|---|
| `src/main.rs` | Agregar `mod modules;`. Añadir el `allow` de `unwrap`/`expect` (§9) |
| `src/app.rs` | Montar las rutas de `sections` sobre el `Router` |
| `src/shared/error.rs` | Agregar `AppError::Conflict` (§7) |
| `Cargo.toml` | `tower` como dev-dependency, solo para los tests (§8) |

## 4. Migración

```sql
-- migrate:up

ALTER TABLE sections
  ADD COLUMN active boolean NOT NULL DEFAULT true;

ALTER TABLE sections
  ADD CONSTRAINT sections_not_blank
  CHECK (btrim(grade_level) <> '' AND btrim(section) <> '');

-- migrate:down

ALTER TABLE sections DROP CONSTRAINT sections_not_blank;
ALTER TABLE sections DROP COLUMN active;
```

Cubre: `active` da cuerpo a RF-18, RF-20, RF-24. El `CHECK` es defensa en
profundidad para RF-5 y RF-15: si el backend tuviera un bug y guardara un
espacio, la base lo rechaza igual. Es el mismo patrón que ya usan
`students_names_not_blank` (`:72`) y `events_name_not_blank` (`:111`).

`active` **no** es un sustituto del `DELETE`: significa "sigue ofreciéndose".
Por eso el `DEFAULT true` no contradice nada y la columna no participa en la
unicidad.

**Orden obligatorio**: la migración se aplica *antes* de compilar. `sqlx`
verifica cada consulta contra el esquema real al compilar, así que
`create.sql` no compila hasta que `active` exista. Ver §10.

## 5. Funciones puras de dominio (`domain.rs`)

| Función | RF |
|---|---|
| `Grado::from_str(&str) -> Result<Grado, ErrorDominio>` | RF-7, RF-2 |
| `Grado::as_str(&self) -> &'static str` | RF-2, RF-7 |
| `Turno::from_str(&str) -> Result<Turno, ErrorDominio>` | RF-6, RF-3 |
| `Turno::as_str(&self) -> &'static str` | RF-3 |
| `normalizar_texto(&str) -> String` (recorta extremos) | RF-4 |
| `validar_no_vacio(&str, campo) -> Result<(), ErrorDominio>` | RF-5, RF-15 |
| `validar_largo(&str, max, campo) -> Result<(), ErrorDominio>` | Caso límite "subdivisión larga" |
| `preparar_entrada(grade_level, section, shift) -> Result<DatosSeccion, ErrorDominio>` | RF-2 a RF-7, RF-14 |

`preparar_entrada` es el punto único donde se decide el orden de las
validaciones: recortar → no vacío → largo → catálogo. Que sea *un* lugar es
lo que garantiza que el alta y la edición se comporten igual (RF-14 lo exige
explícitamente).

El orden importa y no es arbitrario: normalizar **antes** de validar contra
el catálogo es lo que hace que `"1RO"` sea aceptado y guardado como `1ro`
(RF-2 + RF-7 juntos), en vez de rechazado por mayúscula.

`ErrorDominio` es un enum propio con `thiserror`, en español, que el handler
traduce a `AppError`. Los errores de infraestructura (`sqlx`) no pasan por
acá: suben con `?` hasta el middleware.

## 6. Queries

Cada `.sql` se carga con `query_file_as!` (constitución nº 1). Todas devuelven
el mismo conjunto de columnas para que haya un solo tipo de fila.

```sql
-- create.sql                              RF-1
INSERT INTO sections (grade_level, section, shift)
VALUES ($1, $2, $3)
RETURNING id, grade_level, section, shift, active, created_at, updated_at;
```

```sql
-- list_active.sql                         RF-9, RF-10, RF-11, RF-24
SELECT id, grade_level, section, shift, active, created_at, updated_at
FROM sections
WHERE active
  AND ($1::text IS NULL OR grade_level = $1)
ORDER BY COALESCE(array_position($2::text[], grade_level), 99), section;
```

```sql
-- get_by_id.sql                           RF-12, RF-13
SELECT id, grade_level, section, shift, active, created_at, updated_at
FROM sections
WHERE id = $1;
```

```sql
-- update.sql                              RF-14 a RF-17
UPDATE sections
SET grade_level = $2, section = $3, shift = $4
WHERE id = $1
RETURNING id, grade_level, section, shift, active, created_at, updated_at;
```

```sql
-- set_active.sql                          RF-18 a RF-21
UPDATE sections
SET active = $2
WHERE id = $1
  AND active IS DISTINCT FROM $2
RETURNING id, grade_level, section, shift, active, created_at, updated_at;
```

```sql
-- delete.sql                              RF-22, RF-23
DELETE FROM sections
WHERE id = $1
RETURNING id, grade_level, section, shift;
```

Tres comentarios que no son evidentes:

- **`COALESCE(..., 99)`** en el `ORDER BY`. `array_position` devuelve `NULL`
  si el grado no está en el catálogo. Como Rust impide guardar un grado fuera
  del catálogo, no debería pasar; pero si pasara, `NULL` se ordenaría al
  final de forma distinta según la dirección, y RNF-3 promete orden estable.
  El `99` lo hace determinista.
- **`active IS DISTINCT FROM $2`** en `set_active.sql` hace que la operación
  sea realmente idempotente: si ya está en el estado pedido, no actualiza
  nada y no dispara `trg_sections_updated_at`, que es exactamente lo que
  RF-19 y RF-21 prometen ("no modifica nada").
- **`delete.sql` no lleva filtro por `active`.** Una sección dada de baja que
  nunca tuvo estudiantes se elimina igual (caso límite documentado). Lo que
  bloquea es exclusivamente la FK.

## 7. Endpoints y forma del JSON

| Método | Ruta | Éxito | Errores |
|---|---|---|---|
| `POST` | `/sections` | `201` + sección | `400`RF-5/6/7 |
| `GET` | `/sections?grado=<grado>` | `200` + lista | `400` si el grado no existe |
| `GET` | `/sections/{id}` | `200` + sección | `404` |
| `PUT` | `/sections/{id}` | `200` + sección | `400`, `404`, `409` |
| `POST` | `/sections/{id}/baja` | `200` + sección | `404` |
| `POST` | `/sections/{id}/reactivar` | `200` + sección | `404` |
| `DELETE` | `/sections/{id}` | `204` sin cuerpo | `404`, `409` |

Forma de la sección en JSON, en `snake_case` (constitución nº 7):
`id`, `grade_level`, `section`, `shift`, `active`, `created_at`,
`updated_at`.

Baja y reactivación van en endpoints separados y no como un campo `active`
dentro del `PUT`, por dos razones concretas: `PUT` queda siendo "cambiar los
datos de identidad" y no "cambiar cualquier cosa", lo que hace imposible
actualizar el `active` por accidente; y las dos operaciones son idempotentes
por definición (RF-19, RF-21), cosa que no se puede garantizar de un
`PATCH` genérico.

**`AppError::Conflict` es nuevo.** El enum actual solo tiene `Database`,
`NotFound` y `BadRequest`, y hay tres casos que no entran en ninguno:
combinación duplicada al alta (RF-8), combinación duplicada al editar
(RF-17), y eliminar una sección con estudiantes (RF-23). Los tres son
`409 Conflict`. El mapeo desde `sqlx` es por código de error de Postgres:

| Código | Significado | Se traduce a |
|---|---|---|
| `23505` | `unique_violation` sobre `uq_sections_grade_section_shift` | `Conflict` — "esa combinación ya está en uso" (RF-8, RF-17) |
| `23503` | `foreign_key_violation` sobre `students_id_section_fkey` | `Conflict` — "la sección tiene estudiantes" (RF-23) |

`23503` no requiere trigger propio: la constitución nº 3 y `AGENTS.md`
(coordinadas del `ON DELETE RESTRICT` en `db/20260926162010_init.sql:63`) ya
lo contemplan. Traducir el código a un mensaje en español es trabajo del
handler, no de la base.

## 8. Decisiones técnicas

### 8.1 `Grado` y `Turno` como enums de Rust, no como `String`

Elegido: `enum Grado { Primero, Segundo, Tercer[o], Cuarto, Quinto }` con
`TryFrom<&str>`, `FromStr` y `as_str`. El catálogo vive en el enum, y de ahí
se deriva tanto la validación como el array del `ORDER BY`.

Descartado: comparar `&str` con `matches!` dentro del handler. El catálogo
quedaría escrito tres veces —en la validación, en el mensaje de error de
RF-7 ("enumera los grados admitidos") y en el SQL del orden— y nada
verificaría que las tres copias coincidan.

### 8.2 El catálogo viaja al SQL como parámetro, no como literal

Elegido: `array_position($2::text[], grade_level)`, con el array construido en
Rust desde el enum. Fuente única de verdad.

Descartado: escribir `ARRAY['1ro','2do','3ro','4to','5to']` dentro de
`list_active.sql`. Es más legible, pero duplica el catálogo en el `.sql` y en
el `.rs`. El punto peligroso: `sqlx` verifica la *sintaxis* de la consulta al
compilar, no sus *valores*, así que esa duplicación no rompería la
compilación — se descubriría en producción, cuando el listado salga desordenado.

Costo aceptado: el `.sql` es menos autocontenido. Se compensa con un
comentario en el archivo que diga de dónde sale `$2`.

### 8.3 Duplicados: la restricción manda, no una consulta previa

Elegido: se intenta el `INSERT`/`UPDATE` y se traduce el `23505`.

Descartado: un `SELECT` previo para dar un mensaje más amable. Sería una
carrera entre dos peticiones concurrentes (ambas pasarían el `SELECT`,
ninguna vería la fila de la otra), y la restricción seguiría siendo la única
garantía real. Además contradice el espíritu de la constitución: el hecho se
protege con la invariante, no con una comprobación previa que puede quedar
obsoleta.

### 8.4 Idempotencia real, no idempotencia aparente

Elegido: `set_active.sql` con `active IS DISTINCT FROM`, y cuando no devuelve
filas el handler consulta `get_by_id.sql` para distinguir dos casos que la
base no separa: "no existía" (`404`) y "ya estaba así" (`200` sin cambios).

Descartado: un `UPDATE` normal y devolver `200` siempre. Sería más simple,
pero el trigger `trg_sections_updated_at` bumpearía `updated_at` en cada
llamada inútil, y RF-19 y RF-21 dicen literalmente "no modifica nada". Un
`updated_at` que se mueve solo contradice la spec.

### 8.5 Tests: `#[tokio::test]` + `rollback`, no `#[sqlx::test]`

Elegido: cada test abre su propio pool desde `DATABASE_URL`, hace
`pool.begin()`, pasa `&mut tx` al código bajo prueba, y termina en
`rollback()`.

**Conflicto encontrado.** El ejemplo "patrón base" de la skill
`testing-transaccional` muestra `#[sqlx::test] async fn(pool: PgPool)`, que
crea una base efímera por test. Eso contradice el texto de esa misma skill
("no hay `civik_test`") y la constitución nº 6 ("toda prueba corre en una
transacción de `civik_dev` que termina en `rollback`"). Gana la constitución:
`sqlx::test` queda para el único caso que el patrón no cubre, que es
concurrencia real entre conexiones, y esa skill dice que en Civik ese caso no
existe.

Se deja anotado para corregir la skill en otro momento; no se cambia aquí.

### 8.6 Tests HTTP sin abrir un puerto

Elegido: `tower::ServiceExt::oneshot` sobre el mismo `Router` que se sirve en
producción. `tower` 0.5.3 ya está en `Cargo.lock` (depende de `axum`), así que
promoverlo a dev-dependency no agrega ninguna dependencia nueva al proyecto.

Descartado: hacer bind a un puerto real dentro del test. Introduce colisiones
de puerto entre tests paralelos y hace el test lento y frágil.

Esto **no reemplaza** la verificación con `curl` que pide la skill en la fase
de validación: `oneshot` sirve para regresión automática, y la llamada real se
hace igual al cerrar la fase, para tener el registro de que el borde HTTP
funciona de verdad.

### 8.7 `deny(unwrap_used)` a nivel de crate

`AGENTS.md` lo exige y hoy no está. Se agrega
`#![deny(clippy::unwrap_used, clippy::expect_used)]` en `main.rs`, con
`#[allow(...)]` explícito en `main.rs` y `config/database.rs`, que son los
dos únicos lugares donde la constitución nº 5 permite fallar rápido.

Sin esto, el criterio "no quedan advertencias del analizador" no se puede
evaluar de verdad, porque el mismo warning aparecería en el código nuevo.

## 9. Mapa RF → prueba

| RF | Qué la prueba demuestra | Tipo |
|---|---|---|
| RF-1 | El alta devuelve identificador y la fila existe | integración |
| RF-2 | `" 1RO "` y `"A"` se guardan como `1ro` y `a` | integración |
| RF-3 | `"matutino"` se guarda como `MATUTINO` | integración |
| RF-4 | `" 1ro "` se guarda como `1ro` | integración |
| RF-5 | `""` y `"   "` se rechazan | integración |
| RF-6 | `"NOCTURNO"` se rechaza | integración |
| RF-7 | `"quinto"` se rechaza y el error enumera los cinco | integración |
| RF-8 | Alta duplicada se rechaza, y también si la original está dada de baja | integración |
| RF-9 | Un listado devuelve todas las activas en una sola llamada | integración |
| RF-10 | El filtro por grado devuelve solo ese grado | integración |
| RF-11 | El orden es `1ro`→`5to` y dentro `a`, `b`, `c` | integración |
| RF-12 | Una sección dada de baja se consulta por identificador | integración |
| RF-13 | Identificador inexistente da `404` | HTTP |
| RF-14 | La edición normaliza igual que el alta | integración |
| RF-15 | Editar a vacío se rechaza | integración |
| RF-16 | Editar a grado o turno inválidos se rechaza | integración |
| RF-17 | Editar a una combinación ajena se rechaza | integración |
| RF-18 | La baja deja `active = false` sin tocar el resto | integración |
| RF-19 | Dar de baja dos veces: `200` y `updated_at` no cambia | integración |
| RF-20 | La reactivación deja `active = true` | integración |
| RF-21 | Reactivar dos veces: `200` y `updated_at` no cambia | integración |
| RF-22 | Eliminar una sección vacía la borra de verdad | integración |
| RF-23 | Eliminar con un estudiante (activo o inactivo) se rechaza y la fila sigue | integración |
| RF-24 | El listado omite las inactivas | integración |
| RF-25 | Dar de baja una sección con estudiantes los deja en ella | integración |
| RF-26 | Editar una sección con estudiantes los deja en ella | integración |
| RF-27 | Cada operación escribe su entrada de auditoría | integración |
| RNF-1 | `INSERT` directo con `"1ro"` y `""` falla por el `CHECK` | integración |
| RNF-3 | Dos listados seguidos devuelven el mismo orden | integración |
| RNF-4 | Los mensajes de error están en español y nombran el campo | integración |
| — | `201` en alta, `400`/`404`/`409` en los errores, forma del JSON | HTTP (`oneshot`) |
| — | Los mismos bordes contra el servidor real | `curl` en validación |

Las pruebas de RF-23, RF-25 y RF-26 necesitan filas en `students`. Como el
módulo `students` es la Fase 3, esos tests insertan con SQL directo dentro de
la transacción y se descartan con el `rollback`. No se crea código de
`students` en esta fase.

## 10. Bloqueos de entorno — a y b resueltos, c pendiente

> **Estado a 2026-10-06**: los puntos (a) y (b) de esta sección quedaron
> resueltos. `dbmate` v2.36.0 instalado en `~/.local/bin`, con el `PATH`
> agregado a `~/.bashrc`; `db/20260926162010_init.sql` movida a
> `db/migrations/` sin tocar su contenido, que es la ruta que `dbmate` usa
> por defecto; y registrada en `schema_migrations` con la versión
> `20260926162010` (el regexp de `dbmate` es `^(\d+).*\.sql$`, así que la
> versión es el prefijo numérico, no el nombre del archivo). `dbmate status`
> muestra `Applied: 1, Pending: 0`.
>
> De paso: `DATABASE_URL` no decía nada de SSL, y el driver de `dbmate`
> exige `require` mientras el servidor no lo tiene. Se agregó
> `?sslmode=disable` al `.env` de desarrollo, que además hace explícito lo
> que `sqlx` ya resolvía por su cuenta.
>
> Los tres bloques que quedan son los que el plan original levantaba.

**a) `dbmate` ya no está pendiente.** Ver nota de estado arriba.

**b) La ruta de las migraciones ya no está pendiente.** Ver nota de estado
arriba.

**c) `write_audit_log` nunca se ha ejecutado.** Sigue en pie: la firma toma
`entity_id: Option<&str>` y lo bindea a una columna `uuid NOT NULL`. Dos
riesgos que la inspección no puede resolver: Postgres puede rechazar la
comparación `text` contra `uuid` sin un cast explícito, y `Option` permite
`NULL`, que la columna no admite. RF-27 depende de que esto funcione, y nada
en el código actual lo ejercita, porque la Fase 1 construyó el helper y no
lo usó.

Esto no es un problema del plan de `sections`, pero es un bloqueo para
RF-27. Lo más temprano que se puede hacer es un test que llame al helper y
descarte la transacción: si falla, se corrige la firma a `Uuid`
**no-opcional** en `shared/audit.rs`. No `Option<Uuid>`, que es lo que
decía este plan en su primera versión: la columna es `uuid NOT NULL`, y
`Option` seguiría admitiendo un NULL que la base rechaza en runtime, que es
el mismo bug movido de lugar. Es la tarea T3.

De paso, `data: Option<&Value>` bindea contra `jsonb NOT NULL`: con `None`
produce un NULL y revienta igual. Se arregla bindeando `{}` por defecto.
Ojo: no sirve `unwrap_or_default()`, porque `Value::default()` de serde_json
es `Null`, no `{}` — se construye `Value::Object(Map::new())`, que es además
lo que la columna ya trae como `DEFAULT`.

**d) `dbmate` va a generar `db/schema.sql`.** Después del próximo
`dbmate up`, aparece un volcado del esquema en la raíz de `db/`. `AGENTS.md`
no lo menciona y `.gitignore` no lo cubre. Hay que decidir si se versiona
—si sí, un entorno nuevo se reconstruye con `dbmate load` sin tener que
correr la init— o si se agrega a `.gitignore`. Decisión pendiente, no
bloqueante.

## 11. Orden de ejecución

1. Resolver el bloqueo de entorno (a y b de §10) — de esto depende que la
   migración se pueda aplicar y verificar.
2. Migración, aplicada y verificada.
3. `shared/error.rs`: `Conflict` y el mapeo de `23505`/`23503`.
4. `domain.rs` y sus tests unitarios de normalización, sin base de datos.
5. Probar `write_audit_log` (§10c) antes de construir RF-27 sobre él.
6. `queries.rs` + los seis `.sql`, con tests de integración.
7. `handler.rs` + rutas en `app.rs`, con tests `oneshot`.
8. `deny(unwrap_used)` y `cargo clippy` limpio.
9. Validación RF por RF, incluida la llamada real con `curl`.
10. `MEMORY.md` al día y `Estado: implementada` en la spec.

## 12. Riesgos

| Riesgo | Mitigación |
|---|---|
| El orden de `list_active.sql` depende de un array que viene de Rust, y una futura modificación del enum rompa el `.sql` en silencio | La prueba de RF-11 falla de inmediato si el orden se rompe; no hace falta un test extra |
| Los tests de RF-23/25/26 insertan en `students` con SQL directo, y `students` cambia en la Fase 3 | Esos tests leen y escriben solo columnas que la Fase 3 no va a cambiar (`id_section`, `active`); si cambian, se ajustan en esa fase |
| `write_audit_log` con `Option<&str>` contra `uuid NOT NULL` falla | Se prueba antes de construir RF-27 (§10c) |
| RF-27 depende de que `audit_logs.entity_id` admita el id de una sección ya borrada | No hay FK de `audit_logs` a `sections`, así que sobrevive; el test de RF-22 y RF-27 juntos lo comprueban |
| El `CHECK` de no vacío rechaza datos que el backend sí aceptaría, si las reglas de normalización divergen | Ambos aplican la misma regla de recorte; el test de RNF-1 verifica que la base acepta lo que el backend produce |