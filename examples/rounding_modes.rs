//! How the five IEEE 754 rounding modes treat a half-way value.
//!
//!     cargo run --example rounding_modes

use ferrodec::{Decimal128, RoundingMode};

fn main() {
    // 1.5 (= 15 × 10^-1), quantized to integer quantum 10^0.
    let x = Decimal128::try_new(15, -1).unwrap();
    let unit = Decimal128::ONE;

    println!("Quantizing 1.5 to integer quantum:");
    for (name, rm) in [
        ("NearestEven   ", RoundingMode::NearestEven),
        ("NearestAway   ", RoundingMode::NearestAway),
        ("TowardZero    ", RoundingMode::TowardZero),
        ("TowardPositive", RoundingMode::TowardPositive),
        ("TowardNegative", RoundingMode::TowardNegative),
    ] {
        let (r, _) = x.quantize(unit, rm);
        println!("  {name}: {r}");
    }
}
