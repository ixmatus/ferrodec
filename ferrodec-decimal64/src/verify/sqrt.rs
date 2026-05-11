//! Kani harnesses for `Decimal64::sqrt`.
//!
//! Routes every assertion through `sqrt_special_only_for_kani` per
//! ADR-0016. CBMC never encodes `sqrt_positive_finite`'s u64 isqrt +
//! rounding pipeline.

use super::{operand, NUM_OPERANDS};
use crate::decimal::Decimal64;

/// Whenever the operand is NOT a positive finite non-zero, the sqrt
/// special-case path resolves to `Some`. Equivalent to: positive
/// finite non-zero is the only class that falls through.
#[kani::proof]
fn sqrt_special_resolves_on_non_positive_finite() {
    let ai: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    let a = operand(ai);
    kani::assume(a.is_nan() || a.is_infinite() || a.is_zero() || a.is_sign_negative());
    assert!(a.sqrt_special_only_for_kani().is_some());
}

/// NaN propagates through `sqrt_special_only_for_kani`.
#[kani::proof]
fn sqrt_nan_propagates() {
    let ai: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    let a = operand(ai);
    kani::assume(a.is_nan());

    let (r, _) = a
        .sqrt_special_only_for_kani()
        .expect("NaN path resolved by special_cases");
    assert!(r.is_nan());
}

/// Signaling NaN raises `INVALID`.
#[kani::proof]
fn sqrt_snan_raises_invalid() {
    let a = Decimal64::SIGNALING_NAN;
    let (_, s) = a
        .sqrt_special_only_for_kani()
        .expect("sNaN path resolved by special_cases");
    assert!(s.invalid());
}

/// `sqrt(negative finite)` and `sqrt(−∞)` raise INVALID with NaN.
#[kani::proof]
fn sqrt_negative_finite_invalid() {
    let n: Decimal64 = Decimal64::MAX.neg();
    let (r, s) = n
        .sqrt_special_only_for_kani()
        .expect("negative finite resolved by special_cases");
    assert!(r.is_nan());
    assert!(s.invalid());

    let (r, s) = Decimal64::NEG_ONE
        .sqrt_special_only_for_kani()
        .expect("NEG_ONE resolved by special_cases");
    assert!(r.is_nan());
    assert!(s.invalid());

    let (r, s) = Decimal64::NEG_INFINITY
        .sqrt_special_only_for_kani()
        .expect("NEG_INFINITY resolved by special_cases");
    assert!(r.is_nan());
    assert!(s.invalid());
}

/// `sqrt(±0) = ±0`.
#[kani::proof]
fn sqrt_zero_preserves_sign() {
    let neg: bool = kani::any();
    let z = if neg {
        Decimal64::NEG_ZERO
    } else {
        Decimal64::ZERO
    };
    let (r, _) = z
        .sqrt_special_only_for_kani()
        .expect("±0 resolved by special_cases");
    assert!(r.is_zero());
    assert!(r.is_sign_negative() == neg);
}

/// `sqrt(+∞) = +∞`.
#[kani::proof]
fn sqrt_positive_infinity_pass_through() {
    let (r, s) = Decimal64::INFINITY
        .sqrt_special_only_for_kani()
        .expect("+∞ resolved by special_cases");
    assert!(r.is_infinite() && !r.is_sign_negative());
    assert!(!s.invalid());
}
