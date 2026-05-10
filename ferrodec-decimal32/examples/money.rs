//! Decimal32 in a small-ledger setting: parse a price, multiply by a
//! tax rate, quantize to cents, sum into a running total. The 7-digit
//! precision of Decimal32 limits totals to roughly $99,999.99 — fine
//! for till-side telemetry and per-transaction reporting; reach for
//! Decimal64 / Decimal128 if your ledger crosses 8+ digits.
//!
//! Run with: `cargo run --example money --features fmt`

use ferrodec_decimal32::{Decimal32, RoundingMode};

fn parse(s: &str) -> Decimal32 {
    Decimal32::parse_str(s, RoundingMode::NearestEven).unwrap().0
}

fn main() {
    let line_items = ["12.99", "4.50", "1.25", "8.75"];
    let tax_rate = parse("0.0825"); // 8.25% sales tax
    let cent = Decimal32::try_new(1, -2).unwrap(); // 0.01 quantum

    let mut subtotal = Decimal32::ZERO;
    println!("Item       Price       Tax       Total");
    for item in line_items {
        let price = parse(item);
        let tax_unrounded = price.mul(tax_rate, RoundingMode::NearestEven).0;
        let (tax, _) = tax_unrounded.quantize(cent, RoundingMode::NearestEven);
        let total = price.add(tax, RoundingMode::NearestEven).0;
        subtotal = subtotal.add(total, RoundingMode::NearestEven).0;
        println!("           {price:>9}   {tax:>5}   {total:>5}");
    }
    let (subtotal_q, _) = subtotal.quantize(cent, RoundingMode::NearestEven);
    println!("Subtotal:                            {subtotal_q}");
}
