//! Fuzz target: arbitrary `Decimal128` bit pattern through every
//! transcendental kernel.
//!
//! The kernels accept any input — finite, ±0, ±∞, qNaN, sNaN — so the
//! invariant we check here is the simplest one: the operation must
//! return a `(Decimal128, Status)` pair without panicking, regardless
//! of the input bit pattern. Faithful-rounding accuracy is checked by
//! the proptest oracle suite (`tests/property_*.rs`); this target
//! instead exercises the panic-freedom contract on the long tail of
//! pathological inputs that proptest doesn't sample.

#![no_main]

use libfuzzer_sys::fuzz_target;

use ferrodec::{Decimal128, RoundingMode};

#[derive(arbitrary::Arbitrary, Debug)]
struct Input {
    x: u128,
    y: u128,
}

fuzz_target!(|i: Input| {
    let x = Decimal128::from_bits(i.x);
    let y = Decimal128::from_bits(i.y);
    let rm = RoundingMode::NearestEven;

    // exp / log family.
    let _ = x.exp(rm);
    let _ = x.exp2(rm);
    let _ = x.ln(rm);
    let _ = x.log10(rm);
    let _ = x.log2(rm);
    let _ = x.cbrt(rm);
    let _ = x.sqrt(rm);

    // Trig.
    let _ = x.sin(rm);
    let _ = x.cos(rm);
    let _ = x.tan(rm);

    // Inverse trig.
    let _ = x.asin(rm);
    let _ = x.acos(rm);
    let _ = x.atan(rm);
    let _ = x.atan2(y, rm);

    // Hyperbolic + inverse.
    let _ = x.sinh(rm);
    let _ = x.cosh(rm);
    let _ = x.tanh(rm);
    let _ = x.asinh(rm);
    let _ = x.acosh(rm);
    let _ = x.atanh(rm);

    // Pow (two-operand, non-trivial special-case dispatch).
    let _ = x.pow(y, rm);
});
