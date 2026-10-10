use rust_decimal::Decimal;


fn peso_regular() -> Decimal { Decimal::new(1, 0) }
fn peso_extra() -> Decimal { Decimal::new(5, 1) }
fn peso_presentacion() -> Decimal { Decimal::new(2, 0) }

#[derive(Debug, Clone)]
pub struct Conteos {
    pub ausentes_regular: i64,
    pub ausentes_extra: i64,
    pub ausentes_presentacion: i64,
    pub total_eventos_elegibles: i64,
}

#[derive(Debug, Clone)]
pub struct ResultadoNota {
    pub nota: Decimal,
}

pub fn compute_grade(conteos: Conteos) -> ResultadoNota {
    let mut descuento = Decimal::ZERO;
    descuento += Decimal::from(conteos.ausentes_regular) * peso_regular();
    descuento += Decimal::from(conteos.ausentes_extra) * peso_extra();
    descuento += Decimal::from(conteos.ausentes_presentacion) * peso_presentacion();
    let mut nota = Decimal::new(200, 1) - descuento;
    if nota < Decimal::ZERO {
        nota = Decimal::ZERO;
    }
    ResultadoNota { nota }
}
