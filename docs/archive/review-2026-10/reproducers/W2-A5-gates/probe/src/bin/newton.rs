//! Extended::recip / Extended::sqrt on operands outside the seed format's range.
use ferrodec::Decimal128;
use ferrodec_decimal32::Decimal32;
use ferrodec_transcend::extended::Extended;
use std::panic::catch_unwind;
fn show(label: &str, f: impl FnOnce() -> Extended + std::panic::UnwindSafe) {
    match catch_unwind(f) { Ok(v) => println!("{label}: {} digits, adj exp {}, sign={}, top={:?}", v.coef.decimal_digit_count(), v.exp + v.coef.decimal_digit_count() as i32 - 1, v.sign, v.coef), Err(_) => println!("{label}: PANICKED") }
}
fn main() {
    std::panic::set_hook(Box::new(|_| {}));
    show("recip<d128>(1E7000)  [true 1E-7000]", || Extended::parse_str("1e7000").recip::<Decimal128>());
    show("recip<d128>(1E-7000) [true 1E7000]", || Extended::parse_str("1e-7000").recip::<Decimal128>());
    show("sqrt<d128>(1E13000)  [true 1E6500]", || Extended::parse_str("1e13000").sqrt::<Decimal128>());
    show("sqrt<d128>(1E-13000) [true 1E-6500]", || Extended::parse_str("1e-13000").sqrt::<Decimal128>());
    show("recip<d32>(9.9999E96) [subnormal seed]", || Extended::parse_str("9.9999e96").recip::<Decimal32>());
    show("recip<d32>(9.9999E96) x 9.9999E96 [should be 1]", || Extended::parse_str("9.9999e96").recip::<Decimal32>().mul(Extended::parse_str("9.9999e96")));
}
