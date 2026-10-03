---
name: testing-transaccional
description: Úsala al escribir cualquier test de integración de Civik, al decidir cómo probar un trigger o invariante de base de datos, o al dudar entre el patrón de transacción/rollback y sqlx::test.
---

# Testing transaccional — Civik

No hay `civik_test`. Todo test de integración corre contra `civik_dev`
dentro de una transacción que nunca se confirma.

## El patrón base
```rust
#[sqlx::test]
async fn nombre_del_test(pool: PgPool) -> sqlx::Result<()> {
    let mut tx = pool.begin().await?;

    // código bajo prueba, recibiendo &mut tx, nunca el pool directo
    crear_estudiante(&mut tx, datos).await?;
    let resultado = cancelar_evento(&mut tx, id_evento, "prueba").await;

    // asserts leyendo dentro de la misma transacción
    assert!(resultado.is_ok());

    tx.rollback().await?; // o simplemente se descarta `tx`: nunca hacer commit
    Ok(())
}
```

## Probar que una invariante bloquea algo
El intento fallido nunca sale de la transacción descartable — no hay
riesgo de dejar la base en un estado roto:
```rust
let mut tx = pool.begin().await?;
let resultado = sqlx::query(
    "DELETE FROM attendance WHERE id_event = $1 AND id_student = $2"
)
.bind(id_event)
.bind(id_student)
.execute(&mut *tx)
.await;

assert!(resultado.is_err()); // el trigger debe rechazarlo
tx.rollback().await?;
```

## Cuándo este patrón NO alcanza
- Probar concurrencia real entre dos conexiones (dos procesos leyendo o
  escribiendo lo mismo a la vez). Con un desarrollador y ≤300 estudiantes,
  este caso no existe en Civik.
- Probar un `TRUNCATE`: no dispara triggers de fila, así que el rollback
  no protege del todo frente a un test que trunque por error.

Si alguno de estos dos casos aparece de verdad, la alternativa es
`sqlx::test` con una base efímera por test (clonada desde una plantilla
migrada y destruida al terminar), no volver a crear una base de test
separada y persistente.

## Checklist antes de dar un test por bueno
- [ ] ¿Termina en `rollback` (o se descarta sin `commit`)?
- [ ] Si prueba una invariante que bloquea algo, ¿el assert es sobre el
      error, no sobre un side-effect que ya ocurrió?
- [ ] ¿No depende del orden de ejecución con otros tests? (cada
      transacción es independiente, pero conviene confirmarlo)
- [ ] ¿Cubre al menos un caso borde documentado en la skill
      `civik-dominio` (ver "Casos de test obligatorios" de la fórmula de
      notas, si aplica)?
