# Plan 009 — Offline

- Extender attendance: manejar `updated_at`, criterios de conflicto simples
  (el cliente envía el `updated_at` que conoce; el servidor compara y decide
  conflict/ok según una ventana de tolerancia de reloj)
- Service/handler: ajustes sólo si el contrato lo pide; no cambiar las rutas
  de la Fase 7 sin justificarlo aquí
- Tests de integración (patrón tx/rollback) + HTTP, con años escolares y
  secciones exclusivos de este archivo (ver aislamiento de tests en MEMORY)

## Pendiente
El detalle (reglas exactas de conflicto, ventana de tolerancia, formato del
payload idempotente) se especifica en `spec.md` antes de codificar.
