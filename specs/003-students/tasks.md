# Tareas 003 — Estudiantes

En orden de dependencia. Cada tarea se cierra con un commit y deja la base
limpia (todo lo que crean los tests se descarta con el `rollback`).

## Tareas

- [x] **T1. `domain.rs`: normalización y validación de la entrada.** RF-2,
  RF-5, RF-10, RF-11
  - `colapsar_espacios`, `validar_no_vacio`, `validar_largo`,
    `preparar_entrada` y `ErrorEntrada`, puras.
  - Unit tests sin base de datos: colapso de blancos, capitalización
    intacta, nombres vacíos rechazados, largos, teléfono `""`→`None`, y que
    `preparar_entrada` es el mismo camino para alta y edición.
  - Hecho cuando: `cargo test` en verde solo con estos unit tests,
    `cargo clippy --all-targets -- -D warnings` limpio.

- [x] **T2. Infra de errores: `NotFound(String)`.**
  RF-3, RF-9
  - `src/shared/error.rs`: `AppError::NotFound(String)`;
    `mapear_violacion` pasa a `pub(crate)`.
  - `src/modules/sections/handler.rs`: `a_not_found` →
    `NotFound("No se encontró la sección")`.
  - Unit tests nuevos en `error.rs` para el `404` con mensaje; ajustar los
    existentes.
  - Hecho cuando: clippy limpio y suite completa en verde (las pruebas de la
    Fase 2 adaptadas).

- [x] **T3. `create.sql` + `crear`.** RF-1, RF-3, RF-4, RF-18
- [x] **T4. `get_by_id.sql` + `list_by_section.sql` + `consultar` + `listar_por_seccion`.** RF-6, RF-7, RF-8, RF-9, RNF-3, casos límite (sección inactiva consultable)
- [x] **T5. `update.sql` + `editar`.** RF-10, RF-11, RF-12, RF-13
- [x] **T6. `set_active.sql` + `baja`/`reactivar` idempotentes.** RF-14, RF-15, RF-16, RF-17, RF-18
- [x] **T7. `handler.rs` + rutas en `app.rs` + tests `oneshot`.** Bordes 201/200/400/404/409 en los seis endpoints, forma `snake_case`, `null` en teléfono

- [x] **T8. Validación RF por RF, llamada real y cierre de la fase.** RF-13
  (marcar parcial), RF-19 (verificación por no-existencia)
  - Tabla RF → test → veredicto en este archivo; bordes confirmados con
    `cargo run` + `curl` sobre los seis endpoints; spec 003 con `Estado:
    implementada`; `MEMORY.md` al día (Fase 3 cerrada, próxima Fase 4).

## Validación RF por RF (cierre de la fase)

Cada veredicto sale de una prueba que lo demuestra; el `curl` solo confirmó
los códigos de la superficie HTTP con el servidor real (todos: 201/200/400/
404/409/405 para el `DELETE` inexistente).

| RF | Qué la prueba demuestra | Veredicto |
|---|---|---|
| RF-1 | El alta persiste la fila, devuelve el id y audita | Cumple (`el_alta_persiste_la_fila_y_devuelve_identificador`) |
| RF-2 | Nombres vacíos o de solo espacios se rechazan antes de la base | Cumple (`apellido_solo_con_espacios_se_rechaza` + HTTP 400) |
| RF-3 | Alta en sección inexistente se informa | Cumple (`el_alta_en_seccion_inexistente_informa_que_no_existe` + 404) |
| RF-4 | Alta en sección inactiva se informa | Cumple (`el_alta_en_seccion_inactiva_...` + 409) |
| RF-5 | Recorte de extremos y espacios repetidos, capitalización intacta | Cumple (`el_alta_persiste_la_fila...` y `el_editar_normaliza_igual_que_el_alta`) |
| RF-6 | Listado por defecto: solo activos, orden `lower(apellido), lower(nombre)` | Cumple (`el_listado_por_defecto_devuelve_solo_activos_en_orden`) |
| RF-7 | `incluir_inactivas=true`: devuelve inactivos con su estado | Cumple (`incluir_inactivas_devuelve_tambien_las_inactivas_marcadas`) |
| RF-8 | Consulta por id devuelve sección y estado | Cumple (`consultar_devuelve_el_estudiante_incluido_su_estado`) |
| RF-9 | Id inexistente responde que no se encontró | Cumple (`consultar_un_identificador_inexistente_...` + 404) |
| RF-10 | La edición normaliza y valida igual que el alta | Cumple (`el_editar_normaliza_igual_que_el_alta`, mismo `preparar_entrada`) |
| RF-11 | Editar a vacío se rechaza (punto único + CHECK de la base) | Cumple (`apellido_vacio_se_rechaza` y `los_nombres_en_blanco_se_rechazan_aunque_se_salte_el_dominio`, 23514) |
| RF-12 | Mover a sección inexistente/inactiva se rechaza; a activa funciona; sin cambio no se valida otra vez | Cumple (los tres casos en `tests/students.rs` + 409/404 en `curl`) |
| RF-13 | Primer término (mover sin asistencia) | Cumple (`el_editar_mueve_a_otra_seccion_activa_y_la_valida`) |
| RF-13 | Segundo término (regla 3 con asistencia) | **Parcial**: garantizado por `trg_students_section_immutable` de la init; se ejercita recién en la Fase 7 (tasks.md, notas) |
| RF-14 | La baja marca inactivo y lo omite del listado por defecto | Cumple (`la_baja_marca_inactivo_y_lo_omite_del_listado_por_defecto`) |
| RF-15 | Bajar dos veces: `200` y `updated_at` no cambia | Cumple (`bajar_dos_veces_no_modifica_nada_y_es_exitoso`) |
| RF-16 | La reactivación deja `active = true` y vuelve al listado | Cumple (`la_reactivacion_vuelve_el_estudiante_al_listado`) |
| RF-17 | Reactivar dos veces: `200` sin cambios | Cumple (`reactivar_dos_veces_no_modifica_nada`) |
| RF-18 | Cada operación escribe su auditoría; la idempotente no deja falsa | Cumple (`el_alta_deja_su_entrada_en_el_historial`, `el_editar_deja_su_entrada_de_auditoria`, `la_baja_idempotente_no_deja_auditoria_falsa`, `la_reactivacion_deja_su_entrada_de_auditoria`) |
| RF-19 | No existe ruta de borrado | Cumple por ausencia: no hay `delete.sql`, no hay ruta `DELETE`, y `curl -X DELETE /students/{id}` respondió `405` |
| RNF-3 | El orden del listado es estable entre llamadas | Cumple (`el_orden_del_listado_es_estable_entre_llamadas`) |

**Cierre con `curl` contra `127.0.0.1:3000`** (todo respondió con éxito de
operación o el error esperado): alta 201 (normalización y `snake_case`
verificadas en el cuerpo), apellido en blanco → 400, sección inexistente →
404, sección inactiva → 409, listado con y sin `incluir_inactivas` (orden y
marcado verificados), parámetro malo → 400, consulta 200, consulta
inexistente 404, `DELETE` → 405, edición 200 (mueve y normaliza),
edición a sección inexistente 404, a inactiva 409, a inexistente 404,
nombre en blanco 400, baja/reactivación 200 y la repetida también 200, baja
de inexistente 404. La base quedó como estaba (limpieza SQL directa).

**Fase 3 cerrada — 19 RF con veredicto, tests en verde, clippy 0/0, spec 003
`Estado: implementada`.**

**Corrección de la cédula aplicada** (retroactiva): `national_id` y
`uq_students_national_id` se retiran de `students` con la migración
`remove_students_national_id`; la spec ya la declaraba fuera desde la
clarificación. Los RF se renumeraron sin huecos de 22 → 19.

## Notas de ejecución

- **T1 no necesita base de datos** — como en la 002, es la tarea para hacer
  si el entorno está caído.
- **T2 toca código de la Fase 2** (`sections/handler.rs` y pruebas de
  `error.rs`): es un cambio deliberado del plan §8.1, con ajuste explícito
  de los usos existentes.
- **RF-13 tiene dos términos**: el primero (mover de sección sin asistencia)
  se prueba en T5; el segundo (la regla 3 con asistencia) lo garantiza el
  trigger `trg_students_section_immutable` de la init y se ejecutará recién
  en la Fase 7. No es una omisión.
- **RF-19 (sin borrado físico)** se verifica por ausencia: no hay ruta, no
  hay query `delete.sql`, y los tests nunca intentan borrar un estudiante.
- **La corrección de la cédula**: la spec 003 la declaró fuera desde la
  clarificación (RF-5/RF-7/RF-14 marcados *omitir*) y la primera
  implementación la incluyó por error. Se retiró con la migración
  `remove_students_national_id`, quedando la spec con 19 RF renumerados sin
  huecos.
- **Los tests HTTP commitean** (el handler commitea): por eso usan datos
  únicos y limpian con SQL directo al final, igual que `http_sections`.
- **Ninguna tarea deja datos en `civik_dev`**: si una corrida deja filas, es
  un error en el test, no en la base.