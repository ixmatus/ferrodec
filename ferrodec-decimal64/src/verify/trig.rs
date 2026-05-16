//! Kani harnesses for `Decimal64::{sin, cos, tan, asin, acos, atan,
//! atan2}`.
//!
//! Routes every assertion through the `*_special_only_for_kani`
//! shims per ADR-0016. CBMC never encodes the `libm` + `from_f64`
//! finite pipeline; we prove no-panic and IEEE 754-2019 §9.2
//! special-case propagation only. The fall-through (`None`) class is
//! the f64 path and is intentionally out of scope for the proof.

use super::{operand, NUM_OPERANDS};
use crate::decimal::Decimal64;

/// For every non-finite-non-zero operand, `sin` resolves in the
/// special-case path (finite non-zero is the only fall-through).
#[kani::proof]
fn sin_special_resolves_on_non_finite() {
    let ai: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    let a = operand(ai);
    kani::assume(a.is_nan() || a.is_infinite() || a.is_zero());
    assert!(a.sin_special_only_for_kani().is_some());
}

/// `sin(NaN)` propagates a NaN; `sin(sNaN)` raises INVALID;
/// `sin(±∞) = NaN + INVALID`.
#[kani::proof]
fn sin_nan_and_infinity() {
    let ai: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    let a = operand(ai);
    kani::assume(a.is_nan());
    let (r, _) = a
        .sin_special_only_for_kani()
        .expect("NaN resolved by sin_special_cases");
    assert!(r.is_nan());

    let (_, s) = Decimal64::SIGNALING_NAN
        .sin_special_only_for_kani()
        .expect("sNaN resolved by sin_special_cases");
    assert!(s.invalid());

    let (r, s) = Decimal64::INFINITY
        .sin_special_only_for_kani()
        .expect("+∞ resolved by sin_special_cases");
    assert!(r.is_nan() && s.invalid());
}

/// `sin(±0) = ±0` (sign preserved).
#[kani::proof]
fn sin_zero_sign_preserving() {
    let neg: bool = kani::any();
    let z = if neg {
        Decimal64::NEG_ZERO
    } else {
        Decimal64::ZERO
    };
    let (r, _) = z
        .sin_special_only_for_kani()
        .expect("±0 resolved by sin_special_cases");
    assert!(r.is_zero() && r.is_sign_negative() == neg);
}

/// For every non-finite-non-zero operand, `cos` resolves in the
/// special-case path.
#[kani::proof]
fn cos_special_resolves_on_non_finite() {
    let ai: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    let a = operand(ai);
    kani::assume(a.is_nan() || a.is_infinite() || a.is_zero());
    assert!(a.cos_special_only_for_kani().is_some());
}

/// `cos(±0) = +1` (sign not preserved); `cos(±∞) = NaN + INVALID`.
#[kani::proof]
fn cos_zero_is_one_infinity_invalid() {
    let neg: bool = kani::any();
    let z = if neg {
        Decimal64::NEG_ZERO
    } else {
        Decimal64::ZERO
    };
    let (r, _) = z
        .cos_special_only_for_kani()
        .expect("±0 resolved by cos_special_cases");
    assert!(r.to_bits() == Decimal64::ONE.to_bits());

    let (r, s) = Decimal64::NEG_INFINITY
        .cos_special_only_for_kani()
        .expect("−∞ resolved by cos_special_cases");
    assert!(r.is_nan() && s.invalid());
}

/// For every non-finite-non-zero operand, `tan` resolves in the
/// special-case path; `tan(±0) = ±0` (sign preserved).
#[kani::proof]
fn tan_special_resolves_and_zero_sign_preserving() {
    let ai: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    let a = operand(ai);
    kani::assume(a.is_nan() || a.is_infinite() || a.is_zero());
    assert!(a.tan_special_only_for_kani().is_some());

    let neg: bool = kani::any();
    let z = if neg {
        Decimal64::NEG_ZERO
    } else {
        Decimal64::ZERO
    };
    let (r, _) = z
        .tan_special_only_for_kani()
        .expect("±0 resolved by tan_special_cases");
    assert!(r.is_zero() && r.is_sign_negative() == neg);
}

/// For every non-finite-non-zero operand, `asin` resolves in the
/// special-case path; `asin(±0) = ±0`, `asin(±∞) = NaN + INVALID`.
#[kani::proof]
fn asin_special_resolves_and_zero_infinity() {
    let ai: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    let a = operand(ai);
    kani::assume(a.is_nan() || a.is_infinite() || a.is_zero());
    assert!(a.asin_special_only_for_kani().is_some());

    let (r, _) = Decimal64::NEG_ZERO
        .asin_special_only_for_kani()
        .expect("−0 resolved by asin_special_cases");
    assert!(r.is_zero() && r.is_sign_negative());

    let (r, s) = Decimal64::INFINITY
        .asin_special_only_for_kani()
        .expect("+∞ resolved by asin_special_cases");
    assert!(r.is_nan() && s.invalid());
}

/// `asin` of a finite non-zero falls through (`None`): the `|x| > 1`
/// domain INVALID lives on the f64 path, not in the special cases.
#[kani::proof]
fn asin_finite_falls_through() {
    let pos: bool = kani::any();
    let a = if pos {
        Decimal64::ONE
    } else {
        Decimal64::NEG_ONE
    };
    assert!(a.asin_special_only_for_kani().is_none());
}

/// `acos` resolves only on NaN / ±∞; both `Zero` and finite non-zero
/// fall through to the f64 path (`None`). `acos(±∞) = NaN + INVALID`.
#[kani::proof]
fn acos_special_only_nan_and_infinity() {
    let ai: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    let a = operand(ai);
    kani::assume(a.is_nan() || a.is_infinite());
    assert!(a.acos_special_only_for_kani().is_some());

    let (r, s) = Decimal64::INFINITY
        .acos_special_only_for_kani()
        .expect("+∞ resolved by acos_special_cases");
    assert!(r.is_nan() && s.invalid());

    // Zero and finite non-zero are the f64-path fall-through.
    assert!(Decimal64::ZERO.acos_special_only_for_kani().is_none());
    assert!(Decimal64::NEG_ZERO.acos_special_only_for_kani().is_none());
    assert!(Decimal64::ONE.acos_special_only_for_kani().is_none());
}

/// `atan` resolves only on NaN / ±0; both `Infinity` and finite
/// non-zero fall through (`atan(±∞) = ±π/2` is computed by libm).
/// `atan(±0) = ±0` (sign preserved).
#[kani::proof]
fn atan_special_only_nan_and_zero() {
    let ai: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    let a = operand(ai);
    kani::assume(a.is_nan() || a.is_zero());
    assert!(a.atan_special_only_for_kani().is_some());

    let neg: bool = kani::any();
    let z = if neg {
        Decimal64::NEG_ZERO
    } else {
        Decimal64::ZERO
    };
    let (r, _) = z
        .atan_special_only_for_kani()
        .expect("±0 resolved by atan_special_cases");
    assert!(r.is_zero() && r.is_sign_negative() == neg);

    // ±∞ and finite non-zero are the libm fall-through.
    assert!(Decimal64::INFINITY.atan_special_only_for_kani().is_none());
    assert!(Decimal64::NEG_INFINITY
        .atan_special_only_for_kani()
        .is_none());
    assert!(Decimal64::ONE.atan_special_only_for_kani().is_none());
}

/// `atan2` resolves in the special-case path whenever either operand
/// is NaN; with neither NaN it falls through to the f64 path.
#[kani::proof]
fn atan2_special_resolves_iff_a_nan_operand() {
    let yi: u8 = kani::any();
    let xi: u8 = kani::any();
    kani::assume(yi < NUM_OPERANDS);
    kani::assume(xi < NUM_OPERANDS);
    let y = operand(yi);
    let x = operand(xi);
    let resolved = y.atan2_special_only_for_kani(x).is_some();
    assert!(resolved == (y.is_nan() || x.is_nan()));
}

/// IEEE 754-2019 §6.2.3 NaN ordering for binary `atan2`, pinned: the
/// first operand wins. A signaling NaN in `self` raises INVALID
/// regardless of `x`; a quiet NaN in `self` short circuits to OK
/// before `x` is examined, so a signaling NaN in `x` does not
/// upgrade the status in that case.
#[kani::proof]
fn atan2_nan_ordering_first_operand_wins() {
    let xi: u8 = kani::any();
    kani::assume(xi < NUM_OPERANDS);
    let x = operand(xi);

    // self = sNaN: INVALID no matter what x is.
    let (r, s) = Decimal64::SIGNALING_NAN
        .atan2_special_only_for_kani(x)
        .expect("sNaN self resolved by atan2_special_cases");
    assert!(r.is_nan() && s.invalid());

    // self = qNaN: result is NaN + OK even when x is a signaling NaN,
    // because the loop short circuits on self before reaching x.
    let (r, s) = Decimal64::NAN
        .atan2_special_only_for_kani(Decimal64::SIGNALING_NAN)
        .expect("qNaN self resolved by atan2_special_cases");
    assert!(r.is_nan() && !s.invalid());
}
