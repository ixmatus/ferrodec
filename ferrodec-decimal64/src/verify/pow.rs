//! Kani harnesses for `Decimal64::pow` and `Decimal64::cbrt`.
//!
//! Routes every assertion through `pow_special_only_for_kani` /
//! `cbrt_special_only_for_kani` per ADR-0016. CBMC never encodes the
//! negative-base integer test or the `libm` + `from_f64` finite
//! pipeline; we prove no-panic and IEEE 754-2019 §9.2 special-case
//! propagation only.

use super::{operand, NUM_OPERANDS};
use crate::decimal::Decimal64;

/// `pow` resolves in the special-case path exactly when the exponent
/// is zero, the base numerically equals one (only `ONE` in the
/// operand set), or either operand is NaN; otherwise it falls
/// through to the f64 path.
#[kani::proof]
fn pow_special_resolution_set() {
    let ai: u8 = kani::any();
    let bi: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    kani::assume(bi < NUM_OPERANDS);
    let a = operand(ai);
    let b = operand(bi);
    let resolved = a.pow_special_only_for_kani(b).is_some();
    let one_base = a.to_bits() == Decimal64::ONE.to_bits();
    assert!(resolved == (b.is_zero() || one_base || a.is_nan() || b.is_nan()));
}

/// `pow(x, 0) = 1` for every base, including `pow(NaN, 0) = 1`.
#[kani::proof]
fn pow_x_zero_is_one() {
    let ai: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    let a = operand(ai);
    let neg: bool = kani::any();
    let z = if neg {
        Decimal64::NEG_ZERO
    } else {
        Decimal64::ZERO
    };
    let (r, s) = a
        .pow_special_only_for_kani(z)
        .expect("zero exponent resolved by pow_special_cases");
    assert!(r.to_bits() == Decimal64::ONE.to_bits() && !s.invalid());
}

/// `pow(1, y) = 1` for any `y`; an `sNaN` exponent still raises
/// INVALID per §9.2, a quiet NaN or finite exponent does not.
#[kani::proof]
fn pow_one_base_is_one() {
    let (r, s) = Decimal64::ONE
        .pow_special_only_for_kani(Decimal64::SIGNALING_NAN)
        .expect("pow(1, sNaN) resolved by pow_special_cases");
    assert!(r.to_bits() == Decimal64::ONE.to_bits() && s.invalid());

    let (r, s) = Decimal64::ONE
        .pow_special_only_for_kani(Decimal64::NAN)
        .expect("pow(1, qNaN) resolved by pow_special_cases");
    assert!(r.to_bits() == Decimal64::ONE.to_bits() && !s.invalid());

    let (r, s) = Decimal64::ONE
        .pow_special_only_for_kani(Decimal64::MAX)
        .expect("pow(1, finite) resolved by pow_special_cases");
    assert!(r.to_bits() == Decimal64::ONE.to_bits() && !s.invalid());
}

/// NaN propagation order (§6.2.3): with a non-zero exponent and a
/// base that is not one, a signaling NaN in either operand raises
/// INVALID; a quiet NaN propagates with OK.
#[kani::proof]
fn pow_nan_propagation() {
    // sNaN base, finite non-one exponent → qNaN + INVALID.
    let (r, s) = Decimal64::SIGNALING_NAN
        .pow_special_only_for_kani(Decimal64::MAX)
        .expect("sNaN base resolved by pow_special_cases");
    assert!(r.is_nan() && s.invalid());

    // Finite non-one base, sNaN exponent → qNaN + INVALID.
    let (r, s) = Decimal64::MAX
        .pow_special_only_for_kani(Decimal64::SIGNALING_NAN)
        .expect("sNaN exponent resolved by pow_special_cases");
    assert!(r.is_nan() && s.invalid());

    // qNaN base, finite non-one exponent → qNaN + OK.
    let (r, s) = Decimal64::NAN
        .pow_special_only_for_kani(Decimal64::MAX)
        .expect("qNaN base resolved by pow_special_cases");
    assert!(r.is_nan() && !s.invalid());
}

/// A non-NaN, non-one base with a non-zero, non-NaN exponent falls
/// through (`None`): the negative-base integer test and the
/// `libm::pow` path are out of scope for the proof.
#[kani::proof]
fn pow_finite_falls_through() {
    // Positive finite base, positive finite exponent.
    assert!(Decimal64::MAX
        .pow_special_only_for_kani(Decimal64::MAX)
        .is_none());
    // Negative finite base: the non-integer-exponent INVALID is on
    // the f64 path, so this is also a fall-through.
    assert!(Decimal64::NEG_ONE
        .pow_special_only_for_kani(Decimal64::MAX)
        .is_none());
}

/// For every non-finite-non-zero operand, `cbrt` resolves in the
/// special-case path (finite non-zero is the only fall-through);
/// `cbrt(±∞) = ±∞`, `cbrt(±0) = ±0` (sign preserved).
#[kani::proof]
fn cbrt_special_resolves_and_signs() {
    let ai: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    let a = operand(ai);
    kani::assume(a.is_nan() || a.is_infinite() || a.is_zero());
    assert!(a.cbrt_special_only_for_kani().is_some());

    let neg: bool = kani::any();
    let (zi, ii) = if neg {
        (Decimal64::NEG_ZERO, Decimal64::NEG_INFINITY)
    } else {
        (Decimal64::ZERO, Decimal64::INFINITY)
    };
    let (rz, _) = zi
        .cbrt_special_only_for_kani()
        .expect("±0 resolved by cbrt_special_cases");
    assert!(rz.is_zero() && rz.is_sign_negative() == neg);
    let (ri, _) = ii
        .cbrt_special_only_for_kani()
        .expect("±∞ resolved by cbrt_special_cases");
    assert!(ri.is_infinite() && ri.is_sign_negative() == neg);
}

/// `cbrt(NaN)` propagates; `cbrt(sNaN)` raises INVALID.
#[kani::proof]
fn cbrt_nan_propagates() {
    let ai: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    let a = operand(ai);
    kani::assume(a.is_nan());
    let (r, _) = a
        .cbrt_special_only_for_kani()
        .expect("NaN resolved by cbrt_special_cases");
    assert!(r.is_nan());

    let (_, s) = Decimal64::SIGNALING_NAN
        .cbrt_special_only_for_kani()
        .expect("sNaN resolved by cbrt_special_cases");
    assert!(s.invalid());
}
