# Spec 002 — Secciones (alta, consulta, edición y baja)

Estado: aprobada

## Contexto y objetivo

Civik organiza la instrucción premilitar tomando como unidad mínima a la
sección: es a la que pertenecen los estudiantes y sobre la que se calcula la
nota de cada lapso. Hoy no hay ninguna forma de administrar secciones, y sin
secciones no se puede matricular a un solo estudiante: esta es la pieza que
bloquea todo lo demás.

Esta versión entrega el ciclo de vida completo de una sección —alta,
consulta, edición, baja y reactivación— y fija por escrito cómo se comporta
su identidad única, porque es el dato del que dependen los estudiantes, los
eventos y las notas.

## Usuarios

- **Instructor**: único usuario del sistema. A la vez administra y consulta.

## Historias de usuario

- HU-1. Como instructor, quiero registrar una sección con su grado, su
  subdivisión y su turno, para poder matricular estudiantes en ella.
- HU-2. Como instructor, quiero ver todas las secciones activas en una sola
  lista ordenada desde 1ro hasta 5to, y también verlas grado por grado, para
  elegir en cuál tomar la asistencia del día. La sección elegida despliega
  la lista completa de sus estudiantes, de una sola vez, y yo marco a cada
  uno.
- HU-3. Como instructor, quiero consultar una sección por su identificador,
  para ver sus datos y si sigue vigente.
- HU-4. Como instructor, quiero corregir el grado, la subdivisión o el turno
  de una sección, para arreglar un error de captura.
- HU-5. Como instructor, quiero dar de baja una sección sin perderla, para
  que deje de ofrecerse y su historial siga existiendo.
- HU-6. Como instructor, quiero volver a habilitar una sección, para
  reutilizarla en el período siguiente.
- HU-7. Como instructor, quiero eliminar una sección que nunca recibió
  alumnos, para no dejar secciones sobrantes en la lista si me equivoqué al
  crearla.

## Definiciones

- **Sección**: la combinación de grado, subdivisión y turno. Esa combinación
  *es* su identidad: la sección no tiene un nombre aparte.
- **Grado**: el nivel. Es un catálogo cerrado: `1ro`, `2do`, `3ro`, `4to`,
  `5to`. No hay otros grados y no se agregarán.
- **Subdivisión**: la parte que distingue dentro de un grado y turno (`a`,
  `b`, `c`). Se escribe y se muestra siempre en minúscula.
- **Turno**: `MATUTINO` o `VESPERTINO`. Solo esos dos. Se guarda en
  mayúsculas.
- **Baja lógica**: marcar la sección como inactiva, es decir, que deja de
  ofrecerse. No borra nada.
- **Reactivar**: volver a marcar como activa una sección dada de baja.
- **Eliminar**: borrar la fila de la sección de la base. Es la excepción
  documentada de "nada se borra físicamente", y solo existe para secciones sin
  ningún estudiante. No es lo mismo que dar de baja: la baja conserva todo el
  historial, la eliminación no deja rastro.
- **Sección sin subgrupos**: una sección no se divide en grupos internos.
  Todos sus estudiantes se presentan juntos, en una sola lista, sin
  fraccionamiento. Al tomar asistencia de un día, el instructor ve a la
  totalidad de sus alumnos de la sección y marca uno por uno. Esta spec no
  inventa subgrupos, y una spec futura tampoco puede hacerlo.

## Requisitos funcionales

- RF-1: CUANDO el instructor da de alta una sección, EL SISTEMA la registra
  con grado, subdivisión y turno, y devuelve su identificador.
- RF-2: EL SISTEMA guarda el grado y la subdivisión siempre en minúscula,
  sin importar cómo los escriba el instructor.
- RF-3: EL SISTEMA guarda el turno siempre en mayúsculas, sin importar cómo
  lo escriba el instructor.
- RF-4: EL SISTEMA recorta los espacios de los extremos del grado y de la
  subdivisión antes de guardarlos.
- RF-5: SI el grado o la subdivisión quedan vacíos después de recortar,
  ENTONCES EL SISTEMA rechaza el alta y explica que el dato es obligatorio.
- RF-6: SI el turno, ya normalizado a mayúsculas, no corresponde a ninguno de
  los turnos admitidos, ENTONCES EL SISTEMA rechaza el alta e informa que el
  valor no es válido.
- RF-7: SI el grado no corresponde a ninguno de los grados admitidos,
  ENTONCES EL SISTEMA rechaza el alta, informa que el valor no es válido y
  enumera los grados admitidos.
- RF-8: SI ya existe una sección con la misma combinación de grado,
  subdivisión y turno —esté activa o dada de baja—, ENTONCES EL SISTEMA
  rechaza el alta por combinación duplicada, indicando que ya está en uso.
- RF-9: CUANDO el instructor pide el listado, EL SISTEMA devuelve en una sola
  respuesta todas las secciones activas, sin obligar a recorrerlas una por
  una.
- RF-10: CUANDO el instructor pide el listado de un grado concreto, EL
  SISTEMA devuelve únicamente las secciones activas de ese grado, en el
  mismo formato y con el mismo orden que el listado completo.
- RF-11: CUANDO el instructor pide cualquier listado, EL SISTEMA lo devuelve
  en una lista única y plana, ordenada por grado desde `1ro` hasta `5to` y,
  dentro de cada grado, por subdivisión en orden alfabético.
- RF-12: CUANDO el instructor consulta una sección por su identificador,
  EL SISTEMA devuelve sus datos, incluido si está activa o dada de baja.
- RF-13: SI el identificador consultado no corresponde a ninguna sección,
  ENTONCES EL SISTEMA responde que no se encontró.
- RF-14: CUANDO el instructor edita una sección, EL SISTEMA guarda el grado y
  la subdivisión en minúscula y recortados, y el turno en mayúsculas, con la
  misma normalización del alta.
- RF-15: SI la edición deja el grado o la subdivisión vacíos, ENTONCES EL
  SISTEMA rechaza la edición y explica que el dato es obligatorio.
- RF-16: SI la edición deja un grado o un turno fuera de los admitidos,
  ENTONCES EL SISTEMA rechaza la edición, informa que el valor no es válido y
  enumera los admitidos.
- RF-17: SI la edición produce una combinación que ya pertenece a otra
  sección, ENTONCES EL SISTEMA rechaza la edición por combinación duplicada.
- RF-18: CUANDO el instructor da de baja una sección, EL SISTEMA la marca
  como inactiva sin eliminar ningún dato.
- RF-19: CUANDO el instructor da de baja una sección que ya está dada de
  baja, EL SISTEMA no modifica nada y responde con éxito.
- RF-20: CUANDO el instructor reactiva una sección dada de baja, EL SISTEMA
  vuelve a marcarla como activa.
- RF-21: CUANDO el instructor reactiva una sección que ya está activa, EL
  SISTEMA no modifica nada y responde con éxito.
- RF-22: CUANDO el instructor elimina físicamente una sección que no tiene
  ningún estudiante asociado, EL SISTEMA la elimina y no deja rastro de ella.
- RF-23: SI la sección que el instructor intenta eliminar tiene al menos un
  estudiante asociado —activo o inactivo—, ENTONCES EL SISTEMA rechaza la
  eliminación, no borra nada e informa que la sección tiene estudiantes.
- RF-24: MIENTRAS una sección esté inactiva, EL SISTEMA la omite del
  listado.
- RF-25: EL SISTEMA conserva el historial de una sección dada de baja: sus
  estudiantes, sus eventos y sus notas siguen existiendo y son consultables.
- RF-26: SI se edita o se da de baja una sección que tiene estudiantes
  asociados, ENTONCES EL SISTEMA aplica el cambio y los estudiantes
  permanecen en ella.
- RF-27: EL SISTEMA registra toda alta, edición, baja, reactivación y
  eliminación física de una sección, con su fecha y los valores involucrados,
  en el historial de auditoría.

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

- `" 1RO "` se guarda y se devuelve como `1ro`; si ya existe esa sección, es
  combinación duplicada.
- El turno escrito como `matutino` o `Matutino` se guarda como `MATUTINO`.
- Un turno que no existe, como `NOCTURNO`, se rechaza aunque venga en
  mayúsculas.
- Un grado fuera del catálogo, como `quinto` o `6to`, se rechaza y el error
  enumera los cinco admitidos.
- Crear `1ro/a/matutino` cuando ya existe una dada de baja con esa misma
  combinación: se rechaza por duplicado. La combinación no se libera nunca.
- Editar una sección para dejarla exactamente como ya está: se acepta, sin
  duplicarse consigo misma.
- Consultar el identificador de una sección dada de baja: se devuelve, marcada
  como inactiva.
- Una subdivisión más larga que el máximo admitido: se rechaza con un mensaje
  que diga cuál dato excede el límite.
- Altas y ediciones repetidas con la misma carga: a partir de la segunda, se
  rechazan por duplicado.
- Un listado sin secciones activas: se devuelve una lista vacía, no un error.

- Una sección que alguna vez tuvo estudiantes ya no se puede eliminar
  físicamente, aunque hoy no tenga ninguno: los estudiantes no se borran, solo
  se dan de baja, y su vínculo con la sección sigue existiendo.
- Intentar eliminar una sección dada de baja que nunca tuvo estudiantes: se
  elimina igual. La baja previa no bloquea la eliminación.
- Intentar eliminar una sección con un solo estudiante inactivo: se rechaza,
  porque el estudiante sigue siendo un dato vivo.

## Fuera de alcance

- Matricular, consultar o mover estudiantes: es la Fase 3.
- Lapsos, eventos, asistencia y notas.
- Subgrupos, grupos o fraccionamiento de estudiantes dentro de una sección.
- Agregar grados nuevos al catálogo.
- Reasignar estudiantes de una sección a otra.
- Reactivar automáticamente una sección que se haya dado de baja al terminar
  un lapso: la reactivación la pide siempre el instructor.
- Recorrer el historial de auditoría de una sección desde la API: se escribe,
  pero no se expone todavía.
- Historial de cambios más detallado que el registro de auditoría (por
  ejemplo, quién exactamente, o un diff de valores).

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

- (ninguna)