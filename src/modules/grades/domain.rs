//! Dominio puro de `grades`: la fórmula de la nota, sin `async`, sin `sqlx`,
//! sin saber que Postgres existe (RNF-1). El repositorio (adaptador) le acerca
//! conteos agregados y overrides; acá se decide la nota.
//!
//! La nota es base 20 con un decimal; cada `AUSENTE` descuenta el peso de su
//! tipo de evento (`PRACTICA_REGULAR = 1`, `PRACTICA_EXTRA = 0.5`,
//! `PRESENTACION = 2`), con tope en 0. Un override por (estudiante, lapso)
//! prevalece sobre la fórmula y sobre la elegibilidad. Nunca `f64`: los pesos y
//! la nota son `rust_decimal::Decimal` (RNF-3).

use rust_decimal::Decimal;

use crate::shared::round::round_grade;

/// Peso de una práctica regular (base de descuento).
fn peso_regular() -> Decimal {
    Decimal::new(1, 0)
}

/// Peso de una práctica extra (media unidad).
fn peso_extra() -> Decimal {
    Decimal::new(5, 1)
}

/// Peso de una presentación (dos unidades).
fn peso_presentacion() -> Decimal {
    Decimal::new(2, 0)
}

/// La base de la nota: 20.
fn base() -> Decimal {
    Decimal::new(20, 0)
}

/// Devuelve el valor con una escala fija (RF-8). `rescale` muta en el lugar,
/// así que se envuelve para poder construir el `Nota::Valor` de una vez. El
/// valor ya viene redondeado al número de decimales pedido, así que fijar la
/// escala no cambia la magnitud.
fn con_escala(mut valor: Decimal, escala: u32) -> Decimal {
    valor.rescale(escala);
    valor
}

/// Los conteos que el repositorio agrega para un estudiante en un lapso
/// (RNF-4). `n_eventos_elegibles` es del **lapso**, no del estudiante: es la
/// señal de "hay algo que calificar".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conteos {
    pub ausentes_regular: i64,
    pub ausentes_extra: i64,
    pub ausentes_presentacion: i64,
    pub n_eventos_elegibles: i64,
}

/// La nota resuelta: un valor en base 20, o la ausencia de nota.
///
/// `SinNota` no es 0: es que no hay nada que calificar (N = 0 y sin override).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Nota {
    SinNota,
    Valor(Decimal),
}

/// Calcula la nota a partir de los conteos (RF-1, RF-2, RF-6, RF-7, RF-8).
///
/// Sin eventos elegibles no hay nota (`SinNota`), no un 20. Con al menos uno,
/// aplica `max(0, 20 − Σ(peso por cada AUSENTE))` y redondea a un decimal con
/// el redondeo centralizado. `JUSTIFICADO` y `PRESENTE` no llegan acá como
/// AUSENTE, así que no descuentan (RF-2).
pub fn compute_grade(conteos: &Conteos) -> Nota {
    if conteos.n_eventos_elegibles == 0 {
        return Nota::SinNota;
    }

    let descuento = Decimal::from(conteos.ausentes_regular) * peso_regular()
        + Decimal::from(conteos.ausentes_extra) * peso_extra()
        + Decimal::from(conteos.ausentes_presentacion) * peso_presentacion();

    let nota = (base() - descuento).max(Decimal::ZERO);
    // `rescale(1)` fija la representación a un decimal (RF-8): un 19 entero se
    // presenta como "19.0", no como "19".
    Nota::Valor(con_escala(round_grade(nota), 1))
}

/// Resuelve la nota del estudiante (RF-5): si hay override, prevalece sobre el
/// cálculo y sobre la elegibilidad y se devuelve **sin redondear** (el valor
/// exacto que fijó el docente, hasta dos decimales, RF-8); si no, se calcula.
pub fn resolver_nota(conteos: &Conteos, override_: Option<Decimal>) -> Nota {
    match override_ {
        // `rescale(2)` fija la representación a dos decimales (RF-8): un 0 se
        // presenta como "0.00", un 18.5 como "18.50".
        Some(valor) => Nota::Valor(con_escala(valor, 2)),
        None => compute_grade(conteos),
    }
}

// Los tests de unidad pueden usar `unwrap()`: el `deny` de `lib.rs` protege el
// código de producción, y en un assert un panic es el fallo esperado.
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn conteos(regular: i64, extra: i64, presentacion: i64, n: i64) -> Conteos {
        Conteos {
            ausentes_regular: regular,
            ausentes_extra: extra,
            ausentes_presentacion: presentacion,
            n_eventos_elegibles: n,
        }
    }

    fn valor(nota: Nota) -> Decimal {
        match nota {
            Nota::Valor(v) => v,
            Nota::SinNota => panic!("se esperaba una nota, llegó SinNota"),
        }
    }

    // --- RF-1, RF-2: la fórmula y los pesos ---

    #[test]
    fn sin_ausencias_la_nota_es_20() {
        assert_eq!(
            valor(compute_grade(&conteos(0, 0, 0, 5))),
            Decimal::new(20, 0)
        );
    }

    #[test]
    fn cada_tipo_de_evento_descuenta_su_peso() {
        assert_eq!(
            valor(compute_grade(&conteos(1, 0, 0, 1))),
            Decimal::new(19, 0)
        );
        assert_eq!(
            valor(compute_grade(&conteos(0, 1, 0, 1))),
            Decimal::new(195, 1)
        );
        assert_eq!(
            valor(compute_grade(&conteos(0, 0, 1, 1))),
            Decimal::new(18, 0)
        );
    }

    #[test]
    fn los_pesos_se_suman_entre_tipos() {
        // 1 + 0.5 + 2 = 3.5 → 16.5
        assert_eq!(
            valor(compute_grade(&conteos(1, 1, 1, 3))),
            Decimal::new(165, 1)
        );
    }

    #[test]
    fn solo_ausente_descuenta() {
        // JUSTIFICADO/PRESENTE no llegan como AUSENTE: el conteo es 0.
        assert_eq!(
            valor(compute_grade(&conteos(0, 0, 0, 3))),
            Decimal::new(20, 0)
        );
    }

    // --- RF-6: N = 0 es "sin nota", no 0 ni 20 ---

    #[test]
    fn sin_eventos_elegibles_no_hay_nota() {
        assert_eq!(compute_grade(&conteos(0, 0, 0, 0)), Nota::SinNota);
    }

    #[test]
    fn n_cero_es_sin_nota_aunque_haya_ausencias_registradas() {
        // No debería pasar (no hay eventos), pero N manda: sin eventos, sin nota.
        assert_eq!(compute_grade(&conteos(3, 0, 0, 0)), Nota::SinNota);
    }

    // --- RF-7: el tope en 0 ---

    #[test]
    fn el_descuento_se_topa_en_cero() {
        assert_eq!(valor(compute_grade(&conteos(50, 0, 0, 50))), Decimal::ZERO);
    }

    #[test]
    fn justo_en_el_limite_no_pasa_de_cero() {
        // 20 ausentes regulares → 0, no negativo.
        assert_eq!(valor(compute_grade(&conteos(20, 0, 0, 20))), Decimal::ZERO);
    }

    // --- RF-8: redondeo a un decimal ---

    #[test]
    fn la_nota_calculada_se_redondea_a_un_decimal() {
        // 20 − 0.5 = 19.5 conserva un decimal
        let nota = valor(compute_grade(&conteos(0, 1, 0, 1)));
        assert_eq!(nota.scale(), 1, "la nota calculada tiene un decimal");
    }

    // --- RF-5: el override prevalece sin redondear ---

    #[test]
    fn el_override_prevalece_sobre_el_calculo() {
        // La fórmula daría 19, pero el override manda.
        let nota = resolver_nota(&conteos(1, 0, 0, 1), Some(Decimal::new(1875, 2)));
        assert_eq!(valor(nota), Decimal::new(1875, 2));
    }

    #[test]
    fn el_override_prevalece_aunque_no_haya_eventos() {
        // N = 0 daría SinNota, pero el override manda (RF-5).
        let nota = resolver_nota(&conteos(0, 0, 0, 0), Some(Decimal::new(1500, 2)));
        assert_eq!(valor(nota), Decimal::new(1500, 2));
    }

    #[test]
    fn el_override_se_devuelve_con_sus_dos_decimales() {
        let nota = resolver_nota(&conteos(0, 0, 0, 1), Some(Decimal::new(1875, 2)));
        assert_eq!(valor(nota).scale(), 2, "el override no se redondea");
    }

    #[test]
    fn un_override_en_cero_es_una_nota_cero_no_sin_nota() {
        let nota = resolver_nota(&conteos(1, 0, 0, 1), Some(Decimal::ZERO));
        assert_eq!(nota, Nota::Valor(Decimal::ZERO));
    }

    #[test]
    fn sin_override_se_calcula_normalmente() {
        assert_eq!(
            resolver_nota(&conteos(1, 0, 0, 1), None),
            Nota::Valor(Decimal::new(19, 0))
        );
    }
}
