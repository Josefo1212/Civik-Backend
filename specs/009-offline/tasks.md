# Tasks 009 — Offline

## T0
- [x] spec redactada y aprobada (`Estado: aprobada`)
- [x] plan/tasks

## T1
- [x] revisar attendance para soporte updated_at/conflictos
- [x] añadir tests base (idempotencia, corrección, HTTP)

## Pendiente (por especificar antes de codificar)
- [ ] T2: detección de conflicto por `updated_at` (comparación cliente/servidor)
- [ ] T3: tolerancia de reloj (ventana de aceptación, comparación con `NOW()`)
- [ ] T4: contrato idempotente de red (reintentos batch/patch sin duplicar)
- [ ] T5: veredicto de RF en esta spec y tests de integración
