---
description: SDD · Genera el plan técnico de una spec aprobada
agent: plan
---
Lee docs/constitution.md, AGENTS.md y specs/$1/spec.md. Usa la skill sdd.
NO escribas código.

Genera specs/$1/plan.md con: archivos que se crean o modifican y su
responsabilidad, en qué nivel de hexagonalidad cae el módulo (CRUD directo
/ parcial / completo, según la tabla de AGENTS.md), funciones puras de
dominio necesarias, las queries .sql necesarias (una por archivo),
decisiones técnicas justificadas (con su alternativa descartada) y
estrategia de tests con el patrón de la skill testing-transaccional.

Todo debe respetar la constitución y cubrir todos los RF. Marca qué RF
cubre cada parte. Si la spec no está aprobada o tiene dudas abiertas, para
y avísame.
