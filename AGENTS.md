# AGENTS.md — Civik Backend

API REST de **Civik** (instrucción premilitar): alumnos por sección, lapsos,
eventos de práctica, asistencia y notas sobre 20. Un solo desarrollador, un
solo usuario final (el instructor), sin login.

Es un proyecto personal, pero **el nivel de exigencia es el de un sistema
que va a producción a gran escala.** Eso no significa meter todo lo que
existe (Kubernetes, colas de mensajes, microservicios): significa que cada
decisión que se toma se justifica, se documenta, y se prueba — no se asume.
Donde no hace falta complejidad, no se agrega. Donde sí hace falta rigor
(la fórmula de notas, el borrado físico, la concurrencia de sincronización),
no se escatima.

**Idioma:** comentarios, nombres de dominio y mensajes de error en
**español**. Identificadores en `snake_case` (idiomático de Rust) con
palabras en español: `generar_practicas_auto`, `reprogramar_practice_day`.
Nombres de crates, imports y APIs de librerías en inglés, porque son
identificadores externos que no controlamos.

## Estado actual — leer antes de nada

Proyecto **reiniciado desde cero**. La versión anterior (Node + TypeScript +
Express, sin arquitectura hexagonal) se abandonó por decisión explícita,
antes de que hubiera código de aplicación o `db/` versionada: se detectó que
las bases no encajaban con lo que se quería construir, y se corrigió cuando
el costo de corregir era mínimo.

**Lo que se conserva del reinicio anterior**, porque no depende del
lenguaje del backend:
- El esquema de base de datos (10 reglas de dominio, defensa en
  profundidad), ya corregido: sin la tabla `notes`, sin `school_term`, sin
  las funciones de orquestación en PL/pgSQL (ver sección de base de datos).
- `dbmate` como herramienta de migraciones.
- Los tres entornos (código en GitHub, imagen en Docker, dónde corre),
  Tailscale, la filosofía de "nada se borra físicamente".

**Lo que NO existe todavía:** ningún código de Rust, ningún `Cargo.toml`,
ningún test. Este documento describe el plan acordado. No asumir que algo
existe porque está descrito acá.

## Stack

```
Rust (edición 2021, toolchain estable más reciente) + Cargo
axum              — capa HTTP
sqlx (postgres, runtime-tokio-rustls)  — sin ORM, SQL verificado en compilación
tokio             — runtime async
serde / serde_json — (de)serialización
dotenvy           — carga de .env (sucesor mantenido de la crate `dotenv`)
thiserror         — errores de dominio tipados
tracing / tracing-subscriber — logging estructurado, reemplaza console.log/error
uuid (v4, serde)  — ids de dominio
chrono (o time)   — fechas y timestamps; elegir una sola y no mezclar
rust_decimal (con la feature "rust_decimal" de sqlx) — para `numeric(4,2)`
                    de grade_overrides. NUNCA f64 para dinero ni notas: los
                    flotantes no representan 0.1 exacto, y una nota es un
                    valor donde la exactitud importa.
```

PostgreSQL 18 (`postgres:18` / `postgres:18-alpine`). `dbmate` para
migraciones, sin cambios respecto a antes.

## Configuración y conexión a la base

**Un solo archivo, sin capa de validación aparte** — mismo criterio que ya
usabas en `config/database.ts`: no hay un `env.rs` separado que valide con
un schema; la variable se lee directo donde se necesita, y si falta, falla
al arrancar con un mensaje claro. Fallar rápido en el arranque es la
política, no un descuido.

```rust
// src/config/database.rs
//
// Crea el pool de conexión. Si DATABASE_URL falta o la conexión inicial
// falla, el proceso no arranca — es preferible a fallar silenciosamente en
// la primera consulta que llegue.
//
// DATABASE_URL es también la variable que sqlx usa en tiempo de COMPILACIÓN
// para verificar cada consulta contra el esquema real (sqlx::query_file!).
// Por eso ya no hay variables sueltas (DB_HOST/DB_PORT/DB_NAME/...) como en
// la versión anterior: con una sola variable, lo que usa la app para
// conectarse en runtime y lo que usa sqlx para verificar en compile-time es
// exactamente la misma, y no pueden desincronizarse.

use sqlx::postgres::{PgPool, PgPoolOptions};
use std::env;

pub async fn create_pool() -> PgPool {
    dotenvy::dotenv().ok(); // no falla si no existe: en producción las
                            // variables ya vienen del contenedor Docker

    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| panic!("[Config] Falta la variable de entorno DATABASE_URL"));

    PgPoolOptions::new()
        .max_connections(10)
        .connect(&database_url)
        .await
        .unwrap_or_else(|error| panic!("[Database] No se pudo conectar a PostgreSQL: {error}"))
}
```

**Diferencia real con `pg` que hay que tener clara, no un simple cambio de
sintaxis:** `pg.Pool` tiene un evento `.on("error", ...)` que atrapa fallas
asíncronas del pool completo (una conexión ociosa que se cae, por ejemplo).
`sqlx::PgPool` no tiene un listener global equivalente: cada consulta
devuelve su propio `Result<_, sqlx::Error>`, y esas fallas se manejan en el
sitio donde se llama la consulta (o suben al middleware de error de `axum`).
Si hace falta reaccionar a algo a nivel de pool (por ejemplo, loguear cuando
una conexión se agota), se configura con `PgPoolOptions::after_connect` o se
observan las métricas del pool (`pool.size()`, `pool.num_idle()`), no con un
evento. No es un déficit de `sqlx`, es un modelo distinto: en Rust, los
errores son valores que se propagan explícitamente (`Result`), no eventos.

`create_pool()` se llama una vez en `main.rs` y el `PgPool` (que internamente
ya es un `Arc`, así que clonarlo es barato) se comparte a los handlers de
`axum` a través de `State`.

## Arquitectura: hexagonal proporcional

No se aplica el mismo rigor a todo el proyecto. El desacople formal (puertos
como `trait`, adaptador intercambiable) se paga donde hay negocio real que
proteger de la infraestructura; en el resto, negocio y SQL se separan por
organización de archivos, no por traits genéricos que nunca van a tener una
segunda implementación.

| Módulo | Nivel | Por qué |
|---|---|---|
| `grades` | Hexagonal completo: dominio puro (`compute_grade`) + puerto (`trait GradeRepository`) + adaptador `sqlx` | La pieza más sensible del proyecto. Debe testearse sin tocar Postgres |
| `events`, `attendance` | Hexagonal parcial: función de negocio pura separada de las queries, sin traits ni genéricos | Hay reglas reales (reprogramación, inmutabilidad), pero un solo backend, una sola base, para siempre — el trait no se va a usar nunca para cambiar de motor |
| `sections`, `students` | CRUD directo: handler → query, sin capas intermedias | No hay regla de negocio compleja que proteger |

**Decisión tomada en el reinicio:** `generar_practicas_auto` y
`reprogramar_practice_day` vivían como funciones PL/pgSQL, justificado por
atomicidad. Se mueven al dominio de Rust (`events::service`), que logra la
misma atomicidad con `sqlx::Transaction` (`BEGIN`/`COMMIT`/`ROLLBACK`). En la
base solo quedan las invariantes (los triggers de las 10 reglas): protegen
contra un bug del backend, pero ya no deciden secuencias de negocio.

## Estructura de carpetas

```
src/
├── main.rs                    entrypoint: bootstrap, crea el pool, arranca axum en 127.0.0.1
├── app.rs                     construcción del Router de axum (los tests lo importan sin bind)
├── config/
│   └── database.rs            dotenvy + PgPoolOptions (ver arriba)
├── shared/
│   ├── error.rs                enum de error transversal + impl IntoResponse
│   ├── audit.rs                helper para escribir en audit_logs dentro de una transacción
│   └── round.rs                redondeo centralizado de notas (Math.round equivalente)
└── modules/
    ├── students/               CRUD directo
    │   ├── handler.rs
    │   ├── queries.rs
    │   └── queries/*.sql        un archivo por consulta
    ├── sections/                CRUD directo (mismo patrón que students)
    ├── lapses/                  hexagonal parcial
    │   ├── handler.rs
    │   ├── domain.rs
    │   ├── service.rs
    │   ├── queries.rs
    │   └── queries/*.sql
    ├── events/                  hexagonal parcial — acá vive lo que antes
    │   │                        era generar_practicas_auto/reprogramar_practice_day
    │   ├── handler.rs
    │   ├── domain.rs            reglas puras: puede_reprogramarse(evento) -> bool, etc.
    │   ├── service.rs           orquestación con sqlx::Transaction
    │   ├── queries.rs
    │   └── queries/*.sql
    ├── attendance/               hexagonal parcial (incluye sincronización offline)
    │   └── ...                  mismo patrón que events
    ├── audit/
    │   └── ...
    └── grades/                   hexagonal completo
        ├── handler.rs
        ├── domain.rs             compute_grade — puro, sin sqlx, sin async
        ├── ports.rs              trait GradeRepository
        ├── adapter_pg.rs         implementación del trait con sqlx
        └── service.rs
tests/
└── ...                          tests de integración, patrón transacción/rollback
```

**No crear una carpeta hasta que haya código real que la llene.** No hay
`utils/` genérico: lo que de verdad se comparte entre módulos vive en
`shared/`, con un propósito acotado. No hay `controllers/` ni `services/`
centralizados: cada módulo trae los suyos si su nivel de hexagonalidad los
necesita (`students` no tiene `service.rs` porque no hay negocio que separar
del handler).

### Sobre `queries/*.sql` y por qué no un `.json` central

Las consultas **no** viven en un archivo `.json` cargado en runtime. Eso
rompería la razón principal de usar `sqlx`: la verificación en tiempo de
**compilación**. `sqlx::query_file!` (o `query_file_as!`, si mapea a un
struct) necesita ver el SQL como archivo real para poder chequearlo contra
el esquema al compilar (`cargo build` con `DATABASE_URL` disponible, o modo
offline con `cargo sqlx prepare` cacheando en `.sqlx/` para CI). Un `.sql`
por consulta, junto al módulo que la usa, conserva "SQL en archivos, sin
ORM" y además gana el chequeo que antes no existía con `pg`.

```sql
-- modules/students/queries/list_by_section.sql
SELECT id, first_name, last_name
FROM students
WHERE id_section = $1 AND active
ORDER BY last_name;
```

```rust
// modules/students/queries.rs
pub async fn list_by_section(pool: &PgPool, id_section: Uuid) -> sqlx::Result<Vec<Student>> {
    sqlx::query_file_as!(
        Student,
        "src/modules/students/queries/list_by_section.sql",
        id_section
    )
    .fetch_all(pool)
    .await
}
```

## Convenciones idiomáticas de Rust

El equivalente al "solo funciones flecha" de la versión anterior no es una
regla de sintaxis — Rust no tiene esa dicotomía. El espíritu (consistencia,
que el compilador imponga lo que antes era disciplina manual) se traduce
así:

- **`unwrap()`/`expect()` prohibidos fuera del arranque.** El único lugar
  donde fallar con un panic es aceptable es `main.rs`/`config/database.rs`,
  donde "fallar rápido con un mensaje claro" es la política ya acordada. En
  cualquier handler, service o query, un error se propaga con `?` hacia un
  tipo `Result` propio, nunca hace panic. Se activa
  `#![deny(clippy::unwrap_used, clippy::expect_used)]` a nivel de crate, con
  una excepción explícita (`#[allow(...)]`) solo en el archivo de arranque.
- **Errores de dominio como enums tipados con `thiserror`**, con un
  `#[error("...")]` en español y una variante por caso — el equivalente
  directo a "clases propias con un `code` estable". Los errores de
  infraestructura (conexión caída, timeout de `sqlx`) no se envuelven: se
  dejan subir con `?` hasta el middleware de error de `axum`, igual que
  antes no se envolvían los de infraestructura de Node.
- **Newtypes para los ids de dominio** (`StudentId(Uuid)`, `EventId(Uuid)`,
  `LapseId(Uuid)`, etc.), no `Uuid` suelto en cada firma de función. No es
  ceremonia por ceremonia: con seis tablas que usan `Uuid` como clave, pasar
  `id_event` donde se esperaba `id_student` es un error real que el
  compilador puede impedir gratis (el newtype no cuesta nada en tiempo de
  ejecución). Se aplica donde hay más de un tipo de id en juego en el mismo
  módulo (`attendance`, que combina `id_event` + `id_student`, es el caso
  más claro); en un módulo con un solo tipo de id, es opcional.
- **Iteradores antes que loops manuales con índice**, salvo que el loop
  necesite mutar estado externo de forma que el iterador lo haga menos
  legible (el generador de fechas del calendario, por ejemplo, es más claro
  como `while` que como iterador forzado).
- **`match` explícito para los enums de dominio** (`event_status`,
  `attendance_state`), no combinators anidados solo por estilo funcional: un
  `match` con pocos brazos es más legible que una cadena de `.map()` /
  `.and_then()` cuando cada rama tiene una decisión de negocio distinta.
  El estilo funcional se usa donde reduce ruido (transformar una lista),
  no como regla absoluta.
- **`tracing`, no `println!`.** Logging estructurado con campos (`tracing::error!(evento_id = %id, "no se pudo cancelar")`), no strings concatenados como en `console.error`.
- **No crear un `trait` sin una segunda implementación real o planeada.**
  Es la misma regla de "hexagonal proporcional" a nivel de código: un
  `trait GradeRepository` se justifica porque `grades` es la pieza que debe
  testearse sin Postgres; un `trait StudentRepository` para un CRUD no se
  justifica solo por "ser consistente" con `grades`.
- `rustfmt` (formato) y `clippy` (lint) como gate: cada build corre los dos,
  y un warning de clippy no ignorado se trata como error antes de mergear.
- Lo que lleva comentario es el **porqué**, no el qué — sin cambios respecto
  a la convención anterior.

## Base de datos

El esquema se conserva del reinicio, ya con las tres correcciones aplicadas
(ver `20260926162010_init.sql` corregido): sin `notes`/`note_kind`, sin
`school_term`, sin las funciones de orquestación (movidas al dominio de
Rust). Las 10 reglas de dominio permanecen como constraints/triggers: son
invariantes que deben sobrevivir a un bug del backend, no dependen del
lenguaje.

### Las 10 reglas de dominio (resumen; el detalle vive en la skill de dominio)

1. Un evento `AUTO` debe caer en el `practice_day` (día de práctica) de su
   lapso.
2. `source = 'AUTO'` implica `type_event = 'PRACTICA_REGULAR'`.
3. Un estudiante con asistencia no cambia de sección.
4. Un lapso `CLOSED` (cerrado) es terminal: no admite datos nuevos, no se
   reabre.
5. Un evento "ocurrió" si tiene al menos una asistencia, no si su fecha ya
   pasó.
6. `audit_logs` (registros de auditoría) es append-only.
7. No se registra asistencia en un evento `CANCELLED`.
8. Nada se borra físicamente (tres capas: triggers, `RESTRICT` en las FKs,
   sin `ON DELETE CASCADE`).
9. Un evento que ocurrió queda congelado en sus columnas estructurales.
10. La asistencia no se borra ni cambia de identidad; solo se corrige el
    `state` (estado).

### La fórmula de notas — la calcula el dominio de Rust, no la base

- Base **20**. `valor = 20 - Σ(pesos de los AUSENTE)`, mínimo **0**.
- Pesos: `PRACTICA_REGULAR` → `1`, `PRACTICA_EXTRA` → `0.5`,
  `PRESENTACION` → `2`. Viven en Rust, en `grades::domain`, en un único
  lugar.
- Solo `AUSENTE` penaliza.
- `N` cuenta solo eventos `SCHEDULED` con `event_date <= hoy` mientras el
  lapso no esté `CLOSED`; al cerrar, cuenta todos los `SCHEDULED`.
- Un override por (estudiante, lapso) prevalece sobre cálculo y
  elegibilidad.
- `compute_grade` es una función pura, sin `async`, sin `sqlx`: el
  repositorio (`adapter_pg.rs`) solo trae conteos agregados; la fórmula no
  sabe que Postgres existe.

## Testing: transacción + rollback, sin base de datos de test separada

No hay `civik_test`. Los tests corren contra `civik_dev`, dentro de una
transacción que nunca se confirma:

```rust
let mut tx = pool.begin().await?;

crear_estudiante(&mut tx, datos).await?;
let resultado = cancelar_evento(&mut tx, id_evento, "prueba").await;

// asserts leyendo dentro de la misma transacción

tx.rollback().await?; // la base queda exactamente como estaba
```

Sirve también para probar que las protecciones contra `DELETE` funcionan,
sin riesgo, porque el intento fallido nunca sale de la transacción
descartable. Límite del patrón: no sirve para probar concurrencia real entre
conexiones distintas — con un desarrollador y ≤300 estudiantes, ese caso no
existe en este proyecto. Si algún día hace falta, la alternativa es
`sqlx::test` (base efímera por test), no volver a una base de test separada.

## Entornos

| | Desarrollo (laptop) | Producción (Canaima) |
|---|---|---|
| Base | `civik_dev`, Postgres 18 (Docker) | `civik_dev`, Postgres 18 (Docker), otra máquina |
| Tests | Transacciones sobre `civik_dev`, con rollback | No aplica |
| API | `cargo run` | Contenedor |

Producción no expone PostgreSQL; la API escucha en `127.0.0.1` y se accede
por Tailscale. Ningún dump se copia: cada entorno arranca vacío y corre las
mismas migraciones con `dbmate`.

`civik_dev` es el nombre de la base tanto en desarrollo como en producción.
`seed.sql` conserva su guardarraíl de `current_database()` (**base de datos
actual**) y el flag `-v ALLOW_SEED=1`, porque sembrar datos falsos en
producción por error humano con `psql` directo sigue siendo posible.

## Migraciones

`dbmate` se mantiene. Un archivo por cambio, con `-- migrate:up` y
`-- migrate:down`. Nunca se edita una migración ya aplicada; se crea la
siguiente — regla absoluta desde el primer commit de `db/` en este reinicio.

## Reparto de responsabilidades

| Pregunta | Capa |
|---|---|
| ¿Cómo se ve esto en pantalla? | Frontend (PWA, otro repo) |
| ¿Qué se hace cuando pasa X? | Backend — `service.rs` (o el handler, en CRUD directo) |
| ¿Cómo se guarda o se trae de la base? | Backend — `queries.rs` |
| ¿Esto puede pasar aunque el backend tenga un bug? | Base (constraint/trigger) |

## Orden de construcción

El orden importa porque cada módulo depende de que el anterior ya funcione
— no es una lista de tareas intercambiables. Cada fase debería terminar con
algo que corre y se puede probar antes de pasar a la siguiente; no avanzar
con una fase a medias.

| Fase | Qué se construye | Por qué en este orden |
|---|---|---|
| **0. Bootstrap** | `Cargo.toml`, `config/database.rs`, `main.rs`, `app.rs`, `GET /_health`, `shared/error.rs` | Antes de escribir negocio, confirmar que compila, conecta a `civik_dev` y responde algo. Es el equivalente al `GET /_health` que ya tenías funcionando en la versión anterior |
| **1. `shared/audit.rs`** | El helper que escribe en `audit_logs` dentro de una transacción | Todo módulo con `service.rs` (lapses, events, attendance, grades) lo va a necesitar. Construirlo suelto y sin usuario real todavía sería prematuro; construirlo *después* del primer módulo que lo necesite obliga a parar a mitad de ese módulo. Se resuelve escribiéndolo junto con `shared/round.rs`, antes del primer CRUD |
| **2. `sections`** | CRUD directo completo | El módulo más simple del proyecto: sin dependencias de otra tabla, sin `service.rs`. Sirve para validar el patrón `handler.rs` → `queries.rs` → `queries/*.sql` de punta a punta con el menor riesgo posible |
| **3. `students`** | CRUD directo completo | Depende de `sections` (FK). Con el patrón ya probado en la fase 2, agrega poco riesgo nuevo — solo la regla 3 (sección inmutable con asistencia), que todavía no aplica porque no hay `attendance` |
| **4. `lapses`** | Hexagonal parcial: alta, cierre, validaciones de fechas | Sin dependencias de `events`, pero `events` depende de esto. Es también donde se confirma el patrón de `domain.rs` + `service.rs` con transacción, antes de la parte más compleja del proyecto |
| **5. `events` — CRUD manual primero** | Crear presentación/práctica extra, cancelar con motivo | Deliberadamente separado de la parte automática: valida `check_event_date_within_lapse`, la congelación por evidencia (regla 9) y el borrado bloqueado (regla 8) con la superficie más simple posible |
| **6. `events` — orquestación automática** | `generar_practicas_auto` y `reprogramar_practice_day` portadas a `events::service`, con `sqlx::Transaction` | Es la lógica más compleja del proyecto (ver skill `civik-dominio`). Se aborda al final del módulo, con el resto de `events` ya probado, no al principio |
| **7. `attendance`** | Registrar y corregir asistencia (sin sincronización todavía) | Depende de `events` y `students` ya terminados. Cubre las reglas 7 y 10 |
| **8. `grades`** | Hexagonal completo: `compute_grade`, `GradeRepository`, el endpoint por lapso y sección | Necesita datos reales de `attendance`, `events` y `lapses` para tener algo que agregar. Es la pieza más sensible: se construye con todo lo demás ya validado, no en paralelo |
| **9. Sincronización offline** | El protocolo de conflicto sobre `attendance` (idempotencia, `updated_at`, tolerancia de reloj) | Es un incremento sobre `attendance`, no un módulo nuevo. Se deja para el final porque es la parte con más casos borde de concurrencia, y conviene tener el resto del dominio estable antes de sumarle esto |
| **10. Endurecer tests** | Portar los 76 casos de `verify-constraints.sql` a tests de integración en Rust (patrón transacción/rollback), incluida la prueba de sensibilidad | Recién tiene sentido con los módulos ya escritos: antes de eso no hay código de aplicación que ejercite las invariantes, solo la base |
| **11. Deploy** | Dockerfile, `compose.yaml` | Deliberadamente al final. No documentar ni construir esto antes de tener algo real que empaquetar |

**Regla para decidir si una fase está "lista":** compila sin warnings de
`clippy`, tiene al menos los tests de integración que le corresponden
(patrón transacción/rollback), y no depende de nada que la fase siguiente
todavía no vaya a tener. Si una fase se siente forzada a adelantar trabajo
de la siguiente, es señal de que el orden de arriba necesita ajustarse, no
de que hay que apurar la fase actual.

## Frontend (otro repositorio)

Sin cambios: PWA en React/Vite, sin negocio propio. No calcula notas, no
decide si algo se puede cancelar o reprogramar. Guarda en IndexedDB,
sincroniza, y muestra la nota como **provisional** mientras el lapso está
abierto.

## Deploy (más adelante, no ahora)

Se construye en la laptop, se publica la imagen y en Canaima
(Alpine/OpenRC + Docker, Tailscale en el host) se despliega con
`docker compose pull && docker compose up -d`. No documentar más que esto
hasta que exista el primer build real.
