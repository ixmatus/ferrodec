//! A quick tour of Decimal32 transcendental kernels: exp, ln, sin,
//! cos, sqrt. Demonstrates the Status flag handling pattern.
//!
//! Run with: `cargo run --example transcendentals --features transcendentals`

use ferrodec_decimal32::{Decimal32, RoundingMode};

fn parse(s: &str) -> Decimal32 {
    Decimal32::parse_str(s, RoundingMode::NearestEven)
        .unwrap()
        .0
}

fn main() {
    let rm = RoundingMode::NearestEven;

    let one = Decimal32::ONE;
    let pi = parse("3.141593");

    let (e, e_status) = one.exp(rm);
    println!("exp(1)   = {e:>10}   (inexact: {})", e_status.inexact());

    let (ln_e, _) = e.ln(rm);
    println!("ln(e)    = {ln_e:>10}   (round-trip back to ~1)");

    let (sin_pi_2, _) = parse("1.570796").sin(rm);
    println!("sin(π/2) = {sin_pi_2:>10}");

    let (cos_pi, _) = pi.cos(rm);
    println!("cos(π)   = {cos_pi:>10}");

    let (sqrt2, _) = parse("2").sqrt(rm);
    println!("sqrt(2)  = {sqrt2:>10}");

    // Domain errors emit INVALID via Status, leaving the result NaN.
    let (bad_acos, status) = parse("2").acos(rm);
    println!(
        "acos(2)  = {bad_acos:>10}   (status invalid: {})",
        status.invalid()
    );
}
