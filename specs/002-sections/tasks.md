# Tareas 002 — Secciones

División de `plan.md` en tareas ejecutables. Cada tarea se hace **completa y
parada**: test primero, código después, `cargo test` en verde, `cargo clippy`
sin warnings, marcar la casilla y parar. No se empieza la siguiente sin que la
anterior esté marcada.

Orden de dependencia estricto. T1 desbloquea a todas: `sqlx` verifica cada
consulta contra el esquema real al compilar, así que hasta que `active`
exista en la base, ningún `.sql` que la mencione compila.

---

- [x] **T1. Migración de `active` y del `CHECK` de no vacío.**
  RF-18, RF-20, RF-24, RNF-1
  - Crear con `dbmate new sections_active_and_not_blank` en `db/migrations/`,
    sin tocar el archivo `20260926162010_init.sql`.
  - `up`: agregar `active boolean NOT NULL DEFAULT true`, y agregar
    `sections_not_blank` con `btrim(grade_level) <> '' AND
    btrim(section) <> ''`.
  - `down`: quitar la constraint y después la columna, en ese orden (una
    columna no se puede quitar mientras una constraint la use).
  - Hecho cuando: `dbmate up` la aplica sin error, `dbmate status` muestra
    `[X]` en las dos migraciones, un `INSERT` directo con `grade_level = ''`
    falla por el `CHECK`, y `dbmate rollback` seguido de `dbmate up` deja el
    esquema igual.

- [x] **T2. `domain.rs`: enums `Grado` y `Turno`, normalización y validación.**
  RF-2, RF-3, RF-4, RF-5, RF-6, RF-7, RF-14, RF-15, RF-16
  - `Grado` y `Turno` como enums con `TryFrom<&str>` y `as_str`. El catálogo
    de grados vive acá y en ningún otro lugar.
  - `normalizar_texto` (recorta extremos), `validar_no_vacio`,
    `validar_largo`, y `preparar_entrada` como punto único de validación en
    orden: recortar → no vacío → largo → catálogo.
  - `ErrorDominio` propio con `thiserror`, en español.
  - Tests unitarios en el propio archivo, sin base de datos.
  - Hecho cuando: `cargo test` pasa sin `DATABASE_URL`, con casos para
    `" 1RO "`→`1ro`, `"matutino"`→`MATUTINO`, `"  1ro  "`→`1ro`, `""` y
    `"   "` rechazados, `"quinto"` rechazado, `"NOCTURNO"` rechazado, y
    subdivisión más larga que el máximo rechazada.

- [x] **T3. `AppError::Conflict` y mapeo de códigos de Postgres.**
  RF-8, RF-17, RF-23, RF-27
  - Agregar la variante `Conflict(String)` a `AppError` con 409.
  - Traducir `23505` sobre `uq_sections_grade_section_shift` a
    "combinación ya en uso" y `23503` sobre `students_id_section_fkey` a
    "la sección tiene estudiantes".
  - Antes de nada, prueba de humo de `shared::audit::write_audit_log`: el
    helper bindea `Option<&str>` contra una columna `uuid NOT NULL` y nunca
    se ha ejecutado. Si falla, corregir la firma a `Uuid` **no-opcional** —
    la columna es `NOT NULL`, así que `Option` reintroduciría un NULL que la
    base rechaza en runtime, que es el mismo bug movido de lugar. De paso,
    `data: Option<&Value>` contra `jsonb NOT NULL` debe bindear `{}` cuando
    es `None`.
  - Sin `src/lib.rs`, `tests/` no puede llamar al código del binario: agregarlo
    y dejar `main.rs` como envoltorio, para que T4 y T7 puedan usarse desde
    tests de integración.
  - Hecho cuando: la prueba de humo de `write_audit_log` escribe y lee una
    fila de `audit_logs` dentro de la transacción descartable, los dos
    códigos se traducen a su mensaje en español, un `23505` sobre otra
    restricción **no** se convierte en 409, y `AppError::Conflict`
    responde 409.

- [ ] **T4. `queries.rs` y `create.sql`, con el alta y sus duplicados.**
  RF-1, RF-2, RF-3, RF-4, RF-5, RF-6, RF-7, RF-8, RF-27
  - `crear` como función genérica sobre `PgExecutor`, para que el test le
    pueda pasar `&mut tx`.
  - `INSERT` con `RETURNING` de las siete columnas.
  - Escritura de auditoría en la misma transacción.
  - Hecho cuando: el alta persiste la fila ya normalizada y devuelve
    identificador; el alta duplicada de una sección activa falla; el alta
    duplicada de una sección **dada de baja** también falla y no libera la
    combinación; queda la entrada de auditoría del alta.

- [ ] **T5. `update.sql` y `set_active.sql`, con edición, baja y reactivación.**
  RF-14, RF-15, RF-16, RF-17, RF-18, RF-19, RF-20, RF-21, RF-25, RF-26, RF-27
  - `update.sql` con `WHERE id = $1 RETURNING`.
  - `set_active.sql` con `active IS DISTINCT FROM $2` para que la operación
    sea realmente idempotente. Si no devuelve filas, `get_by_id.sql` para
    distinguir "no existe" de "ya estaba así".
  - Hecho cuando: editar normaliza igual que el alta; editar a vacío, a grado
    o turno inválido, o a una combinación ajena se rechaza; baja y reactivar
    dos veces devuelven éxito **sin que cambie `updated_at`**; editar y dar
    de baja una sección con estudiantes los deja en ella.

- [ ] **T6. `delete.sql`, con la eliminación física limitada.**
  RF-22, RF-23, RF-27
  - `DELETE ... WHERE id = $1 RETURNING`, sin filtro por `active`.
  - La auditoría se escribe en la misma transacción, antes del borrado, para
    conservar los valores de la fila que desaparece.
  - Hecho cuando: eliminar una sección sin estudiantes la borra de verdad y
    deja auditoría; eliminar una sección con un estudiante **activo** falla
    con el mensaje de que tiene estudiantes y la fila sigue ahí; eliminar con
    un estudiante **inactivo** también falla; una sección dada de baja que
    nunca tuvo estudiantes sí se elimina.

- [ ] **T7. `get_by_id.sql` y `list_active.sql`, con las lecturas.**
  RF-9, RF-10, RF-11, RF-12, RF-13, RF-24, RNF-3
  - `list_active.sql` con filtro opcional por grado y el orden por
    `array_position($2::text[], grade_level)`, con el array construido en
    Rust desde el enum `Grado`.
  - `COALESCE(..., 99)` para que un grado fuera del catálogo no rompa el
    orden estable que promete RNF-3.
  - Hecho cuando: el listado trae todas las activas en una sola llamada y
    omite las inactivas; ordena `1ro`→`5to` y dentro del grado `a`, `b`,
    `c`; el filtro por un grado devuelve solo ese grado; una sección dada de
    baja se consulta por identificador; un identificador inexistente no
    devuelve fila; dos listados seguidos devuelven el mismo orden.

- [ ] **T8. `handler.rs` y las rutas en `app.rs`.**
  RF-1 a RF-27 (bordes HTTP: códigos de estado y forma del JSON)
  - `tower` como dev-dependency para poder probar con `ServiceExt::oneshot`
    sin abrir un puerto.
  - Siete endpoints: alta, listado con filtro opcional por grado, consulta,
    edición, baja, reactivación y eliminación.
  - `DELETE` responde 204 sin cuerpo; el resto responde 200 o 201 con la
    sección.
  - Hecho cuando: los tests `oneshot` confirman 201 en el alta, 200 en
    lectura, 204 en eliminación, 400 en validación, 404 en inexistente y 409
    en duplicado y en sección con estudiantes; y el JSON trae `id`,
    `grade_level`, `section`, `shift`, `active`, `created_at`, `updated_at` en
    `snake_case`, con el grado en minúscula y el turno en mayúscula.

- [ ] **T9. `#![deny(unwrap_used, expect_used)]` y `clippy` limpio.**
  Todos
  - El `allow` explícito va solo en `main.rs` y `config/database.rs`, que
    son los dos únicos lugares donde la constitución nº 5 permite fallar
    rápido.
  - Hecho cuando: `cargo clippy --all-targets` no emite ni un warning, y los
    nueve warnings preexistentes de código no usado desaparecieron porque
    ya no hay código sin usar.

- [ ] **T10. Validación RF por RF, llamada real y cierre de la fase.**
  Todos
  - Recorrer los 27 RF de la spec anotando, para cada uno, qué test lo cubre
    y si pasa.
  - Verificar los bordes HTTP que los tests no cubran contra el servidor
    real: `cargo run` + `curl` sobre los siete endpoints.
  - Actualizar `MEMORY.md` y poner `Estado: implementada` en la spec.
  - Hecho cuando: los 27 RF tienen un veredicto registrado, los bordes HTTP
    están confirmados con `curl` y no solo con `oneshot`,
    `cargo test` y `cargo clippy` pasan limpios, y no queda ninguna casilla
    sin marcar.

---

## Notas de ejecución

- **T1 es bloqueante.** Sin la migración aplicada no compila ningún `.sql`
  que mencione `active`, porque `sqlx` los verifica contra el esquema real.
- **T2 no necesita base de datos.** Es la única tarea que se puede hacer
  sin `DATABASE_URL`, y conviene hacerla primero si el entorno está caído.
- **T3 tiene una condición escondida**: si la prueba de humo de
  `write_audit_log` falla, la corrección toca `shared/audit.rs`, que es
  código de la Fase 1. Es un cambio de tres líneas (la firma de
  `entity_id`), pero hay que decirlo al cerrar la tarea.
- **Los tests de T5 y T6 insertan en `students` con SQL directo**, porque el
  módulo `students` es la Fase 3 y todavía no existe. Todo dentro de la
  transacción descartable.
- **Ninguna tarea hace `commit`.** Si un test deja datos en `civik_dev`, es un
  error en el test, no en la base.