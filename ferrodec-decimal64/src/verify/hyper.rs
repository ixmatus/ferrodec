//! Kani harnesses for `Decimal64::{sinh, cosh, tanh, asinh, acosh,
//! atanh}`.
//!
//! Routes every assertion through the `*_special_only_for_kani`
//! shims per ADR-0016. CBMC never encodes the `libm` + `from_f64`
//! finite pipeline; we prove no-panic and IEEE 754-2019 §9.2
//! special-case propagation only. The fall-through (`None`) class is
//! the f64 path and is intentionally out of scope for the proof.

use super::{operand, NUM_OPERANDS};
use crate::decimal::Decimal64;

/// For every non-finite-non-zero operand, `sinh` resolves in the
/// special-case path (finite non-zero is the only fall-through);
/// `sinh(±∞) = ±∞`, `sinh(±0) = ±0` (sign preserved).
#[kani::proof]
fn sinh_special_resolves_and_signs() {
    let ai: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    let a = operand(ai);
    kani::assume(a.is_nan() || a.is_infinite() || a.is_zero());
    assert!(a.sinh_special_only_for_kani().is_some());

    let neg: bool = kani::any();
    let (zi, ii) = if neg {
        (Decimal64::NEG_ZERO, Decimal64::NEG_INFINITY)
    } else {
        (Decimal64::ZERO, Decimal64::INFINITY)
    };
    let (rz, _) = zi
        .sinh_special_only_for_kani()
        .expect("±0 resolved by sinh_special_cases");
    assert!(rz.is_zero() && rz.is_sign_negative() == neg);
    let (ri, _) = ii
        .sinh_special_only_for_kani()
        .expect("±∞ resolved by sinh_special_cases");
    assert!(ri.is_infinite() && ri.is_sign_negative() == neg);
}

/// `sinh(NaN)` propagates; `sinh(sNaN)` raises INVALID.
#[kani::proof]
fn sinh_nan_propagates() {
    let ai: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    let a = operand(ai);
    kani::assume(a.is_nan());
    let (r, _) = a
        .sinh_special_only_for_kani()
        .expect("NaN resolved by sinh_special_cases");
    assert!(r.is_nan());

    let (_, s) = Decimal64::SIGNALING_NAN
        .sinh_special_only_for_kani()
        .expect("sNaN resolved by sinh_special_cases");
    assert!(s.invalid());
}

/// For every non-finite-non-zero operand, `cosh` resolves;
/// `cosh(±∞) = +∞`, `cosh(±0) = +1` (even function).
#[kani::proof]
fn cosh_special_resolves_and_even() {
    let ai: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    let a = operand(ai);
    kani::assume(a.is_nan() || a.is_infinite() || a.is_zero());
    assert!(a.cosh_special_only_for_kani().is_some());

    let neg: bool = kani::any();
    let z = if neg {
        Decimal64::NEG_ZERO
    } else {
        Decimal64::ZERO
    };
    let (rz, _) = z
        .cosh_special_only_for_kani()
        .expect("±0 resolved by cosh_special_cases");
    assert!(rz.to_bits() == Decimal64::ONE.to_bits());

    let (ri, _) = Decimal64::NEG_INFINITY
        .cosh_special_only_for_kani()
        .expect("−∞ resolved by cosh_special_cases");
    assert!(ri.is_infinite() && !ri.is_sign_negative());
}

/// For every non-finite-non-zero operand, `tanh` resolves;
/// `tanh(±∞) = ±1`, `tanh(±0) = ±0` (sign preserved).
#[kani::proof]
fn tanh_special_resolves_and_signs() {
    let ai: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    let a = operand(ai);
    kani::assume(a.is_nan() || a.is_infinite() || a.is_zero());
    assert!(a.tanh_special_only_for_kani().is_some());

    let neg: bool = kani::any();
    let (zi, ii, one) = if neg {
        (
            Decimal64::NEG_ZERO,
            Decimal64::NEG_INFINITY,
            Decimal64::NEG_ONE,
        )
    } else {
        (Decimal64::ZERO, Decimal64::INFINITY, Decimal64::ONE)
    };
    let (rz, _) = zi
        .tanh_special_only_for_kani()
        .expect("±0 resolved by tanh_special_cases");
    assert!(rz.is_zero() && rz.is_sign_negative() == neg);
    let (ri, _) = ii
        .tanh_special_only_for_kani()
        .expect("±∞ resolved by tanh_special_cases");
    assert!(ri.to_bits() == one.to_bits());
}

/// For every non-finite-non-zero operand, `asinh` resolves;
/// `asinh(±∞) = ±∞`, `asinh(±0) = ±0` (sign preserved).
#[kani::proof]
fn asinh_special_resolves_and_signs() {
    let ai: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    let a = operand(ai);
    kani::assume(a.is_nan() || a.is_infinite() || a.is_zero());
    assert!(a.asinh_special_only_for_kani().is_some());

    let neg: bool = kani::any();
    let (zi, ii) = if neg {
        (Decimal64::NEG_ZERO, Decimal64::NEG_INFINITY)
    } else {
        (Decimal64::ZERO, Decimal64::INFINITY)
    };
    let (rz, _) = zi
        .asinh_special_only_for_kani()
        .expect("±0 resolved by asinh_special_cases");
    assert!(rz.is_zero() && rz.is_sign_negative() == neg);
    let (ri, _) = ii
        .asinh_special_only_for_kani()
        .expect("±∞ resolved by asinh_special_cases");
    assert!(ri.is_infinite() && ri.is_sign_negative() == neg);
}

/// `acosh` is defined on `[1, +∞)`: it resolves in the special-case
/// path for NaN, ±∞, ±0, and any negative finite, and falls through
/// (`None`) only for a positive finite non-zero. `acosh(+∞) = +∞`;
/// `acosh(−∞) = NaN + INVALID`; `acosh(±0)` and `acosh(negative) =
/// NaN + INVALID`.
#[kani::proof]
fn acosh_special_boundary() {
    let ai: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    let a = operand(ai);
    // Positive finite non-zero (ONE, MAX, MIN_POSITIVE) is the only
    // fall-through; everything else resolves.
    let pos_finite = !a.is_nan() && !a.is_infinite() && !a.is_zero() && !a.is_sign_negative();
    assert!(a.acosh_special_only_for_kani().is_some() == !pos_finite);

    let (rp, _) = Decimal64::INFINITY
        .acosh_special_only_for_kani()
        .expect("+∞ resolved by acosh_special_cases");
    assert!(rp.is_infinite() && !rp.is_sign_negative());

    let (rn, sn) = Decimal64::NEG_INFINITY
        .acosh_special_only_for_kani()
        .expect("−∞ resolved by acosh_special_cases");
    assert!(rn.is_nan() && sn.invalid());

    let (rz, sz) = Decimal64::ZERO
        .acosh_special_only_for_kani()
        .expect("0 resolved by acosh_special_cases");
    assert!(rz.is_nan() && sz.invalid());

    let (rneg, sneg) = Decimal64::NEG_ONE
        .acosh_special_only_for_kani()
        .expect("negative finite resolved by acosh_special_cases");
    assert!(rneg.is_nan() && sneg.invalid());
}

/// For every non-finite-non-zero operand, `atanh` resolves;
/// `atanh(±∞) = NaN + INVALID`, `atanh(±0) = ±0`. The `|x| == 1`
/// pole and `|x| > 1` domain checks live on the f64 path.
#[kani::proof]
fn atanh_special_resolves_and_signs() {
    let ai: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    let a = operand(ai);
    kani::assume(a.is_nan() || a.is_infinite() || a.is_zero());
    assert!(a.atanh_special_only_for_kani().is_some());

    let neg: bool = kani::any();
    let z = if neg {
        Decimal64::NEG_ZERO
    } else {
        Decimal64::ZERO
    };
    let (rz, _) = z
        .atanh_special_only_for_kani()
        .expect("±0 resolved by atanh_special_cases");
    assert!(rz.is_zero() && rz.is_sign_negative() == neg);

    let (ri, si) = Decimal64::INFINITY
        .atanh_special_only_for_kani()
        .expect("+∞ resolved by atanh_special_cases");
    assert!(ri.is_nan() && si.invalid());

    // A finite non-zero (here ±1, the pole) is the f64-path
    // fall-through, not a pure special.
    assert!(Decimal64::ONE.atanh_special_only_for_kani().is_none());
}
