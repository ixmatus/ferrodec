// C3 scratch probe: a transcend integration test against a real format.
use ferrodec_decimal32::Decimal32;
use ferrodec_ieee::RoundingMode;
#[test]
fn atan2_d32_through_the_crate() {
    let y = Decimal32::parse_str("6304256E86", RoundingMode::NearestEven).unwrap().0;
    let x = Decimal32::parse_str("9523351E90", RoundingMode::NearestEven).unwrap().0;
    let (r, _) = ferrodec_transcend::inverse_trig::atan2_kernel(y, x, RoundingMode::NearestEven);
    println!("atan2 = {r}");
    let (s, _) = ferrodec_transcend::hyperbolic::sinh_kernel(
        Decimal32::parse_str("224.01", RoundingMode::NearestEven).unwrap().0, RoundingMode::NearestEven);
    println!("sinh(224.01) = {s}");
}
