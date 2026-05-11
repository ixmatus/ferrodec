//! Kani harnesses for `Decimal64::mul`.
//!
//! Routes every assertion through `mul_special_only_for_kani` per
//! ADR-0016 so CBMC doesn't encode the u64 product / rounding
//! pipeline symbolically.

use super::{operand, NUM_OPERANDS};
use crate::decimal::Decimal64;

/// Whenever at least one operand is NaN or Infinity, the multiply
/// special-case path resolves to `Some`. (Zero × Finite falls
/// through to the finite path's zero-coefficient branch, the same
/// way decimal64's addsub dispatcher leaves `(Zero, Finite)` to the
/// finite path — pinning that case through the shim would require
/// the same dispatcher-extension refactor; tracked separately.)
#[kani::proof]
fn mul_special_resolves_on_nan_or_infinity() {
    let ai: u8 = kani::any();
    let bi: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    kani::assume(bi < NUM_OPERANDS);

    let a = operand(ai);
    let b = operand(bi);
    kani::assume(a.is_nan() || a.is_infinite() || b.is_nan() || b.is_infinite());

    assert!(a.mul_special_only_for_kani(b).is_some());
}

/// NaN propagates through `mul_special_only_for_kani`.
#[kani::proof]
fn mul_nan_propagates() {
    let ai: u8 = kani::any();
    let bi: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    kani::assume(bi < NUM_OPERANDS);

    let a = operand(ai);
    let b = operand(bi);
    kani::assume(a.is_nan() || b.is_nan());

    let (r, _) = a
        .mul_special_only_for_kani(b)
        .expect("NaN path resolved by special_cases");
    assert!(r.is_nan());
}

/// Signaling NaN raises `INVALID`.
#[kani::proof]
fn mul_snan_raises_invalid() {
    let ai: u8 = kani::any();
    let bi: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    kani::assume(bi < NUM_OPERANDS);

    let a = operand(ai);
    let b = operand(bi);
    kani::assume(a.is_signaling_nan() || b.is_signaling_nan());

    let (_, status) = a
        .mul_special_only_for_kani(b)
        .expect("sNaN path resolved by special_cases");
    assert!(status.invalid());
}

/// `0 × ∞` and `∞ × 0` (in either sign combination) are NaN with
/// `INVALID` raised.
#[kani::proof]
fn mul_zero_times_infinity_invalid() {
    let zero_neg: bool = kani::any();
    let inf_neg: bool = kani::any();
    let a_is_zero: bool = kani::any();

    let zero = if zero_neg {
        Decimal64::NEG_ZERO
    } else {
        Decimal64::ZERO
    };
    let inf = if inf_neg {
        Decimal64::NEG_INFINITY
    } else {
        Decimal64::INFINITY
    };
    let (a, b) = if a_is_zero { (zero, inf) } else { (inf, zero) };

    let (r, s) = a
        .mul_special_only_for_kani(b)
        .expect("0 × ∞ resolved by special_cases");
    assert!(r.is_nan());
    assert!(s.invalid());
}

/// `(±∞) × (±finite-non-zero)` produces `±∞` with sign `XOR`.
#[kani::proof]
fn mul_infinity_finite_xor_sign() {
    let inf_neg: bool = kani::any();
    let other_idx: u8 = kani::any();
    // ±1 / ±MAX (indices 6, 7, 8) — non-zero finite so we don't
    // collide with the 0 × ∞ INVALID rule.
    kani::assume(other_idx == 6 || other_idx == 7 || other_idx == 8);
    let inf = if inf_neg {
        Decimal64::NEG_INFINITY
    } else {
        Decimal64::INFINITY
    };
    let other = operand(other_idx);

    let (r, s) = inf
        .mul_special_only_for_kani(other)
        .expect("∞ × finite resolved by special_cases");
    assert!(r.is_infinite());
    assert!(r.is_sign_negative() == (inf_neg ^ other.is_sign_negative()));
    assert!(!s.invalid());
}
