# Spec 003 — Estudiantes (alta individual, consulta, edición, baja y listado por sección)

Estado: implementada

## Contexto y objetivo

Civik organiza la instrucción premilitar en secciones; el estudiante es la
segunda unidad del sistema: pertenece a una sección, y sobre sus asistencias
se calcula la nota de cada lapso. Con la Fase 2 ya se administran las
secciones; sin estudiantes, sin embargo, la sección no tiene contenido y no
se puede tomar asistencia.

Esta versión entrega el ciclo de vida completo de un estudiante —alta
individual, consulta, edición (incluido el cambio de sección), baja y
reactivación— y fija por escrito cómo se normalizan sus datos personales y
cómo se ordena su padrón, porque de ese orden depende la lista con la que el
instructor marca la asistencia.

## Usuarios

- **Instructor**: único usuario del sistema. A la vez administra y consulta.

## Historias de usuario

- HU-1. Como instructor, quiero registrar a un estudiante en una sección con
  su apellido, su nombre(primer nombre y apellido), no se guardara cedula.
- HU-2. Como instructor, quiero ver en una sola llamada la lista de los
  estudiantes activos de una sección, ordenada por apellido y nombre, para
  marcar la asistencia recorriéndola en ese mismo orden.
- HU-3. Como instructor, quiero consultar los datos de un estudiante por su
  identificador, para ver a qué sección pertenece y si sigue activo.
- HU-4. Como instructor, quiero corregir los datos de un estudiante —nombre,
  apellido, teléfono e incluso su sección— para arreglar un error de
  captura.
- HU-5. Como instructor, quiero dar de baja a un estudiante sin perderlo,
  para que deje de aparecer en la toma de asistencia y su historial siga
  existiendo.
- HU-6. Como instructor, quiero volver a activar a un estudiante que di de
  baja, para reincorporarlo.

## Definiciones

- **Estudiante**: una persona matriculada en una sección.la cédula existe pero yo no se las pedire, no se guardara cedula en la app; sin cédula,
  la fila vale por sí misma.
- **Apellido y nombre**: el apellido es el primer apellido o apellidos
  (`Pérez`), y el nombre el primer nombre o nombres (`María José`). Se
  guardan en campos separados y se muestran juntos como `apellido, nombre`.

- **Baja lógica**: marcar al estudiante como inactivo. No borra nada; sigue
  perteneciendo a su sección y su historial permanece consultable.
- **Reactivar**: volver a marcar como activo a un estudiante inactivo.
- **Texto tal como se escribe**: los nombres se conservan con la
  capitalización que tecleó el instructor (`Ana María Pérez`, `De La Cruz`),
  solo recortados de espacios en los extremos y con los espacios repetidos
  colapsados a uno. El sistema no decide cómo se escriben los nombres de las
  personas.

## Requisitos funcionales

- RF-1: CUANDO el instructor registra a un estudiante en una sección, EL
  SISTEMA lo crea con apellido, nombre y sección, teléfono si se
  informan, y devuelve su identificador.
- RF-2: SI faltan el apellido o el nombre, o quedan vacíos después de recortar
  espacios, ENTONCES EL SISTEMA rechaza el alta y explica que el dato es
  obligatorio.
- RF-3: SI la sección indicada no existe, ENTONCES EL SISTEMA rechaza el alta
  e informa que la sección no existe.
- RF-4: SI la sección indicada está dada de baja, ENTONCES EL SISTEMA rechaza
  el alta e informa que la sección está inactiva.
- RF-5: SI la cédula indicada ya pertenece a otro estudiante —activo o
  inactivo—, ENTONCES EL SISTEMA rechaza el alta e informa que la cédula está
  en uso.(omite este requisito porque no habra cedula)
- RF-6: EL SISTEMA recorta los espacios de los extremos y los espacios
  repetidos de apellido, nombre,  y teléfono, y conserva la
  capitalización tal como la escribió el instructor.
- RF-7: CUANDO la cédula no se informa o queda vacía después de recortar,
  ENTONCES EL SISTEMA la guarda como ausente, y la ausencia no impide que
  otros estudiantes tampoco la tengan.(ignora esto, en la base de datos no hay campo cedula)
- RF-8: CUANDO el instructor pide la lista de una sección, EL SISTEMA
  devuelve en una sola llamada los estudiantes activos, ordenados por
  apellido y luego por nombre, sin distinguir mayúsculas.
- RF-9: CUANDO el instructor pide la lista de una sección incluyendo a los
  inactivos, EL SISTEMA los devuelve junto a los activos, en el mismo orden
  y marcando el estado de cada uno.
- RF-10: CUANDO el instructor consulta un estudiante por su identificador, EL
  SISTEMA devuelve sus datos personales, su sección y su estado.
- RF-11: SI el identificador consultado no corresponde a ningún estudiante,
  ENTONCES EL SISTEMA responde que no se encontró.
- RF-12: CUANDO el instructor edita un estudiante, EL SISTEMA aplica a
  apellido, nombre,  y teléfono la misma normalización y la misma
  validación que en el alta.
- RF-13: SI la edición deja vacío el apellido o el nombre, ENTONCES EL
  SISTEMA la rechaza y explica que el dato es obligatorio.
- RF-14: SI la edición asigna a un estudiante una cédula que ya pertenece a
  otro estudiante, ENTONCES EL SISTEMA la rechaza e informa que la cédula
  está en uso.
- RF-15: CUANDO la edición cambia la sección del estudiante, EL SISTEMA
  valida la nueva sección igual que en el alta: debe existir y estar activa.
- RF-16: MIENTRAS el estudiante no tenga asistencia registrada, EL SISTEMA
  permite cambiarle la sección; una vez que tenga asistencia, la regla 3 de
  la base lo impide y EL SISTEMA comunica el impedimento al instructor.
- RF-17: CUANDO el instructor da de baja a un estudiante, EL SISTEMA lo marca
  como inactivo sin eliminar sus datos, y lo omite del listado por defecto.
- RF-18: CUANDO el instructor da de baja a un estudiante que ya está
  inactivo, EL SISTEMA no modifica nada y responde con éxito.
- RF-19: CUANDO el instructor reactiva a un estudiante inactivo, EL SISTEMA
  lo vuelve a marcar como activo.
- RF-20: CUANDO el instructor reactiva a un estudiante que ya está activo, EL
  SISTEMA no modifica nada y responde con éxito.
- RF-21: EL SISTEMA registra toda alta, edición, baja y reactivación de un
  estudiante en el historial de auditoría, con su fecha y los valores
  involucrados.
- RF-22: EL SISTEMA no expone ninguna operación que borre físicamente a un
  estudiante.

## Requisitos no funcionales

- RNF-1: Toda normalización y toda validación ocurre en el servidor: ningún
  dato se guarda sin normalizar, aunque el cliente lo envíe mal.
- RNF-2: Ninguna operación deja datos a medias: o se guarda el cambio
  completo, o no se guarda nada.
- RNF-3: Un mismo conjunto de peticiones produce siempre la misma respuesta,
  en el mismo orden, para que dos dispositivos sin conexión puedan
  reconciliar sin ambigüedad.
- RNF-4: Los mensajes de error dicen qué dato falló y por qué, en español, y
  no exponen detalles técnicos.
- RNF-5: Cada requisito funcional queda cubierto por una prueba automática o,
  si es un borde de protocolo, por una verificación directa contra el
  endpoint.

## Casos límite

- `" Ana    María "` como nombre se guarda como `Ana María`: extremos
  recortados, espacios repetidos colapsados y capitalización intacta.
- `" V-12345678 "` como cédula se guarda como `V-12345678`.
- La cédula ya usada por un estudiante inactivo no se libera: registrar otra
  estudiante con esa misma cédula se rechaza.
- Dos estudiantes sin cédula conviven sin problema (la ausencia no colisiona).
- Silenciar la cédula al editar la deja sin cédula, aunque antes la tuviera.
- Registrar en una sección inexistente y en una dada de baja: dos errores
  distintos, ambos informan la causa.
- Mover al estudiante a la sección en la que ya está: se acepta, sin efecto.
- Editar dejando los datos exactamente como estaban: se acepta.
- Un listado sin estudiantes activos devuelve una lista vacía, no un error.
- Un apellido, nombre, cédula o teléfono más largos que su límite se rechaza
  indicando cuál dato excede el límite.
- Un estudiante dado de baja sigue apareciendo con su sección al consultarlo
  por identificador.

## Fuera de alcance

- Asistencia, eventos y notas: fases posteriores.
- Eliminación física de un estudiante: no existe y no se va a crear.
- Alta masiva de estudiantes en una sola llamada.
- Búsqueda global de estudiantes por nombre o cédula sin pasar por una
  sección.
- Teléfono con validación de formato: se guarda tal como se escribe,
  recortado.
- Recorrer el historial de auditoría del estudiante desde la API: se
  escribe, pero no se expone todavía.

## Criterios de finalización

- Cada requisito funcional está cubierto por una prueba o por una
  verificación directa al endpoint, con el resultado registrado.
- Las pruebas se ejecutan sobre la base de desarrollo y no dejan datos
  detrás: todo lo que crean se descarta al terminar.
- Los mensajes de error de la API están escritos en español.
- No quedan advertencias del analizador estático al compilar.
- Los siete principios de la constitución se cumplen, sin excepciones.
- `MEMORY.md` queda actualizado al cerrar la fase.

## Dudas abiertas

Resueltas en la clarificación del 2026-10-06:

- **Normalización de nombres**: se respeta la capitalización escrita, con
  extremos recortados y espacios repetidos colapsados.
- **Alta en sección inactiva**: se rechaza; una sección dada de baja no
  recibe cupo nuevo.
- **Cambio de sección**: la edición de un estudiante incluye moverlo de
  sección, válido mientras no tenga asistencia.
- **Alta masiva**: solo individual en esta fase.