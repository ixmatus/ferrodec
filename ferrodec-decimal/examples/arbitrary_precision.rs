//! Arbitrary-precision decimal arithmetic.
//!
//! Run with `cargo run -p ferrodec-decimal --example arbitrary_precision`.
//! Shows the explicit-context, per-operation-status API computing values that
//! no fixed-width format can hold exactly, plus a fixed-scale money example.

use ferrodec_decimal::{Context, Decimal, Rounding};

fn main() {
    // Sixty working digits, rounding half to even.
    let ctx = Context::new(
        core::num::NonZeroU32::new(60).unwrap(),
        1_000_000,
        -1_000_000,
        Rounding::HalfEven,
    );

    let one = Decimal::parse_str("1").unwrap();
    let three = Decimal::parse_str("3").unwrap();
    let (third, s1) = one.divide(&three, &ctx);
    println!("1 / 3        = {third}   (inexact: {})", s1.inexact());

    let two = Decimal::parse_str("2").unwrap();
    let (root2, s2) = two.sqrt(&ctx);
    println!("sqrt(2)      = {root2}   (inexact: {})", s2.inexact());

    // The square of the rounded root, back at the same precision.
    let (squared, _) = root2.multiply(&root2, &ctx);
    println!("sqrt(2)^2    = {squared}");

    // A fixed-scale money computation: a price times a quantity, quantized to
    // cents.
    let price = Decimal::parse_str("19.99").unwrap();
    let qty = Decimal::parse_str("3").unwrap();
    let (total, _) = price.multiply(&qty, &ctx);
    let cents = Decimal::parse_str("0.01").unwrap();
    let (rounded, _) = total.quantize(&cents, &ctx);
    println!("19.99 * 3    = {rounded}");
}
