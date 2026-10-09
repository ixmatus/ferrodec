use ferrodec_transcend::DecimalFormat;
fn show<F: DecimalFormat + core::fmt::Display + std::panic::RefUnwindSafe>(name: &str, x: F) {
    let r = std::panic::catch_unwind(|| F::nan_from(x));
    match r { Ok(v) => println!("{name}: nan_from(1) = {v}"), Err(_) => println!("{name}: nan_from(1) panicked") }
}
fn main() {
    show("Decimal128", ferrodec::Decimal128::ONE);
    show("Decimal64", ferrodec_decimal64::Decimal64::ONE);
    show("Decimal32", ferrodec_decimal32::Decimal32::ONE);
}
