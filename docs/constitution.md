# Constitución — Civik Backend

Principios innegociables. Toda spec, plan y tarea debe cumplirlos.

1. **SQL verificado, no asumido**: cada consulta vive en su propio archivo
   `.sql` y se carga con `sqlx::query_file!`/`query_file_as!`, verificada
   contra el esquema real al compilar. Nunca un `.json` de queries, nunca
   un ORM.
2. **Hexagonal proporcional**: desacople formal (`trait` + adaptador) solo
   en `grades`. El resto separa negocio de SQL por organización de
   archivos, o no lo separa si es CRUD directo (ver tabla de `AGENTS.md`).
3. **Nada se borra físicamente**: toda eliminación es un estado
   (`CANCELLED`, `active = false`) con motivo. Hay dos excepciones, y se
   justifican por lo mismo: la fila no tiene historial que perder. (a) lapso
   vacío, no cerrado y sin terminar. (b) sección sin ningún estudiante que la
   referencie — lo garantiza la FK `RESTRICT` de `students.id_section`, así
   que no hace falta un trigger propio para esta tabla.
4. **Un hecho se protege por su evidencia, no por su fecha**: un evento
   "ocurrió" si tiene asistencia registrada, nunca por comparar con hoy.
5. **Fallar rápido, nunca en silencio**: `unwrap()`/`expect()` solo en el
   arranque (`config/database.rs`, `main.rs`). En todo lo demás, el error
   se propaga con `?` hacia un tipo propio.
6. **Testing sin base de prueba separada**: toda prueba corre en una
   transacción de `civik_dev` que termina en `rollback`. Nunca un `commit`
   en un test.
7. **Idioma**: dominio en español (`snake_case`), comentarios y errores en
   español, crates e imports en inglés.

Si una spec, un plan o una tarea contradice un principio de acá, la spec
está mal — no la constitución.
