# Plan 004 — Lapsos

## 1. Qué se construye

El módulo `lapses` completo, hexagonal parcial (ver §2), con siete endpoints:

| HTTP | Ruta | Acción | RF |
|---|---|---|---|
| POST | `/lapses` | `crear` | RF-1, RF-2, RF-3, RF-4 |
| GET | `/lapses` | `listar` | RF-5 |
| GET | `/lapses/{id}` | `consultar` | RF-6, RF-7 |
| PATCH | `/lapses/{id}` | `editar` | RF-8, RF-9, RF-10, RF-11 |
| POST | `/lapses/{id}/activar` | `activar` | RF-12, RF-13, RF-14 |
| POST | `/lapses/{id}/cerrar` | `cerrar` | RF-15, RF-16 |
| DELETE | `/lapses/{id}` | `eliminar` | RF-17, RF-18 |

Todas las operaciones de escritura corren en `service.rs` con transacción y
auditoría atómica; la lectura directa de `consultar`/`listar` no necesita
transacción (verbos sin efecto, mismas queries `list.sql`/`get_by_id.sql`).

## 2. Nivel de hexagonalidad

`lapses` es **hexagonal parcial** (tabla de AGENTS.md): negocio puro separado
de las queries, sin `trait` ni genérico. Se diferencia de `sections`/`students`
(CRUD directo) porque hay **reglas de dominio reales**: transiciones de estado
(`PLANNED → ACTIVE → CLOSED`, ninguna retroceso), validaciones de fechas y la
excepción de borrado físico del lapso vacío. La transacción ya no vive en el
handler como en las fases 2/3, sino en `service.rs`: es la primera fase donde
una operación de escritura necesita leer el estado **antes** de escribir
(validar la transición) y auditar **en el mismo** `BEGIN/COMMIT`, y así lo
deja la arquitectura acordada ("domain.rs + service.rs con transacción").

## 3. Archivos

### Crear

```
src/modules/lapses/
├── mod.rs                 (declaración del módulo; `mod.rs` ya existe y suma `pub mod lapses`)
├── handler.rs             capa fina HTTP: parsea, llama a service, decide código de estado
├── domain.rs              DatosLapso, DiaSemana, EstadoLapso, preparar_entrada,
│                          validar_transicion + unit tests (sin base de datos)
├── service.rs             dueño de la transacción: read → validación de dominio → queries → audit
├── queries.rs             SQL con las queries y la escritura de auditoría
└── queries/
    ├── create.sql         INSERT + RETURNING (RF-1)
    ├── get_by_id.sql      SELECT por id (RF-6/7)
    ├── list.sql           SELECT ordenado school_year DESC, lapse_number ASC (RF-5)
    ├── update.sql         UPDATE estructural (RF-8, RF-9, RF-10, RF-11)
    ├── set_status.sql     UPDATE status con assert del estado previo (RF-12…RF-16)
    ├── events_outside.sql SELECT count de eventos fuera de un rango de fechas (RF-11 pre-chequeo)
    └── delete.sql         DELETE + RETURNING (RF-17, RF-18)
```

### Modificar

- `src/app.rs`: rutas nuevas (patrón `/lapses/{id}` de axum 0.8, igual que `sections`/`students`).
- `src/shared/error.rs`: cuatro variantes nuevas en `mapear_violacion` + docstring, y sus unit tests.
- `tests/lapses.rs` (nuevo): integración con patrón transacción/rollback (RF 1…19).
- `tests/http_lapses.rs` (nuevo): bordes HTTP con `oneshot`, datos únicos y limpieza.
- `tests/conflictos_bd.rs`: casos de defensa en profundidad que la API no puede alcanzar
  (trigger de `CLOSED`, solape directo, segundo `ACTIVE` directo, borrado de lapso cerrado/terminado,
  fake `events` contra el borrado y contra el movimiento de fechas).

## 4. Funciones puras de dominio (`domain.rs`)

- `DiaSemana`: enum sobre los 7 valores de `week_day` (`MONDAY`…`SUNDAY`), con
  `TryFrom<&str>` case-insensitive y recortado (mismo espíritu que `Grado` de
  la 002): `" thursday "` → `THURSDAY`. `TODOS` como única fuente de verdad.
- `EstadoLapso`: enum `Planeado`/`Activo`/`Cerrado` sobre `lapse_status`;
  `parse(&str)` para leer `status::text` de la base.
- `DatosLapso { school_year: i32, lapse_number: i16, start_date: NaiveDate,
  end_date: NaiveDate, practice_day: DiaSemana }`.
- `ErrorDominio` (thiserror, mensajes en español): año fuera de 2000–2100,
  número fuera de 1–3, fecha inválida, fechas invertidas, día inválido,
  estado incorrecto para la operación.
- `preparar_entrada(&str, &str, &str, &str, &str) -> Result<DatosLapso, ErrorDominio>`:
  todas las entradas llegan como `String` para que **el único punto de
  validación** (mismo criterio que `preparar_entrada` de la 002) produzca los
  mensajes en español — `school_year` y `lapse_number` entran como texto y se
  parsean acá, igual que `start_date`/`end_date` (`NaiveDate`) y `practice_day`.
- `validar_transicion(estado: &EstadoLapso, operacion: Operacion) -> Result<(), ErrorDominio>`
  con `Operacion { Activar, Cerrar, Editar, Eliminar }`. Reglas:
  - `Activar`: solo `PLANNED` — un `ACTIVE` ya está activo y un `CLOSED` es terminal → 409.
  - `Cerrar`: solo `ACTIVE` → 409.
  - `Editar`: nunca `CLOSED` → 409.
  - `Eliminar`: nunca `CLOSED` y `end_date >= hoy` → 409.

La decisión de la clarificación (editable mientras no `CLOSED`, cierre solo
desde `ACTIVE`, borrado físico solo del lapso vacío/no cerrado/no terminado)
vive **acá**, no en la base: la base solo garantiza las invariantes duras
(filas que sobreviven a un bug del backend), y este dominio produce el mensaje
entendible para el caso que la base rechaza con `23514` de un trigger.

## 5. Queries y su estructura

Patrón de enums vía `::text`, igual que `sections`: en `INSERT`/`UPDATE` el
parámetro viaja `text` y se castea con `$n::text::week_day` / `$n::text::lapse_status`;
en `SELECT`/`RETURNING` se lee `practice_day::text AS practice_day` y
`status::text AS status`, que sqlx infiere como `Option<String>` y
`en_lapso()` normaliza a `String` (idéntico a `FilaConShift` de la 002).
`start_date`/`end_date` son `date` → `NaiveDate`; los timestamps → `DateTime<Utc>`
(sqlx con feature `chrono`, ya en `Cargo.toml`).

### Mapeos nuevos en `mapear_violacion`

| (código, restricción) | Mensaje | Caso |
|---|---|---|
| (23505, `uq_lapses_year_number`) | "ya existe un lapso para ese año escolar y número" | alta/edición duplicada |
| (23P01, `lapses_no_overlap`) | "las fechas se solapan con las de otro lapso" | `EXCLUDE` con rango `[]` (ojo: **23P01**, no 23505) |
| (23505, `uq_lapses_single_active`) | "ya hay otro lapso activo" | segunda activación (índice parcial) |
| (23001\|23503, `events_id_lapse_fkey`) | "el lapso tiene eventos asociados" | borrado |
| (23001\|23503, `grade_overrides_id_lapse_fkey`) | "el lapso tiene overrides de nota" | borrado |

`uq_lapses_single_active` y `lapses_no_overlap` son índices, no constraints
UNIQUE con nombre de tabla: el arm existe y `AppError::from` los traduce a
`Conflict` (409). Los triggers (23514) **no** se mapean: son la defensa final y
se prueban con SQL directo en `conflictos_bd.rs`.

### Pre-chequeos dentro de la transacción

- `service::editar`: antes del `UPDATE` de fechas, `events_outside.sql`
  cuenta los eventos que quedarían fuera; si hay alguno → `409` con mensaje
  claro (RF-11). El trigger `trg_lapses_dates_against_events` queda como
  defensa. Este pre-chequeo lo hace posible el `service.rs`: en `sections` el
  handler no leía antes de escribir porque no había reglas que consultar.
- `service::eliminar`: `get_by_id` → `validar_transicion(Eliminar)` → auditoría
  → `DELETE`. El trigger `lapses_delete_guard` queda como defensa (confirma
  el mismo criterio de la 002 con `students`).
- `service::activar`/`cerrar`: la query lleva `status = $esperado` en el
  `WHERE` como assert; si `set_status.sql` no devuelve filas, `get_by_id`
  distingue 404 (no existe) de estado inválido.

### Auditoría

`entity = "lapses"`, acciones `alta`/`editar`/`activar`/`cerrar`/`eliminar`,
payload con claves en español como la 002 (`anio_escolar`, `numero_lapso`,
`inicio`, `fin`, `dia_practica`; para `eliminar` además `estado`). La de
`eliminar` se escribe **antes** del `DELETE`, para conservar la fila que
desaparece (mismo criterio que `students::eliminar`, RF-19).

## 6. `AppError` y su ampliación

No hay variantes nuevas: se usan `BadRequest` (400) para entrada inválida,
`Conflict` (409) para colisiones y transiciones de estado, `NotFound` (404) e
`IntoResponse` ya existentes. Sí se agregan los arms de `mapear_violacion` de §5
y sus unit tests (código sin restricción ≠ mensaje cierto, como documenta el
docstring actual).

Distinción de códigos: **400** si el dato de entrada falla (el cuerpo que envió
el cliente), **409** si el recurso está en un estado que impide la operación
(colisión de unicidad/solape, lapso ya activo/cerrado). A diferencia de la baja
idempotente de `sections`, **activar algo ya activo es 409 y no 200**: un lapso
`ACTIVE` no se puede "volver a activar" porque el sistema solo admite uno, y
repetir `cerrar` sobre un `CLOSED` intentaría mutar un registro terminal.

## 7. Endpoints y forma del JSON

Cuerpo de alta/edición (todo como texto; el dominio lo valida):
```json
{ "school_year": "2026", "lapse_number": "1",
  "start_date": "2026-09-14", "end_date": "2026-12-18",
  "practice_day": "THURSDAY" }
```
Respuesta (siempre `snake_case` y con su estado):
```json
{ "id": "...", "school_year": 2026, "lapse_number": 1,
  "start_date": "2026-09-14", "end_date": "2026-12-18",
  "practice_day": "THURSDAY", "status": "PLANNED",
  "created_at": "...", "updated_at": "..." }
```
Códigos: alta 201; listado/consulta/edición/activación/cierre 200; borrado
204; 400 (entrada), 404 (no existe), 409 (colisión o transición ilegal).

## 8. Decisiones técnicas

### 8.1 `service.rs` dueño de la transacción (primera vez en el proyecto)

En 002/003 el handler abría la transacción. Acá el `service` la abre porque
escribir ya no es "crear y auditar": `editar`/`activar`/`cerrar`/`eliminar`
deben **leer el estado, validarlo y recién escribir** en el mismo `BEGIN`,
y esa secuencia es negocio, no transporte. El handler se queda como capa fina
tal cual lo declara AGENTS.md para lapses. Alternativa descartada: validar el
estado en el handler leyendo el lapso — duplicaría la lectura y convertiría el
handler en una segunda capa de negocio.

### 8.2 Sin comparaciones contra "hoy" en alta/edición/activación

La spec lo fija (decisión de la clarificación) y aquí se ejecuta: `preparar_entrada`
no toca `Utc::now()`; un lapso puede registrarse y activarse con fechas pasadas
(el primer término con la primera clase del 8/10 ya ocurrida). La **única**
comparación con el presente del módulo es la defensa del borrado (no eliminar
un lapso terminado), y replica exactamente `lapses_delete_guard` para dar el
mensaje en 409 en vez de dejar pasar el `23514`.

### 8.3 El solape se detecta como `23P01`, no `23505`

Un índice `EXCLUDE` violado emite `exclusion_violation` (23P01), no
`unique_violation`. Si se mapeara solo `23505`, un solape se convertiría en
500 con mensaje de infraestructura. El arm `(23P01, lapses_no_overlap)` es el
que vuelve al instructor un mensaje en español (RNF-4).

### 8.4 Los tests HTTP no prueban el cierre feliz (residuo indeletable)

Los tests HTTP commitean (el handler commitea) y un lapso `CLOSED` es
**imposible de borrar** (el guardarraíl de la base lo bloquea). Por eso el
camino feliz de `cerrar` se prueba solo en integración (transacción+rollback),
y en HTTP se ejercita el borde `cerrar` un lapso `PLANNED` → 409 (sin residuo).
Lo mismo aplica a los 409 de `editar`/`eliminar` sobre `CLOSED`: integración.
Es una desviación deliberada del "todo se limpia con SQL directo" de la 003,
justificada por la inmutabilidad del `CLOSED`; se documenta en tasks.md.

### 8.5 Identidad única en HTTP: `school_year` derivado del UUID

`uq_lapses_year_number` es la única llave "natural". Los tests HTTP usan un
`school_year` dentro de 2000–2100 derivado de un UUID (2000 + fragmento) y
fechas futuras (ej. fin en 2045) para que `end_date >= hoy` siempre se cumpla
y el `DELETE` de limpieza nunca caiga en el guardarraíl del borrado. Riesgo
asumido: si una corrida falla antes de limpiar y deja un lapso, el próximo run
puede toparse con ese año; el 409 resultante lo delata con su mensaje.

### 8.6 Listado simple, sin filtros

`GET /lapses` devuelve todo ordenado (`school_year` DESC, `lapse_number` ASC).
Son ≤9 filas; el frontend filtra. Si algún día se pide `?school_year=` se
agrega con un `$1::int IS NULL OR` (patrón `list_active.sql`). Se evita el
sesgo de construir filtros sin usuario real que los exija.

## 9. Mapa RF → prueba

| RF | Prueba (integración «lapses.rs», HTTP «http_lapses.rs», directa «conflictos_bd.rs») |
|---|---|
| RF-1 | `el_alta_persiste_la_fila_en_estado_planeado` + `alta_201_devuelve_el_lapso_normalizado` |
| RF-2 | unit de `domain.rs` (año/número/fechas/día) + `alta_400_...` en HTTP |
| RF-3 | `el_alta_duplicada_en_mismo_anio_y_numero_falla` + `alta_duplicada_409` |
| RF-4 | `el_alta_solapada_falla` + `alta_solapada_409` (los dos: 23P01) |
| RF-5 | `listar_ordena_por_anio_descendente_y_numero_ascendente` + vacío `[]` + `listado_200` |
| RF-6 | `consultar_devuelve_el_lapso_con_su_estado` + `consulta_200` |
| RF-7 | `consultar_inexistente_no_devuelve_fila` + `consulta_404` |
| RF-8 | `la_edicion_valida_igual_que_el_alta_y_no_toca_el_estado` + `editar_200` |
| RF-9 | `editar_un_lapso_cerrado_se_rechaza` + `el_trigger_no_deja_editar_un_closed` (directa) |
| RF-10 | `editar_hacia_un_anio_y_numero_ajenos_falla` + `editar_hacia_solape_falla` + `editar_duplicado_409` |
| RF-11 | `mover_fechas_dejando_un_evento_fuera_se_rechaza` (pre-chequeo del service, con fila de `events` insertada por SQL) + `el_trigger_no_mueve_fechas_con_eventos_fuera` (directa) |
| RF-12 | `la_activacion_pasa_un_lapso_planeado_a_activo` + `activar_200` |
| RF-13 | `la_segunda_activacion_con_otro_lapso_activo_falla` + `activar_segundo_activo_409` + `el_indice_no_permite_dos_activos_directo` (directa) |
| RF-14 | `activar_un_lapso_no_planeado_falla` + `activar_un_ya_activo_409` |
| RF-15 | `el_cierre_pasa_un_lapso_activo_a_cerrado_e_inmutable` (integración; ver §8.4) |
| RF-16 | `cerrar_un_lapso_no_activo_falla` + `cerrar_no_activo_409` |
| RF-17 | `eliminar_un_lapso_vacio_lo_borra_y_deja_auditoria` + `eliminar_204` |
| RF-18 | `eliminar_un_lapso_cerrado_se_rechaza`, `eliminar_un_lapso_terminado_se_rechaza`, `eliminar_un_lapso_con_evento_se_rechaza` + `el_guardarrail_no_borra_cerrados_ni_terminados` (directa) |
| RF-19 | `el_alta_deja_su_entrada_en_el_historial`, `la_activacion_y_el_cierre_dejan_su_auditoria`, `el_borrado_deja_su_auditoria_antes_de_desaparecer` |

## 10. Orden de ejecución

Ver `tasks.md`. Orden: 1) domain puro (+unit) → 2) `mapear_violacion` (+unit) →
3) queries + SQL → 4) service → 5) handler + rutas → 6) integración →
7) HTTP → 8) defensa directa en conflictos_bd → 9) validación RF por RF y cierre.

## 11. Riesgos

- **Enums y sqlx**: `practice_day`/`status` como `Option<String>` en las filas
  (casts infieren nulable); `en_lapso()` normaliza. Es el mismo patrón probado
  de la 002, sin riesgo nuevo.
- **23P01**: fácil de mapear solo 23505 y dejar el solape como 500; mitigado en §8.3.
- **Residuo de HTTP**: los lapsos cerrados/terminados no se pueden limpiar;
  mitigado en §8.4 (el feliz de cerrar solo se prueba en integración).
- **Colisión de años en HTTP**: baja, delatada por el mensaje de 409; §8.5.
- **La regla 11 (fechas vs eventos)**: el pre-chequeo del service duplica la
  lógica del trigger; es intencional (mensaje 409) y el trigger queda como
  defensa, no como segunda fuente de verdad.