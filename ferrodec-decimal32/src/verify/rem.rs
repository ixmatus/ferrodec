//! Kani harnesses for `Decimal32::rem_near` (the IEEE 754-2019 §5.3.1
//! nearest-even remainder).
//!
//! Routes every assertion through `rem_special_only_for_kani` per
//! ADR-0016. CBMC never encodes the u128 alignment / quotient
//! pipeline; we prove no-panic and IEEE 754 special-case propagation
//! only. Port of `ferrodec/src/verify/rem.rs` to the 32-bit format.

use super::{operand, NUM_OPERANDS};
use crate::decimal::Decimal32;

/// The special-case path resolves whenever either operand is NaN, the
/// dividend is infinite, the divisor is zero, or the divisor is
/// infinite. (A zero dividend against a finite non-zero divisor stays
/// on the general path: decimal32's `handle_specials` returns `None`
/// for it, unlike the decimal128 parent which resolves `±0 / finite`
/// in the special-case helper. The narrower set is the sibling's
/// actual dispatcher coverage.)
#[kani::proof]
fn rem_special_resolves() {
    let ai: u8 = kani::any();
    let bi: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    kani::assume(bi < NUM_OPERANDS);

    let a = operand(ai);
    let b = operand(bi);
    kani::assume(a.is_nan() || b.is_nan() || a.is_infinite() || b.is_zero() || b.is_infinite());

    assert!(a.rem_special_only_for_kani(b).is_some());
}

#[kani::proof]
fn rem_nan_propagates() {
    let ai: u8 = kani::any();
    let bi: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    kani::assume(bi < NUM_OPERANDS);
    let a = operand(ai);
    let b = operand(bi);
    kani::assume(a.is_nan() || b.is_nan());

    let (r, _) = a
        .rem_special_only_for_kani(b)
        .expect("NaN path special-cased");
    assert!(r.is_nan());
}

#[kani::proof]
fn rem_snan_raises_invalid() {
    let ai: u8 = kani::any();
    let bi: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    kani::assume(bi < NUM_OPERANDS);
    let a = operand(ai);
    let b = operand(bi);
    kani::assume(a.is_signaling_nan() || b.is_signaling_nan());
    let (_, s) = a
        .rem_special_only_for_kani(b)
        .expect("sNaN path special-cased");
    assert!(s.invalid());
}

/// `x / 0` is invalid — NaN + INVALID.
#[kani::proof]
fn rem_x_over_zero_invalid() {
    // ONE / NEG_ONE / MAX / MIN_POSITIVE — finite non-zero dividends.
    let xi: u8 = kani::any();
    kani::assume(xi >= 6 && xi < NUM_OPERANDS);
    let zb: bool = kani::any();
    let zero = if zb {
        Decimal32::NEG_ZERO
    } else {
        Decimal32::ZERO
    };
    let (r, s) = operand(xi)
        .rem_special_only_for_kani(zero)
        .expect("x/0 special-cased");
    assert!(r.is_nan());
    assert!(s.invalid());
}

/// `±∞ / y` is invalid — NaN + INVALID for any non-NaN y.
#[kani::proof]
fn rem_inf_over_y_invalid() {
    let sa: bool = kani::any();
    let inf = if sa {
        Decimal32::NEG_INFINITY
    } else {
        Decimal32::INFINITY
    };
    let bi: u8 = kani::any();
    kani::assume(bi >= 2 && bi < NUM_OPERANDS); // not NaN/sNaN
    let (r, s) = inf
        .rem_special_only_for_kani(operand(bi))
        .expect("∞/y special-cased");
    assert!(r.is_nan());
    assert!(s.invalid());
}

/// `x / ±∞` returns `x` exactly (with whatever sign and cohort `x` has).
#[kani::proof]
fn rem_x_over_inf_returns_x() {
    // ±0 / ±1 / MAX / MIN_POSITIVE — every finite-or-zero dividend.
    let xi: u8 = kani::any();
    kani::assume(xi == 4 || xi == 5 || xi == 6 || xi == 7 || xi == 8 || xi == 9);
    let sb: bool = kani::any();
    let inf = if sb {
        Decimal32::NEG_INFINITY
    } else {
        Decimal32::INFINITY
    };

    let x = operand(xi);
    let (r, s) = x.rem_special_only_for_kani(inf).expect("x/∞ special-cased");
    assert!(r.to_bits() == x.to_bits());
    assert!(!s.invalid());
}
