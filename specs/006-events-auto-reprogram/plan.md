# Plan 006 — Events auto + reprogramación

## Decisiones de diseño

- **Rutas bajo `/lapses`**, no bajo `/events`: el sujeto de ambas operaciones
  es un lapso (su `practice_day` y su calendario), y ya existen
  `/lapses/{id}/activar|cerrar`. Colgar el generador y la reprogramación de
  `/lapses/{id}/...` es coherente con `app.rs`.
  - `POST /lapses/{id}/generar-practicas` → generar (200, lista de eventos)
  - `POST /lapses/{id}/reprogram-practice-day` body `{ "nuevo_dia" }` →
    reprogramar (200, resumen)
- **Generador manual + rango completo** (acordado): una práctica AUTO por cada
  `practice_day` de `start_date..end_date`. Idempotente: omite fechas que ya
  tienen una práctica regular (AUTO o MANUAL, cualquier estado). Es la
  decisión de `MEMORY.md` («cada día de práctica tiene como máximo una
  regular, la cree quien la cree»).
- **Reprogramar = cambiar `lapses.practice_day`** (acordado): actualiza el día
  y mueve los eventos pendientes (SCHEDULED + sin asistencia +
  PRACTICA_REGULAR AUTO|MANUAL) al próximo día de práctica estrictamente mayor
  que su fecha. No-regulares y congelados/cancelados no se tocan.
- **«Una regular por día» en la reprogramación**: si el destino ya tiene una
  regular, el evento se queda (se relega al count de `no_movidos`). Mantiene
  la misma decisión que el generador.
- **Orden de escritura en la tx de reprogramación**: primero `UPDATE
  lapses.set_practice_day`, después mover eventos, para que el trigger de la
  regla 1 (`check_event_date_within_lapse`, que dispara sobre `UPDATE OF
  event_date`) evalúe contra el **nuevo** `practice_day` del lapso.
- **Auditoría**: una entrada por operación (entity `"lapses"`, entity_id =
  lapso), dentro de la misma transacción. Es distinto del CRUD (una entrada
  por fila) a propósito: la generación y la reprogramación son operaciones
  masivas, y la entrada única con resumen es el registro fiel del "qué se
  hizo cuando se hizo".
  - `generar_practicas_auto` → `{ generadas, fechas, omitidas }`
  - `reprogramar_practice_day` → `{ dia_anterior, dia_nuevo, reprogramados:
    [{anterior, nuevo}], no_movidos }`
- **No-op idempotente**: reprogramar con `nuevo_dia == actual` responde 200
  sin tocar nada y **sin auditoría** (mismo espíritu que `set_active`
  idempotente de sections).
- **Mapeo de conflictos**: `uq_events_auto_one_per_day` (23505) se mapea a
  409 — hoy devolvería 500 (el unit test de `error.rs` lo deja explícito
  como no-mapeado). Se agrega el mapping y se ajusta el test.
- **Ningún segundo catálogo de días**: `events::domain` reusa
  `lapses::domain::DiaSemana` (parse del texto y `as_str()`); solo agrega la
  contraparte calendárica `weekday_de(DiaSemana) -> chrono::Weekday`, que es
  la traducción del `week_day_of` de la base al dominio de Rust. Se eliminan
  los helpers del stub (`obtener_dia_semana(&str)`) que duplicaban.

## Funciones puras nuevas (`events::domain`)

| Función | Responsabilidad | RF |
|---|---|---|
| `puede_generar_auto(&EstadoLapso) -> bool` | no `CLOSED` (PLANNED y ACTIVE sí) | RF-14, RF-19 |
| `fechas_de_practica(inicio, fin, DiaSemana) -> Vec<NaiveDate>` | todos los días del rango que coinciden con `practice_day` | RF-14 |
| `proxima_fecha_practica(desde, fin, DiaSemana) -> Option<NaiveDate>` | primer día **después** de `desde` (≤ fin) con ese `weekday` | RF-16 |
| `puede_reprogramarse_evento(estado, source, type, tiene_asistencia) -> bool` | SCHEDULED + sin asistencia + AUTO\|MANUAL + PRACTICA_REGULAR | RF-16 |
| `weekday_de(DiaSemana) -> chrono::Weekday` | contraparte de `week_day_of` de la base | — |

## Queries nuevas (`events::queries` + `queries/*.sql`)

- `create_auto.sql` — `INSERT` con `type_event='PRACTICA_REGULAR'` y
  `source='AUTO'` **hardcodeados en el SQL**, status `SCHEDULED`; binds solo
  `(id_lapse, event_date, event_name)`. Sin binds de enum.
- `set_practice_day.sql` — `UPDATE lapses SET practice_day = $2::text::week_day
  WHERE id = $1 RETURNING id, practice_day::text, status::text`.
- `mover_fecha_evento.sql` — `UPDATE events SET event_date = $2 WHERE id = $1
  RETURNING ...` (forma `Event`). El trigger de la regla 1 valida el nuevo
  día; el índice `uq_events_auto_one_per_day` protege la colisión (defensa).
- Ya existen `obtener_lapso` (`LapseLite`) y `listar_por_lapso` (fase 5).

## Archivos que se crean/modifican

- `src/modules/events/domain.rs` — reglas puras + unit tests (corrige el stub)
- `src/modules/events/service.rs` — `generar_practicas_auto`,
  `reprogramar_practice_day`, `ResultadoReprogramacion`
- `src/modules/events/handler.rs` — `generar_auto`, `reprogramar`
- `src/modules/events/queries.rs` + 3 `.sql`
- `src/shared/error.rs` — mapeo `uq_events_auto_one_per_day` → 409
- `src/app.rs` — las 2 rutas nuevas
- `tests/events_auto.rs` (T5) y `tests/http_events.rs` (T6)

## Cambio de rutas respecto al borrador

El plan viejo las ponía bajo `/events` (`POST /events/generate-auto`, `POST
/events/reprogram`). Pasan a `/lapses/{id}/...` por el argumento de arriba;
la spec aprobada 2026-10-09 fija las rutas nuevas.

## Estrategia de tests

- **T5 `tests/events_auto.rs`** (tx/rollback): genera en practice_day
  (todas las fechas); omite fechas con regular AUTO o MANUAL (idempotencia);
  lapso `CLOSED` → 409; genera con práctica extra sobre practice_day (la
  regular sí se crea); reprograma y mueve pendientes al nuevo día;
  congelados (con asistencia) y `CANCELLED` no se mueven; `EXTRA`/
  `PRESENTACION` no se mueven; combinación de dos eventos que apuntan al
  mismo destino → uno queda y se reporta `no_movidos`; no-op con el mismo
  día; auditoría presente. Años de lapso exclusivos por test.
- **T6 `tests/http_events.rs`**: `POST /lapses/{id}/generar-practicas` → 200;
  `POST /lapses/{id}/reprogram-practice-day` → 200; día inválido → 400;
  lapso inexistente → 404; lapso `CLOSED` → 409.
- **`tests/conflictos_bd.rs`**: 23505 de `uq_events_auto_one_per_day` y 23514
  del trigger regla 1 como defensa (SQL directo).

## Mapa RF → tests

| RF | Test de integración (T5) | HTTP (T6) |
|---|---|---|
| RF-14 | genera todas las fechas de practice_day | 200 |
| RF-15 | omite regular AUTO o MANUAL; segunda llamada → 0 | — |
| RF-16 | reprograma y mueve; no toca congelados/cancelados/no-regulares | 200 |
| RF-17 | destino ocupado → no_movidos | — |
| RF-18 | sin destino en el lapso → no_movidos | — |
| RF-19 | CLOSED → 409 / día inválido → 400 / inexistente → 404 | 400, 404, 409 |
| RF-20 | auditoría en la misma tx | — |
| RNF-5 | `tests/conflictos_bd.rs` | — |