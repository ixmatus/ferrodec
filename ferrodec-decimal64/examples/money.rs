//! Decimal64 in a ledger setting: parse a price, multiply by a tax
//! rate, quantize to cents, sum into a running total. 16-digit
//! precision comfortably handles billion-dollar totals to the cent
//! (10¹⁶ cents = ten quadrillion dollars). Reach for ferrodec
//! (Decimal128) only when you need >16 digits — for instance, a
//! single-currency aggregate above ten quadrillion units, or
//! cohort-preserving multi-currency arithmetic where intermediate
//! precision must be padded.
//!
//! Run with: `cargo run --example money --features fmt`

use ferrodec_decimal64::{Decimal64, RoundingMode};

fn parse(s: &str) -> Decimal64 {
    Decimal64::parse_str(s, RoundingMode::NearestEven)
        .unwrap()
        .0
}

fn main() {
    let line_items = ["12.99", "4.50", "1.25", "8.75"];
    let tax_rate = parse("0.0825"); // 8.25% sales tax
    let cent = Decimal64::try_new(1, -2).unwrap(); // 0.01 quantum

    let mut subtotal = Decimal64::ZERO;
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
