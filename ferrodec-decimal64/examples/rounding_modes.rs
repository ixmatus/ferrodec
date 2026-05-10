//! How Decimal64 rounds 0.5 boundary cases under each of the five
//! IEEE 754 rounding modes.
//!
//! Run with: `cargo run --example rounding_modes --features fmt`

use ferrodec_decimal64::{Decimal64, RoundingMode};

fn parse(s: &str) -> Decimal64 {
    Decimal64::parse_str(s, RoundingMode::NearestEven).unwrap().0
}

fn main() {
    let cent = Decimal64::try_new(1, -2).unwrap();
    let modes = [
        ("NearestEven  (banker's)", RoundingMode::NearestEven),
        ("NearestAway  (away from 0)", RoundingMode::NearestAway),
        ("TowardZero   (truncate)", RoundingMode::TowardZero),
        ("TowardPositive (ceiling)", RoundingMode::TowardPositive),
        ("TowardNegative (floor)", RoundingMode::TowardNegative),
    ];

    let halfways = ["1.005", "1.015", "-1.005", "-1.015"];
    println!(
        "{:<28} {:>12} {:>12} {:>12} {:>12}",
        "rounding mode", halfways[0], halfways[1], halfways[2], halfways[3]
    );
    for (label, mode) in modes {
        let row: [Decimal64; 4] = [
            parse(halfways[0]).quantize(cent, mode).0,
            parse(halfways[1]).quantize(cent, mode).0,
            parse(halfways[2]).quantize(cent, mode).0,
            parse(halfways[3]).quantize(cent, mode).0,
        ];
        println!(
            "{label:<28} {:>12} {:>12} {:>12} {:>12}",
            row[0], row[1], row[2], row[3]
        );
    }
}
