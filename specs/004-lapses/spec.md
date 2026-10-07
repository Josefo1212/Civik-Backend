# Spec 004 — Lapsos (alta, consulta, edición, activación, cierre y borrado del lapso vacío)

Estado: aprobada

## Contexto y objetivo

Civik evalúa por períodos: cada año escolar se divide en tres lapsos, y sobre
la asistencia a los eventos de un lapso se calcula la nota final de ese lapso.
Todos los módulos posteriores dependen de este: `events` referencia a un lapso,
y los cálculos de `grades` agregan por lapso. Sin el lapso creado y activado
no hay práctica que programar ni nada que calificar.

Esta fase entrega el ciclo de vida completo del lapso —alta, consulta,
listado, edición, activación y cierre— y define qué se puede y qué no se puede
hacer según el estado (`PLANNED`, `ACTIVE`, `CLOSED`). Es también la fase donde
se confirma por primera vez en el proyecto el patrón de `domain.rs` +
`service.rs` con transacción y auditoría, que luego usan `events`, `attendance`
y `grades`.

La base ya impone las invariantes duras (un solo lapso activo, fechas sin
solape, `CLOSED` inmutable, guardarraíl del borrado). El servidor las
**comunica en español** y valida lo que la base no puede decir con un mensaje
claro (número fuera de 1–3, fechas invertidas, dato faltante).

## Usuarios

- **Instructor**: único usuario del sistema. A la vez administra y consulta.

## Historias de usuario

- HU-1. Como instructor, quiero registrar un lapso con su año escolar, número,
  fecha de inicio, fecha de fin y día de práctica, para organizar el período.
- HU-2. Como instructor, quiero ver la lista de los lapsos con su estado, para
  saber cuál es el período vigente.
- HU-3. Como instructor, quiero activar el lapso que comienza, para que el
  sistema sepa que es el período en curso.
- HU-4. Como instructor, quiero cerrar el lapso al terminar el período, para
  fijarlo como historial y dejar de admitir datos en él.
- HU-5. Como instructor, quiero corregir los datos de un lapso, para arreglar
  un error de captura.
- HU-6. Como instructor, quiero borrar un lapso que creé por error y que aún
  no se usó, para no dejar descartes en el historial.

## Definiciones

- **Lapso**: un período de evaluación dentro de un año escolar, con fechas de
  inicio y fin y un único día de práctica semanal (`practice_day`). Se
  identifica por el par (año escolar, número). El texto "1er/2do/3er lapso" se
  deriva del número y no se guarda.
- **Estado del lapso**: `PLANNED` (registrado, aún no vigente), `ACTIVE`
  (vigente; el sistema admite un solo lapso activo a la vez) y `CLOSED`
  (cerrado, terminal e inmutable).
- **Convivir con el calendario**: el año escolar es una etiqueta (el año en
  que inicia) y puede abarcar fechas del calendario siguiente (por ejemplo
  septiembre–julio). Las fechas de un lapso no tienen que caer dentro del año
  de calendario que los etiqueta, y **ninguna operación de alta, edición o
  activación compara contra la fecha de hoy**: en Civik un período vale por su
  evidencia, no porque "hoy" caiga dentro de sus fechas. La única comparación
  con el presente vive en la base y es de otro tipo: el guardarraíl del borrado
  impide eliminar un lapso cuya fecha de fin ya pasó, porque eso es historial.

## Requisitos funcionales

- RF-1: CUANDO el instructor registra un lapso con año escolar, número, fecha
  de inicio, fecha de fin y día de práctica, EL SISTEMA lo crea en estado
  `PLANNED` y devuelve su identificador.
- RF-2: SI el año escolar no está entre 2000 y 2100, o el número está fuera de
  1 a 3, o la fecha de fin es anterior a la de inicio, o el día de práctica no
  es un día válido, ENTONCES EL SISTEMA rechaza el alta e indica el dato que
  falló y por qué.
- RF-3: SI ya existe un lapso con el mismo año escolar y número, ENTONCES EL
  SISTEMA rechaza el alta e informa la duplicación.
- RF-4: SI las fechas del nuevo lapso se solapan con las de otro lapso ya
  existente, ENTONCES EL SISTEMA rechaza el alta e informa el solapamiento.
- RF-5: CUANDO el instructor pide el listado de lapsos, EL SISTEMA devuelve
  todos, ordenados por año escolar descendente y por número de lapso
  ascendente, cada uno con su estado.
- RF-6: CUANDO el instructor consulta un lapso por su identificador, EL SISTEMA
  devuelve sus datos y su estado.
- RF-7: SI el identificador consultado no corresponde a ningún lapso, ENTONCES
  EL SISTEMA responde que no se encontró.
- RF-8: CUANDO el instructor edita un lapso que no está `CLOSED`, EL SISTEMA
  valida los campos con las mismas reglas del alta y los aplica, sin tocar el
  estado.
- RF-9: SI la edición intenta modificar un lapso `CLOSED`, ENTONCES EL SISTEMA
  la rechaza e informa que el lapso está cerrado y es inmutable.
- RF-10: SI la edición dejaría duplicados (año, número) o fechas solapadas con
  otro lapso, ENTONCES EL SISTEMA la rechaza e informa la causa.
- RF-11: SI la edición de las fechas dejaría eventos fuera del lapso, ENTONCES
  EL SISTEMA la rechaza; la invariante vive en la base (trigger) y se demuestra
  con SQL directo, porque el endpoint de eventos todavía no existe.
- RF-12: CUANDO el instructor activa un lapso `PLANNED`, EL SISTEMA lo pasa a
  `ACTIVE`.
- RF-13: SI ya existe otro lapso `ACTIVE`, ENTONCES EL SISTEMA rechaza la
  activación e informa que el sistema solo admite un lapso activo.
- RF-14: SI el instructor intenta activar un lapso que no está `PLANNED`,
  ENTONCES EL SISTEMA lo rechaza e indica su estado actual.
- RF-15: CUANDO el instructor cierra un lapso `ACTIVE`, EL SISTEMA lo pasa a
  `CLOSED`; desde entonces no admite datos nuevos ni cambios, y no se reabre.
- RF-16: SI el instructor intenta cerrar un lapso que no está `ACTIVE`,
  ENTONCES EL SISTEMA lo rechaza e indica su estado actual.
- RF-17: CUANDO el instructor elimina un lapso vacío, no `CLOSED` y con fecha
  de fin hoy o futura, EL SISTEMA lo borra físicamente.
- RF-18: SI el lapso a eliminar está `CLOSED`, ya terminó o tiene eventos u
  overrides asociados, ENTONCES EL SISTEMA rechaza la eliminación e indica el
  motivo (el guardarraíl está en la base).
- RF-19: EL SISTEMA registra en el historial de auditoría toda alta, edición,
  activación, cierre y eliminación de un lapso, con su fecha y los valores
  involucrados, en la misma transacción que el cambio.

## Requisitos no funcionales

- RNF-1: Toda validación ocurre en el servidor: ningún dato se guarda sin
  validar, aunque el cliente lo envíe mal.
- RNF-2: Ninguna operación deja datos a medias: o se guarda el cambio completo
  con su auditoría, o no se guarda nada.
- RNF-3: Un mismo conjunto de peticiones produce siempre la misma respuesta,
  en el mismo orden, para que dos dispositivos sin conexión puedan reconciliar
  sin ambigüedad.
- RNF-4: Los mensajes de error dicen qué dato falló y por qué, en español, y no
  exponen detalles técnicos.
- RNF-5: Cada requisito funcional queda cubierto por una prueba automática o,
  si es un borde de protocolo, por una verificación directa contra el endpoint.

## Casos límite

- Lapso con fecha de fin anterior a la de inicio: se rechaza indicando que las
  fechas están invertidas.
- Dos lapsos distintos del mismo año con el mismo número: el segundo se
  rechaza por duplicado.
- Crear un lapso cuyas fechas caen dentro de otro ya existente: se rechaza por
  solapamiento (el índice `EXCLUDE` con rango cerrado `[]` trata extremos
  compartidos como solape).
- Activar dos lapsos seguidos: el segundo se rechaza porque el primero quedó
  `ACTIVE`.
- Activar un lapso cuyas fechas ya empezaron a correr: se acepta sin
  comparaciones contra "hoy" (ver Definiciones).
- Cerrar un lapso que nunca se activó: se rechaza (o se acepta según la
  decisión de la clarificación; ver Dudas abiertas).
- Editar dejando los datos exactamente como estaban: se acepta, sin efecto.
- Un listado sin lapsos devuelve una lista vacía, no un error.
- Borrar un lapso recién creado: se borra físicamente y es la única
  excepción a "nada se borra físicamente".
- Un lapso `CLOSED`, por vacío que esté, nunca se borra.

## Fuera de alcance

- Eventos y prácticas (manuales y automáticas): fases 5 y 6. Activarlo no
  genera prácticas.
- `reprogramar_practice_day` y la orquestación automática: fase 6.
- Asistencia y sincronización offline: fases 7 y 9.
- Cálculo de notas y overrides: fase 8.
- Reabrir un lapso `CLOSED`: no existe y no se va a crear.
- Más de un lapso `ACTIVE` a la vez: no existe y no se va a crear.
- Cambiar el estado de un lapso desde la edición: el estado solo se mueve por
  los endpoints de activar y cerrar.
- Filtros en el listado de lapsos (por año, por estado): se agregan si el
  frontend los pide; el listado completo es pequeño.
- Recorrer el historial de auditoría del lapso desde la API: se escribe, pero
  no se expone todavía.

## Criterios de finalización

- Cada requisito funcional está cubierto por una prueba o por una verificación
  directa al endpoint, con el resultado registrado.
- Las pruebas se ejecutan sobre la base de desarrollo y no dejan datos detrás:
  todo lo que crean se descarta al terminar.
- Los mensajes de error de la API están escritos en español.
- No quedan advertencias del analizador estático al compilar.
- `MEMORY.md` queda actualizado al cerrar la fase.

## Dudas abiertas

Resueltas en la clarificación del 2026-10-07:

- **Edición durante `ACTIVE`**: se permite; solo el `CLOSED` es inmutable. El
  cambio de `practice_day` en un lapso vigente es lo que en la fase 6 dispara
  `reprogramar_practice_day`.
- **Cierre solo desde `ACTIVE`**: un lapso `PLANNED` nunca se cierra; si no se
  usó, se deshace con el borrado físico.
- **Alcance de la edición**: incluye el par (año escolar, número), las fechas
  y el `practice_day`; la unicidad y el solape se siguen validando. El estado
  no se edita.
- **Borrado físico (RF-17/RF-18)**: se incluye en esta fase; la constitución
  lo permite para el lapso vacío, no cerrado y no terminado, y la base ya
  tiene el guardarraíl.
- **Sin comparaciones contra "hoy"** en alta, edición y activación: el lapso
  del primer término debe poder registrarse y activarse con su primera clase
  ya pasada.