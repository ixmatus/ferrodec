//! Kani harnesses for `Decimal32::exp` and `Decimal32::ln`.
//!
//! Routes every assertion through `exp_special_only_for_kani` /
//! `ln_special_only_for_kani` per ADR-0016. CBMC never encodes the
//! `libm` + `from_f64` finite pipeline; we prove no-panic and
//! IEEE 754-2019 §9.2 special-case propagation only.

use super::{operand, NUM_OPERANDS};
use crate::decimal::Decimal32;

/// For every non-finite-non-zero operand, `exp` resolves in the
/// special-case path (finite non-zero is the only fall-through).
#[kani::proof]
fn exp_special_resolves_on_non_finite() {
    let ai: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    let a = operand(ai);
    kani::assume(a.is_nan() || a.is_infinite() || a.is_zero());
    assert!(a.exp_special_only_for_kani().is_some());
}

/// `exp(NaN)` propagates a NaN.
#[kani::proof]
fn exp_nan_propagates() {
    let ai: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    let a = operand(ai);
    kani::assume(a.is_nan());
    let (r, _) = a
        .exp_special_only_for_kani()
        .expect("NaN resolved by exp_special_cases");
    assert!(r.is_nan());
}

/// `exp(sNaN)` raises `INVALID`.
#[kani::proof]
fn exp_snan_raises_invalid() {
    let (_, s) = Decimal32::SIGNALING_NAN
        .exp_special_only_for_kani()
        .expect("sNaN resolved by exp_special_cases");
    assert!(s.invalid());
}

/// `exp(+∞) = +∞`, `exp(−∞) = +0`.
#[kani::proof]
fn exp_infinities() {
    let (r, s) = Decimal32::INFINITY
        .exp_special_only_for_kani()
        .expect("+∞ resolved by exp_special_cases");
    assert!(r.is_infinite() && !r.is_sign_negative());
    assert!(!s.invalid());

    let (r, _) = Decimal32::NEG_INFINITY
        .exp_special_only_for_kani()
        .expect("−∞ resolved by exp_special_cases");
    assert!(r.is_zero());
}

/// `exp(±0) = 1`.
#[kani::proof]
fn exp_zero_is_one() {
    let neg: bool = kani::any();
    let z = if neg {
        Decimal32::NEG_ZERO
    } else {
        Decimal32::ZERO
    };
    let (r, _) = z
        .exp_special_only_for_kani()
        .expect("±0 resolved by exp_special_cases");
    assert!(r.to_bits() == Decimal32::ONE.to_bits());
}

/// For every operand that is not a positive finite non-zero, `ln`
/// resolves in the special-case path.
#[kani::proof]
fn ln_special_resolves_on_non_positive_finite() {
    let ai: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    let a = operand(ai);
    kani::assume(a.is_nan() || a.is_infinite() || a.is_zero() || a.is_sign_negative());
    assert!(a.ln_special_only_for_kani().is_some());
}

/// `ln(NaN)` propagates; `ln(sNaN)` raises `INVALID`.
#[kani::proof]
fn ln_nan_propagates() {
    let ai: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    let a = operand(ai);
    kani::assume(a.is_nan());
    let (r, _) = a
        .ln_special_only_for_kani()
        .expect("NaN resolved by ln_special_cases");
    assert!(r.is_nan());

    let (_, s) = Decimal32::SIGNALING_NAN
        .ln_special_only_for_kani()
        .expect("sNaN resolved by ln_special_cases");
    assert!(s.invalid());
}

/// `ln(±0) = −∞ + DIV_BY_ZERO`.
#[kani::proof]
fn ln_zero_is_neg_infinity_div_by_zero() {
    let neg: bool = kani::any();
    let z = if neg {
        Decimal32::NEG_ZERO
    } else {
        Decimal32::ZERO
    };
    let (r, s) = z
        .ln_special_only_for_kani()
        .expect("±0 resolved by ln_special_cases");
    assert!(r.is_infinite() && r.is_sign_negative());
    assert!(s.div_by_zero());
}

/// `ln(negative finite)` and `ln(−∞)` raise INVALID with NaN.
#[kani::proof]
fn ln_negative_raises_invalid() {
    let (r, s) = Decimal32::NEG_ONE
        .ln_special_only_for_kani()
        .expect("negative finite resolved by ln_special_cases");
    assert!(r.is_nan() && s.invalid());

    let (r, s) = Decimal32::NEG_INFINITY
        .ln_special_only_for_kani()
        .expect("−∞ resolved by ln_special_cases");
    assert!(r.is_nan() && s.invalid());
}

/// `ln(+∞) = +∞`.
#[kani::proof]
fn ln_positive_infinity_pass_through() {
    let (r, s) = Decimal32::INFINITY
        .ln_special_only_for_kani()
        .expect("+∞ resolved by ln_special_cases");
    assert!(r.is_infinite() && !r.is_sign_negative());
    assert!(!s.invalid());
}
