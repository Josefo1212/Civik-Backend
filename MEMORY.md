# MEMORY.md — Civik Backend

Memoria del proyecto entre sesiones. Máximo ~50 líneas: resume o elimina lo
que ya no aporte. Al convertirse en regla permanente, se mueve a
`AGENTS.md` o a la skill `civik-dominio`, no se queda acá.

## Estado actual

- Proyecto reiniciado desde cero (Node/TS → Rust + hexagonal proporcional).
  No existe todavía ningún `Cargo.toml` ni código de aplicación.
- El esquema de base de datos (`db/migrations/20260926162010_init.sql`) ya
  está corregido: sin `notes`/`school_term`, sin orquestación en PL/pgSQL.
  Las 10 reglas de dominio están como constraints/triggers.
- `AGENTS.md` y la skill `civik-dominio` ya documentan arquitectura, stack,
  entornos, testing y la fórmula de notas. Léelos antes de proponer nada.
- Fase actual del plan de construcción (ver tabla "Orden de construcción"
  en `AGENTS.md`): **Fase 0 — Bootstrap**, sin empezar.

## Decisiones (y por qué)

- Una sola base (`civik_dev`), sin `civik_test`: los tests usan transacción
  + rollback, nunca hacen `commit`.
- `DATABASE_URL` unificada (no variables sueltas): la necesita `sqlx` para
  verificar consultas en compilación, y evita que runtime y compile-time
  usen fuentes de verdad distintas.
- La orquestación de prácticas automáticas se mueve a `events::service`
  (Rust), no a PL/pgSQL: con `sqlx::Transaction` se logra la misma
  atomicidad sin mezclar negocio con invariantes de base de datos.

## Aprendizajes y errores a evitar

- (vacío por ahora — completar según aparezcan sorpresas de `sqlx`/`axum`)

## Próximos pasos

- Fase 0: `Cargo.toml`, `config/database.rs`, `main.rs`, `app.rs`,
  `GET /_health`, `shared/error.rs`.
- Antes de Fase 0: decidir si se adopta SDD (constitution + specs) para
  las fases siguientes, empezando por la Fase 2 (`sections`).

## Fase 1 (2026-10-03) - shared/audit.rs y shared/round.rs

- Recreada base de datos: eliminado civik_dev/civik_test, creado civik_dev y aplicado db/20260926162010_init.sql (7 tablas, 7 enums).
- Implementado shared/audit.rs con write_audit_log (PgExecutor) y write_audit_log_tx (PgTransaction) para insertar en audit_logs con entity, entity_id, action, data (jsonb).
- Implementado shared/round.rs con round_grade (redondeo a 1 decimal, bankers rounding) y round_to_one_decimal (patrón Math.round(n*10)/10). shared/mod.rs actualizado.
- Cargo check sin errores; advertencias solo por código no usado (esperado).
