//! Decimal money math: multiply by a tax rate, quantize to cents, sum.
//!
//!     cargo run --example money

use ferrodec::{Decimal128, RoundingMode};

fn main() {
    let rm = RoundingMode::NearestEven;
    let cents = Decimal128::parse_str("0.01", rm).unwrap().0;

    let subtotal = Decimal128::parse_str("47.50", rm).unwrap().0;
    let tax_rate = Decimal128::parse_str("0.0875", rm).unwrap().0;

    let (tax_raw, _) = subtotal.mul(tax_rate, rm);
    let (tax, _) = tax_raw.quantize(cents, rm);
    let (total, _) = subtotal.add(tax, rm);

    println!("Subtotal:    {subtotal}");
    println!("Tax (8.75%): {tax}");
    println!("Total:       {total}");
}
