# Plan 010 — Endurecer tests (invariantes de base)

Spec: `specs/010-endurecer-tests/spec.md` · Estado: implementada al cierre.

## Nivel de hexagonalidad

No aplica: esta fase no toca código de aplicación. Es una fase de **tests**
que opera contra la base (SQL directo y helpers de query existentes) para
fijar las invariantes de `db/schema.sql`. No se crea ningún módulo nuevo.

## Archivos

- **`tests/constraints.rs` (nuevo).** Toda la matriz de invariantes que hoy no
  tiene caso: CHECK/UNIQUE/EXCLUDE/triggers de `lapses`, `events`,
  `attendance`, `students`/`sections`, `grade_overrides` y `audit_logs`, más
  `assert_lapse_is_open`, `event_occurred` y `week_day_of`. Ordenado por tabla,
  con los RF citados en cada test. Sin `commit` (patrón tx/rollback).
- **No se modifica** `tests/conflictos_bd.rs` ni los tests de módulo
  (`lapses.rs`, `sections.rs`, `students.rs`): ya cubren su parte y se
  referencian desde el comentario de cabecera del archivo nuevo.
- **`specs/010-endurecer-tests/{spec,plan,tasks}.md`**, `MEMORY.md`: estado y
  cierre.

## Estrategia de tests

- **Patrón**: `common::pool()` + `pool.begin()` → operaciones → asserts →
  `tx.rollback()`. Para el intento que debe fallar: `SAVEPOINT` /
  `ROLLBACK TO SAVEPOINT` (sqlx 0.8 no expone savepoints), como en
  `conflictos_bd.rs`. Postgres aborta la transacción al primer error.
- **Asertos**: `sqlx::Error::Database(base)`, `base.code()` (SQLSTATE) y
  `base.constraint()` (nombre) — nunca solo `is_err()`.
- **Sensibilidad**: cada caso negativo va junto a su positivo adyacente (borde
  de fecha, `lapse_number` 1/3, `value` 0/20, sección vacía, estudiante sin
  asistencia). Sin SQLSTATE inventado: cada aserto se ve contra Postgres real.
- **Aislamiento**: años escolares y secciones propios de este archivo
  (rango `4001+`, secciones `Z*`), limpieza acotada por ese rango; los tests de
  solo-lectura de funciones (`event_occurred`, `week_day_of`) no ensucian.
- **Helpers**: `montar_lapso`, `montar_evento`, `montar_estudiante`,
  `montar_override`, `intento(...)` (ejecuta y devuelve el error de DB tras un
  savepoint), reutilizando `modules::*::queries` donde exista y SQL directo
  donde la app validaría antes de llegar a la base.

## Decisiones técnicas

- **SQL directo donde el dominio bloquearía antes de Postgres** (p. ej. nombre
  en blanco, `source`/`type` incoherente, `lapse_number` fuera de rango): el
  objetivo es probar **la base**, no el handler; un camino HTTP no ejercitaría
  la restricción. Alternativa descartada: forzar valores por la API (no
  llegan).
- **No reescribir lo cubierto** (alternativa descartada: consolidar todo en
  `conflictos_bd.rs`): menos riesgo de romper verde existente; el archivo nuevo
  documenta en cabecera qué queda en los otros.
- **Rangos de año/sección propios** (alternativa descartada: reciclar años ya
  usados): evita colisiones con la suite paralela.

## Mapa RF → tarea

| Tarea | RF |
|---|---|
| T1 (andamiaje + lapso) | RF-1, RF-2 |
| T2 (eventos: CHECK) | RF-9, RF-10, RF-11, RF-12 |
| T3 (eventos: triggers/FK) | RF-13, RF-14, RF-16, RF-17, RF-18 |
| T4 (asistencia) | RF-23, RF-24 |
| T5 (estudiante/sección) | RF-26 |
| T6 (override de nota) | RF-30, RF-31, RF-32, RF-33 |
| T7 (auditoría y utilidades) | RF-34, RF-35, RF-36, RF-37 |
| T8 (cierre) | criterios de finalización |

Los RF ya cubiertos (RF-3 a RF-8, RF-15, RF-19 a RF-22, RF-25, RF-27 a RF-29)
no generan tarea: se referencian en la cabecera de `tests/constraints.rs`.
