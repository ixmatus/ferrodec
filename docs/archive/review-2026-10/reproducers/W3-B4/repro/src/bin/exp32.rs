use ferrodec_decimal::*;
fn main() {
    let nz = |p| NonZeroU32::new(p).unwrap();
    // Context admits emin = i32::MIN; result exponent below i32::MIN
    let c = Context::new(nz(9), i32::MAX, i32::MIN, Rounding::HalfEven);
    let a = Decimal::finite(false, DecBig::from_u32(1), i32::MIN);
    let b = Decimal::finite(false, DecBig::from_u32(1), -1);
    let (r, s) = a.multiply(&b, &c);
    println!("1E-2147483648 * 1E-1 (p9, emin i32::MIN) = {:?} {:?}", r.finite_parts(), s);
    let (r, s) = a.divide(&Decimal::from_u64(10), &c);
    println!("1E-2147483648 / 10 = {:?} {:?}", r.finite_parts(), s);
    // GDA-legal emin with a large precision
    let c2 = Context::new(nz(2_000_000_000), 999_999_999, -999_999_999, Rounding::HalfEven);
    let (r, s) = a.multiply(&b, &c2);
    println!("same, p=2e9 emin=-999999999 = {:?} {:?}", r.finite_parts(), s);
    let (r, s) = a.plus(&c);
    println!("plus = {:?} {:?}", r.finite_parts(), s);
}
