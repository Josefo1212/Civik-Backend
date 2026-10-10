# Plan 008 — Notas (cálculo por sección)

## Nivel de hexagonalidad

Hexagonal **completo** (el único del proyecto, AGENTS.md): `domain.rs` puro (sin
`async`, sin `sqlx`), `ports.rs` (`trait GradeRepository`), `adapter_pg.rs`
(implementación `sqlx`), `service.rs` (orquesta), `handler.rs` (HTTP). El trait
existe porque es la pieza que debe poder probarse **sin Postgres**.

## Archivos

| Archivo | Responsabilidad |
|---|---|
| `src/modules/grades/domain.rs` | `Conteos`, `Nota`, `compute_grade` (puro), `resolver_nota` (override prevalece). Pesos en un único lugar. |
| `src/modules/grades/ports.rs` | `trait GradeRepository` con las consultas que el dominio necesita. |
| `src/modules/grades/adapter_pg.rs` | Implementación del trait con `sqlx` sobre `queries/*.sql`. |
| `src/modules/grades/queries/existe_lapso.sql` | `SELECT EXISTS(...)`. |
| `src/modules/grades/queries/existe_seccion.sql` | `SELECT EXISTS(...)`. |
| `src/modules/grades/queries/n_eventos_elegibles.sql` | Cuántos eventos elegibles tiene el lapso (N, a nivel de lapso). |
| `src/modules/grades/queries/ausentes_por_estudiante.sql` | Ausentes por tipo y por estudiante, solo de eventos elegibles y solo `AUSENTE`. |
| `src/modules/grades/queries/roster.sql` | Estudiantes activos de la sección ∪ con asistencia en el lapso, con su override, orden canónico. |
| `src/modules/grades/service.rs` | Une roster + conteos + N + overrides y resuelve la nota por estudiante. |
| `src/modules/grades/handler.rs` | `GET /lapses/{id_lapse}/sections/{id_section}/grades`. |
| `src/modules/grades/mod.rs`, `src/app.rs` | Registro y montaje. |
| `tests/grades.rs` | Dominio puro (sin base) y service con un `GradeRepository` falso (RNF-6). |
| `tests/http_grades.rs` | Bordes HTTP con `oneshot` (200/404). |

## Modelo de dominio (puro)

```rust
struct Conteos {
    ausentes_regular: i64,       // AUSENTE en eventos elegibles PRACTICA_REGULAR
    ausentes_extra: i64,
    ausentes_presentacion: i64,
    n_eventos_elegibles: i64,    // N del lapso (a nivel de lapso, no por estudiante)
}

enum Nota {
    SinNota,          // N = 0 y sin override
    Valor(Decimal),
}

fn compute_grade(&Conteos) -> Nota;                    // RF-1, RF-2, RF-6, RF-7, RF-8
fn resolver_nota(conteos: &Conteos, override_: Option<Decimal>) -> Nota;  // RF-5
```

- Pesos en un único lugar: `PRACTICA_REGULAR = 1`, `PRACTICA_EXTRA = 0.5`,
  `PRESENTACION = 2` (Decimal, nunca `f64`, RNF-3).
- `compute_grade`: si `n_eventos_elegibles == 0` → `SinNota`; si no,
  `Valor(round_grade(max(0, 20 − Σ)))` con `shared/round.rs` (un decimal).
- `resolver_nota`: si hay override → `Valor(override)` **sin redondear** (se
  devuelve el valor almacenado); si no, `compute_grade`.
- N es del lapso: "no hay nada que calificar" no depende del estudiante.

## Puerto (`trait GradeRepository`)

```rust
#[async_trait]
pub trait GradeRepository: Send + Sync {
    async fn existe_lapso(&self, id_lapse: Uuid) -> Result<bool, sqlx::Error>;
    async fn existe_seccion(&self, id_section: Uuid) -> Result<bool, sqlx::Error>;
    async fn n_eventos_elegibles(&self, id_lapse: Uuid) -> Result<i64, sqlx::Error>;
    async fn ausentes_por_estudiante(&self, id_lapse: Uuid)
        -> Result<Vec<AusentesEstudiante>, sqlx::Error>;
    async fn roster(&self, id_lapse: Uuid, id_section: Uuid)
        -> Result<Vec<AlumnoSeccion>, sqlx::Error>;
}
```

El repositorio trae **conteos agregados** (RNF-4), no filas de asistencia; el
dominio nunca ve un `Event` ni un `Attendance`.

## Elegibilidad (RF-3, RF-4) — dónde vive

La regla "eventos `SCHEDULED` con `event_date <= hoy`, o todos si el lapso está
`CLOSED`" es una condición de la consulta (el repositorio la aplica al contar N y
al contar ausentes), porque depende de la fecha y del estado del lapso en la
base. El **efecto** (N=0 → sin nota; los `AUSENTE` futuros no descuentan) se
verifica en integración creando eventos pasados/futuros y un lapso cerrado.

## Endpoint

`GET /lapses/{id_lapse}/sections/{id_section}/grades` →
`200` con un arreglo, en orden canónico:

```json
[
  { "id_student": "...", "first_name": "...", "last_name": "...", "nota": "19.5" },
  { "id_student": "...", "first_name": "...", "last_name": "...", "nota": null }
]
```

- `nota` es `null` cuando `SinNota` (RF-6), string con el decimal cuando hay
  (override con 2 decimales, cálculo con 1).
- Lapso inexistente → `404` (RF-10); sección inexistente → `404` (RF-11).

## Decisiones técnicas (con alternativa descartada)

- **El repositorio aplica la elegibilidad por fecha/lapso** en vez de traer
  todas las filas y filtrar en Rust: la fecha y el estado del lapso son datos de
  la base, y el conteo agregado es la consulta natural (RNF-4). Descartado:
  traer eventos y asistencia crudos, que rompería "la fórmula no sabe que
  Postgres existe" con datos de infraestructura en el dominio.
- **N a nivel de lapso**, no por estudiante: "no hay eventos elegibles" es una
  propiedad del lapso. Descartado: derivar N de las filas de asistencia de cada
  estudiante, que confundiría "sin asistencia" con "sin eventos".
- **Override resuelto en el dominio** (`resolver_nota`), no en el SQL: la regla
  "el override prevalece sobre cálculo y elegibilidad" es de negocio y se prueba
  pura. Descartado: un `COALESCE(override, cálculo)` en SQL, que esconde la
  regla en la query.
- **Un `trait`, no funciones sueltas**: es lo que permite el test con repositorio
  falso (RNF-6) sin renunciar a `sqlx` en producción.
- **Dos decimales del override intactos**: `numeric(4,2)` es el valor que fijó el
  docente; redondearlo sería alterar el dato.

## Estrategia de tests

- **Dominio puro** (`tests/grades.rs`, sin base): `compute_grade` con cada peso,
  `JUSTIFICADO`/`PRESENTE` no descuentan (esto se modela como `AUSENTE`=0), tope
  en 0, N=0 → `SinNota`, `resolver_nota` con override que prevalece, redondeo a
  un decimal.
- **Service con `GradeRepository` falso** (RNF-6): un doble en memoria que
  devuelve conteos, N y overrides fijos; verifica que el service una todo y
  respete `SinNota`/`Valor` y el `null` del override ausente.
- **Integración `tx`/`rollback`**: crear sección, estudiantes, lapso, eventos
  pasados/futuros y asistencia; verificar que un `AUSENTE` futuro no descuenta
  con el lapso abierto y sí con el lapso `CLOSED`; override que gana; orden
  canónico y `null`.
- **HTTP `oneshot`** (`tests/http_grades.rs`): `200` con la forma del JSON, `404`
  de lapso y de sección. Purga su residuo al arrancar (año y sección exclusivos).
- **Años exclusivos**: cada test con su año escolar.

## RF → dónde se prueba

- RF-1, RF-2, RF-7: dominio puro.
- RF-3, RF-4: integración (eventos pasados/futuros, lapso cerrado).
- RF-5: dominio puro (`resolver_nota`) + integración (override gana).
- RF-6: dominio puro (N=0) + integración (nota `null`).
- RF-8: dominio puro (redondeo) + integración (override con 2 decimales).
- RF-9: integración (roster, orden, inactivo con asistencia).
- RF-10, RF-11: HTTP (404).
- RF-12: integración (notas independientes).
- RF-13: integración (no persiste; recalcula tras corregir asistencia).
