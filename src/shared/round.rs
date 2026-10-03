use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;

const ROUNDING_SCALE: u32 = 1;
const MULTIPLIER: i64 = 10;

/// Redondeo centralizado de notas (equivalente a Math.round(n*10)/10).
/// Redondea a 1 decimal usando redondeo estándar (bankers rounding) de rust_decimal.
/// `n` debe estar en base 20; el resultado conserva hasta 1 decimal.
pub fn round_grade(n: Decimal) -> Decimal {
    n.round_dp_with_strategy(ROUNDING_SCALE, rust_decimal::RoundingStrategy::MidpointNearestEven)
}

/// Redondea multiplicando por 10, redondeando a entero, dividiendo por 10.
/// Expresa explícitamente el patrón "Math.round(n*10)/10" mencionado en AGENTS.md.
pub fn round_to_one_decimal(n: Decimal) -> Decimal {
    let scaled = n * Decimal::new(MULTIPLIER, 0);
    let rounded = scaled.round_dp(0);
    rounded / Decimal::new(MULTIPLIER, 0)
}
