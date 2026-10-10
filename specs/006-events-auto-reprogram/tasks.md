# Tasks 006 — Auto + reprogramación

## T0. Spec/plan/tasks aprobados y decisiones acordadas
- [x] spec aprobada
- [x] plan completo
- [x] tasks listo
- [x] Generador: manual, rango completo, idempotente (2026-10-09)
- [x] Reprogramar: cambia `lapses.practice_day` y mueve pendientes (2026-10-09)
- [x] Rutas bajo `/lapses/{id}/...` (2026-10-09)

## T1. `domain.rs` — reglas puras (RF-14, RF-16, RF-19)
- [x] Eliminar helpers del stub que duplican catálogos (`obtener_dia_semana`,
      `DiaSemana` re-parseado, uso de estados inventados "OPEN"/"PLANEADO")
- [x] `puede_generar_auto(&EstadoLapso) -> bool` (no CLOSED)
- [x] `fechas_de_practica(inicio, fin, DiaSemana) -> Vec<NaiveDate>`
- [x] `proxima_fecha_practica(desde, fin, DiaSemana) -> Option<NaiveDate>`
      (estrictamente posterior a `desde`)
- [x] `puede_reprogramarse_evento(estado, source, type, tiene_asistencia)`
      (SCHEDULED + sin asistencia + AUTO|MANUAL + PRACTICA_REGULAR)
- [x] `weekday_de(DiaSemana) -> chrono::Weekday`
- [x] Unit tests (sin base de datos) para todas las funciones

Hecho cuando: `cargo test --lib` verde y clippy 0/0 (test-first, primero
comprobar que fallan).

## T2. Queries + SQL (RF-14, RF-16, RNF-5)
- [x] `queries/create_auto.sql` (binds `(id_lapse, event_date, event_name)`;
      `type_event`/`source` hardcodeados)
- [x] `queries/set_practice_day.sql` (UPDATE lapses, devuelve practice_day/status)
- [x] `queries/mover_fecha_evento.sql` (UPDATE events SET event_date RETURNING *)
- [x] `queries.rs`: `crear_auto` (3 params), `set_practice_day`,
      `mover_fecha_evento`
- [x] `shared/error.rs`: mapear `uq_events_auto_one_per_day` (23505) → 409 y
      ajustar el unit test que hoy lo declara no-mapeado

Hecho cuando: compila (sqlx verifica contra el esquema), clippy 0/0.

## T3. `service.rs` — orquestación (RF-14 a RF-20)
- [x] `generar_practicas_auto(pool, id_lapse) -> Vec<Event>`: tx → lapso (404)
      → estado no CLOSED (409) → parse `DiaSemana` → fechas → omitir días con
      regular existente → `crear_auto` por fecha → auditoría (1 entrada con
      generadas/fechas/omitidas) → commit. Respeta reglas 1 y 2.
- [x] `reprogramar_practice_day(pool, id_lapse, nuevo_dia) -> ResultadoReprogramacion`:
      parse día (400) → tx → lapso (404) → no CLOSED (409) → no-op si el día
      no cambia (sin auditoría) → `set_practice_day` → eventos → filtrar
      pendientes → destino `proxima_fecha_practica` y colisión (→
      `no_movidos`) → `mover_fecha_evento` → auditoría (1 entrada con
      dia_anterior/dia_nuevo/reprogramados/no_movidos) → commit.
- [x] Struct `ResultadoReprogramacion` serializable
      `{ dia_anterior, dia_nuevo, movidos, no_movidos }`

Hecho cuando: compila, clippy 0/0.

## T4. `handler.rs` + rutas (RF-14, RF-16, RF-19)
- [x] `handler::generar_auto` — `POST /lapses/{id}/generar-practicas` → 200
- [x] `handler::reprogramar` (body `{ nuevo_dia }`) —
      `POST /lapses/{id}/reprogram-practice-day` → 200
- [x] `app.rs`: montar las 2 rutas

Hecho cuando: compila, clippy 0/0, rutas responden.

## T5. `tests/events_auto.rs` — integración tx/rollback (RF-14 a RF-18, RF-20)
- [x] genera en practice_day: una AUTO por fecha del rango
- [x] idempotente: segunda llamada → 0 creadas
- [x] omite fecha que ya tiene regular MANUAL (y la cancela: cuenta como
      ocupada igual)
- [x] práctica EXTRA/PRESENTACION sobre practice_day no bloquea la regular
- [x] lapso CLOSED → 409 (service devuelve error)
- [x] reprograma: cambia practice_day y mueve pendientes al nuevo día
- [x] con asistencia (congelado) no se mueve
- [x] CANCELLED y EXTRA/PRESENTACION no se mueven
- [x] dos pendientes al mismo destino → uno queda, `no_movidos >= 1`
- [x] no-op con el mismo día → sin entrada de auditoría
- [x] auditoría presente (1 entrada por operación)

Hecho cuando: tests verdes, clippy 0/0.

## T6. `tests/http_events.rs` — oneshot (RF-19)
- [x] generar-practicas → 200
- [x] reprogram → 200
- [x] día inválido → 400
- [x] lapso inexistente → 404
- [x] lapso CLOSED → 409

Hecho cuando: tests verdes, clippy 0/0.

## T7. Validación y cierre
- [x] Tabla RF→test en `tasks.md` (veredicto)
- [x] `cargo test` y `cargo clippy --all-targets -D warnings` 0/0
- [x] spec 006 → `Estado: implementada`
- [x] `MEMORY.md` actualizado (Fase 6 cerrada, siguiente Fase 7 `attendance`)
- [x] Commit de cierre

Hecho cuando: todo verde, docs actualizadas, commit hecho.

## Veredicto RF → test

| RF | Test(s) |
|---|---|
| RF-14 | `generar_crea_una_auto_por_dia_de_practica`, `generar_practicas_responde_200_con_lista` (HTTP) |
| RF-15 | `generar_es_idempotente_segunda_llamada_no_crea`, `generar_omite_fecha_con_regular_manual_o_cancelada` |
| RF-16 | `reprogramar_mueve_pendientes_y_actualiza_practice_day`, `reprogramar_practice_day_responde_200` (HTTP) |
| RF-17 | `reprogramar_respeta_destino_ocupado` |
| RF-18 | `reprogramar_sin_cupo_posterior_suma_no_movidos` |
| RF-19 | `generar_en_lapso_cerrado_responde_conflict`, `reprogramar_dia_invalido_responde_400`, `generar_y_reprogramar_lapso_inexistente_responde_404`, `generar_y_reprogramar_lapso_cerrado_responde_409` (HTTP) |
| RF-20 | `contar_auditoria` en generar/reprogramar, `reprogramar_con_el_mismo_dia_es_noop_sin_auditoria` |
| Reglas 1, 2, 9 (defensa) | `dos_auto_el_mismo_dia_emiten_unicidad_23505` (conflictos BD), `reprogramar_no_mueve_evento_con_asistencia` |
| Idempotencia sin auditoría falsa | `reprogramar_con_el_mismo_dia_es_noop_sin_auditoria` |