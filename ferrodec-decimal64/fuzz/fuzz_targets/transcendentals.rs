//! Fuzz target: arbitrary `u64` bit patterns through the
//! transcendental kernels. The contract under test is panic-freedom
//! only; accuracy is covered by the unit tests.

#![no_main]

use libfuzzer_sys::fuzz_target;

use ferrodec_decimal64::{Decimal64, RoundingMode};

fuzz_target!(|bits: u64| {
    let x = Decimal64::from_bits(bits);
    let rm = RoundingMode::NearestEven;

    let _ = x.exp(rm);
    let _ = x.ln(rm);
    let _ = x.sin(rm);
    let _ = x.cos(rm);
    let _ = x.tan(rm);
    let _ = x.asin(rm);
    let _ = x.acos(rm);
    let _ = x.atan(rm);
    let _ = x.sinh(rm);
    let _ = x.cosh(rm);
    let _ = x.tanh(rm);
    let _ = x.asinh(rm);
    let _ = x.acosh(rm);
    let _ = x.atanh(rm);
    let _ = x.cbrt(rm);
    let _ = x.sqrt(rm);
});
