use civik_backend::modules::grades::domain::{compute_grade, Conteos};
use rust_decimal::Decimal;

#[test]
fn nota_sin_ausencias_es_20() {
    let c = Conteos { ausentes_regular: 0, ausentes_extra: 0, ausentes_presentacion: 0, total_eventos_elegibles: 5 };
    let r = compute_grade(c);
    assert_eq!(r.nota, Decimal::new(20,0));
}

#[test]
fn ausente_regular_resta_1() {
    let c = Conteos { ausentes_regular: 1, ausentes_extra: 0, ausentes_presentacion: 0, total_eventos_elegibles: 2 };
    let r = compute_grade(c);
    assert_eq!(r.nota, Decimal::new(19,0));
}

#[test]
fn nota_no_puede_ser_negativa() {
    let c = Conteos { ausentes_regular: 50, ausentes_extra: 0, ausentes_presentacion: 0, total_eventos_elegibles: 2 };
    let r = compute_grade(c);
    assert_eq!(r.nota, Decimal::ZERO);
}
