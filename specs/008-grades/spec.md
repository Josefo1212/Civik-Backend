# Spec 008 — Notas (cálculo por lapso)

Estado: implementada

> Reconciliación cerrada (fase C, 2026-10-10): la primera implementación
> calculaba la nota sin elegibilidad por fecha, sin respetar el override y con
> un endpoint por estudiante (no por sección). Se reescribió esta spec y el
> módulo se completó en hexagonal completo (dominio puro, `GradeRepository`,
> adaptador, service y endpoint por lapso y sección).

## Contexto y objetivo

La nota de un lapso es el resultado que el instructor entrega al final del
período. Se deriva de la asistencia a los eventos del lapso: cada `AUSENTE`
descuenta un peso según el tipo de evento, con base 20. Esta fase construye el
cálculo y su consulta, apoyada en `attendance`, `events` y `lapses` ya
terminados (fases 5, 6 y 7).

La pieza central es una función **pura** (`compute_grade`) que no sabe que
Postgres existe: el repositorio le acerca conteos agregados y ella aplica la
fórmula. Alrededor, el módulo se organiza en hexagonal **completo**
(`trait GradeRepository` + adaptador `sqlx`) porque es la parte del sistema que
debe poder probarse sin base de datos.

La nota es **provisional mientras el lapso está abierto**: el docente aún puede
corregir asistencia, así que el resultado puede cambiar. Al cerrar el lapso, el
cálculo se congela (todos los eventos `SCHEDULED` cuentan, ya no se filtra por
fecha).

## Usuarios

- Instructor (único usuario del sistema).

## Historias de usuario

- HU-1. Como instructor, quiero ver la nota de cada estudiante de una sección en
  un lapso, para entregar los resultados del período.
- HU-2. Como instructor, quiero que la nota refleje solo la asistencia
  registrada hasta hoy mientras el lapso está abierto, para que un evento
  futuro todavía no afecte el resultado.
- HU-3. Como instructor, quiero que al cerrar el lapso cuenten todos los
  eventos programados, para que un evento posterior a hoy en un lapso ya
  cerrado no quede fuera del cálculo.
- HU-4. Como instructor, quiero poder fijar una nota manual que reemplace la
  calculada cuando lo amerite, para casos que la fórmula no cubre.

## Definiciones

- **Nota**: valor en base 20, con un decimal, resultado del cálculo (o de un
  override). Nunca puede ser negativo (mínimo 0).
- **Evento elegible**: un evento `SCHEDULED` del lapso. Si el lapso **no** está
  `CLOSED`, además debe tener `event_date <= hoy`; si está `CLOSED`, todo evento
  `SCHEDULED` es elegible sin importar la fecha.
- **N (total de eventos elegibles)**: cuántos eventos elegibles hay en el lapso
  para el estudiante. Es la señal de si "hay algo que calificar".
- **Override**: una nota fijada a mano para un par (estudiante, lapso); vive en
  `grade_overrides` (valor `numeric(4,2)`, entre 0 y 20). Si existe, **prevalece
  sobre el cálculo y sobre la elegibilidad**: no importa N ni los conteos.
- **AUSENTE penaliza; JUSTIFICADO y PRESENTE no.** Un estudiante "sin marcar"
  (sin fila en `attendance`) no es `AUSENTE`: no penaliza.
- **Sin nota**: no hay resultado que mostrar (N=0 y sin override). No es 0: es
  ausencia de dato.

## Requisitos funcionales

- RF-1. EL SISTEMA calcula la nota de un lapso como
  `max(0, 20 − Σ(peso(tipo) por cada AUSENTE elegible))`, con los pesos
  `PRACTICA_REGULAR = 1`, `PRACTICA_EXTRA = 0.5` y `PRESENTACION = 2`.
- RF-2. SI un estado de asistencia es `JUSTIFICADO` o `PRESENTE`, ENTONCES no
  descuenta nada; solo `AUSENTE` descuenta.
- RF-3. MIENTRAS el lapso **no** esté `CLOSED`, EL SISTEMA cuenta solo los
  eventos elegibles con `event_date <= hoy`.
- RF-4. SI el lapso está `CLOSED`, ENTONCES EL SISTEMA cuenta todos los eventos
  `SCHEDULED` del lapso, sin filtrar por fecha.
- RF-5. SI existe un override para el par (estudiante, lapso), ENTONCES EL
  SISTEMA devuelve el valor del override y no calcula la fórmula ni evalúa la
  elegibilidad.
- RF-6. SI no hay ningún evento elegible (N=0) y no hay override, ENTONCES EL
  SISTEMA responde que el estudiante no tiene nota para ese lapso.
- RF-7. EL SISTEMA nunca devuelve una nota menor que 0 (el descuento se topa en
  0) ni mayor que 20.
- RF-8. EL SISTEMA redondea la nota calculada a **un decimal** con el redondeo
  centralizado (`shared/round.rs`); el override se devuelve con su valor exacto
  almacenado (hasta dos decimales), sin redondear.
- RF-9. CUANDO el instructor consulta las notas de una sección en un lapso
  (`GET /lapses/{id_lapse}/sections/{id_section}/grades`), EL SISTEMA devuelve,
  para cada estudiante **activo** de la sección (más los que tengan asistencia
  en el lapso aunque estén de baja) y en el orden canónico de estudiantes
  (`lower(last_name), lower(first_name), id`), su nota o `null` si no tiene.
- RF-10. SI el lapso consultado no existe, ENTONCES EL SISTEMA responde 404.
- RF-11. SI la sección consultada no existe, ENTONCES EL SISTEMA responde 404.
- RF-12. EL SISTEMA calcula la nota de cada estudiante de forma independiente:
  la ausencia de datos de un estudiante no afecta la nota de otro.
- RF-13. EL SISTEMA no persiste la nota calculada: se recalcula en cada consulta
  a partir de la asistencia vigente (solo el override se guarda).

## Requisitos no funcionales

- RNF-1. `compute_grade` es una función pura: sin `async`, sin `sqlx`, sin
  acceso a base de datos; recibe conteos y devuelve la nota.
- RNF-2. El módulo es hexagonal **completo** (el único del proyecto): `domain.rs`
  puro, `ports.rs` (`trait GradeRepository`), `adapter_pg.rs` (implementación
  `sqlx`), `service.rs` y `handler.rs`.
- RNF-3. Nunca `f64`: la nota se representa con `rust_decimal::Decimal`
  (constitución y `AGENTS.md`). Los pesos viven en un único lugar del dominio.
- RNF-4. El repositorio trae **conteos agregados** (una fila por estudiante,
  con los ausentes por tipo y N), no filas de asistencia una por una; el dominio
  no sabe que Postgres existe.
- RNF-5. Cada consulta vive en su propio archivo `.sql` y se carga con
  `sqlx::query_file!`/`query_file_as!` (constitución nº 1); nada de SQL inline.
- RNF-6. El trait `GradeRepository` se prueba con una implementación falsa en
  memoria (sin Postgres): es la razón de ser del desacople.
- RNF-7. Mensajes en español, identificadores `snake_case`, sin `unwrap`/`expect`
  fuera del arranque.
- RNF-8. Tests de integración con patrón transacción/rollback y bordes HTTP con
  `oneshot`; años de lapso exclusivos por test.

## Casos límite

- Lapso sin ningún evento: todos los estudiantes de la sección salen con
  `null` (N=0), no con 20.
- Lapso abierto con eventos todos futuros: N=0 → `null`.
- Lapso abierto con un evento pasado `PRESENTE` y uno futuro `AUSENTE`: el
  futuro no cuenta (el lapso no está cerrado), la nota es 20.
- Lapso cerrado con un evento pasado `AUSENTE`: cuenta (20 − peso), aunque el
  evento sea anterior a la consulta, porque el lapso ya está `CLOSED`.
- Mismo estudiante, mismo lapso, con override y con muchos `AUSENTE`: prevalece
  el override aunque la fórmula diera menos.
- Override en 0 (válido) vs sin nota (`null`): son cosas distintas; 0 sí se
  devuelve como 0.
- Estudiante activo de la sección sin ninguna asistencia: sale con `null`.
- Estudiante dado de baja con asistencia en el lapso: aparece con su nota.
- Descuento mayor que 20 (muchos `AUSENTE`): la nota se topa en 0, nunca negativa.
- Un evento `CANCELLED`: no es elegible (no cuenta, no descuenta).

## Fuera de alcance

- Endpoint de escritura de overrides (crear/editar/borrar): por ahora el cálculo
  solo **lee** `grade_overrides`. Se agrega cuando el frontend lo pida.
- Historial de notas por lapso cerrado (snapshot persistido): la nota se recalcula
  en cada consulta (RF-13).
- Exportar la nota en boletines o actas: no existe todavía.
- Nota final del año o promedio entre lapsos: fuera de alcance.
- Consulta individual de la nota de un estudiante (el cálculo es por sección;
  si el frontend la necesita, se agrega después).

## Criterios de finalización

- RF-1 a RF-13 verificados con tests: el dominio puro con un repositorio falso
  (RNF-6) y la integración con `tx`/`rollback`; los bordes HTTP (404 de lapso y
  sección) con `oneshot`.
- `cargo test` verde y `cargo clippy --all-targets -D warnings` 0/0.
- Spec marcada `implementada`, `MEMORY.md` actualizado y commit de cierre.

## Dudas abiertas

Resueltas en la clarificación del 2026-10-10:

- **Forma del endpoint: por lapso y sección.** Coincide con `AGENTS.md` ("el
  endpoint por lapso y sección") y con lo que pinta el frontend (una tabla de la
  sección). No se conserva el endpoint por estudiante.
- **N=0: sin nota.** Si no hay eventos elegibles y no hay override, el
  estudiante sale con `null`; no se inventa un 20 ni un 0.
- **Overrides: solo lectura.** El cálculo respeta el override si existe, pero no
  se exponen endpoints para crearlo o editarlo en esta fase.
- **Redondeo: un decimal** vía `shared/round.rs`. Como los pesos son 1, 0.5 y 2,
  el cálculo ya cae en múltiplos de 0.5; el redondeo es la regla general, no un
  caso especial.
- **Estudiante de baja con asistencia:** aparece en el listado (misma regla que
  el padrón de la fase 7), para no perder la evidencia de un lapso ya cursado.
