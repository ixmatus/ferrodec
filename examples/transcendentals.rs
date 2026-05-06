//! Quick tour of the transcendental kernels.
//!
//!     cargo run --example transcendentals --features transcendentals

use ferrodec::{Decimal128, RoundingMode};

fn main() {
    let rm = RoundingMode::NearestEven;

    // exp then ln returns nearly the input (each kernel rounds at <= 1 ULP).
    let one = Decimal128::ONE;
    let (e, _) = one.exp(rm);
    let (back, _) = e.ln(rm);
    println!("e          = {e}");
    println!("ln(e)      = {back}");

    // sin(π/2) = 1, with Payne-Hanek argument reduction handling
    // the cancellation that drops 33+ digits of precision.
    let pi = ferrodec::pi();
    let two = Decimal128::try_new(2, 0).unwrap();
    let (half_pi, _) = pi.div(two, rm);
    let (s, _) = half_pi.sin(rm);
    println!("sin(π/2)   = {s}");
}
