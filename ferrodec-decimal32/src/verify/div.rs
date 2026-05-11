//! Kani harnesses for `Decimal32::div`.
//!
//! Routes every assertion through `div_special_only_for_kani` per
//! ADR-0016. CBMC never encodes the u128 scaled-divide pipeline.

use super::{operand, NUM_OPERANDS};
use crate::decimal::Decimal32;

/// Whenever at least one operand is NaN, Infinity, or zero, the divide
/// special-case path resolves to `Some` — div has more dispatcher
/// coverage than add/mul because both `x / 0` (DIV_BY_ZERO) and `∞ /
/// y` (with infinity rules) live in the dispatcher.
#[kani::proof]
fn div_special_resolves_on_nan_infinity_or_zero_divisor() {
    let ai: u8 = kani::any();
    let bi: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    kani::assume(bi < NUM_OPERANDS);

    let a = operand(ai);
    let b = operand(bi);
    kani::assume(a.is_nan() || a.is_infinite() || b.is_nan() || b.is_infinite() || b.is_zero());

    assert!(a.div_special_only_for_kani(b).is_some());
}

/// NaN propagates through `div_special_only_for_kani`.
#[kani::proof]
fn div_nan_propagates() {
    let ai: u8 = kani::any();
    let bi: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    kani::assume(bi < NUM_OPERANDS);

    let a = operand(ai);
    let b = operand(bi);
    kani::assume(a.is_nan() || b.is_nan());

    let (r, _) = a
        .div_special_only_for_kani(b)
        .expect("NaN path resolved by special_cases");
    assert!(r.is_nan());
}

/// Signaling NaN raises `INVALID`.
#[kani::proof]
fn div_snan_raises_invalid() {
    let ai: u8 = kani::any();
    let bi: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    kani::assume(bi < NUM_OPERANDS);

    let a = operand(ai);
    let b = operand(bi);
    kani::assume(a.is_signaling_nan() || b.is_signaling_nan());

    let (_, status) = a
        .div_special_only_for_kani(b)
        .expect("sNaN path resolved by special_cases");
    assert!(status.invalid());
}

/// `finite_non_zero / ±0` raises `DIV_BY_ZERO`, result is `±∞` with
/// XOR signs.
#[kani::proof]
fn div_finite_by_zero_raises_div_by_zero() {
    let dividend_idx: u8 = kani::any();
    // ±1, ±MAX, ±MIN_POSITIVE (indices 6..NUM_OPERANDS).
    kani::assume(dividend_idx >= 6 && dividend_idx < NUM_OPERANDS);
    let neg_zero_divisor: bool = kani::any();
    let dividend = operand(dividend_idx);
    let divisor = if neg_zero_divisor {
        Decimal32::NEG_ZERO
    } else {
        Decimal32::ZERO
    };

    let (r, s) = dividend
        .div_special_only_for_kani(divisor)
        .expect("x / 0 resolved by special_cases");
    assert!(r.is_infinite());
    assert!(r.is_sign_negative() == (dividend.is_sign_negative() ^ neg_zero_divisor));
    assert!(s.div_by_zero());
}

/// `0 / 0` is NaN + INVALID.
#[kani::proof]
fn div_zero_by_zero_invalid() {
    let za_neg: bool = kani::any();
    let zb_neg: bool = kani::any();
    let a = if za_neg {
        Decimal32::NEG_ZERO
    } else {
        Decimal32::ZERO
    };
    let b = if zb_neg {
        Decimal32::NEG_ZERO
    } else {
        Decimal32::ZERO
    };

    let (r, s) = a
        .div_special_only_for_kani(b)
        .expect("0 / 0 resolved by special_cases");
    assert!(r.is_nan());
    assert!(s.invalid());
}

/// `∞ / ∞` is NaN + INVALID.
#[kani::proof]
fn div_infinity_by_infinity_invalid() {
    let sa: bool = kani::any();
    let sb: bool = kani::any();
    let a = if sa {
        Decimal32::NEG_INFINITY
    } else {
        Decimal32::INFINITY
    };
    let b = if sb {
        Decimal32::NEG_INFINITY
    } else {
        Decimal32::INFINITY
    };

    let (r, s) = a
        .div_special_only_for_kani(b)
        .expect("∞ / ∞ resolved by special_cases");
    assert!(r.is_nan());
    assert!(s.invalid());
}
