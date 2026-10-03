---
description: SDD · Valida la spec RF por RF (cargo test + llamada real al endpoint)
agent: build
---
Recorre specs/$1/spec.md requisito por requisito. Para cada RF de dominio,
indica qué test lo cubre y el resultado de `cargo test`.

Para un RF de borde HTTP (status code, forma del JSON) que no quede
cubierto por un test de integración, verifícalo con una llamada real al
endpoint (`cargo run` + curl/httpie).

Si algún RF no está cubierto o falla, dilo claramente. NO arregles nada
todavía. Después comprueba los criterios de finalización y dame un
veredicto: ¿la spec está cumplida?
