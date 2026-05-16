//! Kani harnesses for the quantum-manipulating family:
//! `Decimal32::{quantize, scaleb, logb, next_up, next_down}`.
//!
//! These operations are pure decimal (no `libm` / `f64` path).
//! Routes every assertion through the `*_special_only_for_kani`
//! shims per ADR-0016: the proof covers no-panic and IEEE 754-2019
//! §5.3 special-case propagation; the finite-finite rescale / ULP
//! arithmetic is the `None` fall-through and is out of scope (CBMC
//! cannot tractably encode the full digit-shift loops).

use super::{operand, NUM_OPERANDS};
use crate::decimal::Decimal32;

/// `quantize` resolves in the special-case path exactly when either
/// operand is NaN or either operand is infinite; it falls through
/// (`None`) only when both operands are finite or zero.
#[kani::proof]
fn quantize_special_resolution_set() {
    let ai: u8 = kani::any();
    let bi: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    kani::assume(bi < NUM_OPERANDS);
    let a = operand(ai);
    let b = operand(bi);
    let resolved = a.quantize_special_only_for_kani(b).is_some();
    assert!(resolved == (a.is_nan() || b.is_nan() || a.is_infinite() || b.is_infinite()));
}

/// `quantize` NaN ordering (§6.2.3, first operand wins) and the
/// infinity rule: `quantize(±∞, ±∞)` keeps the sign of `self`, any
/// other infinity pairing is `NaN + INVALID`.
#[kani::proof]
fn quantize_nan_and_infinity() {
    let bi: u8 = kani::any();
    kani::assume(bi < NUM_OPERANDS);
    let b = operand(bi);

    // sNaN self → INVALID regardless of target.
    let (r, s) = Decimal32::SIGNALING_NAN
        .quantize_special_only_for_kani(b)
        .expect("sNaN self resolved by quantize_special_cases");
    assert!(r.is_nan() && s.invalid());

    // ±∞ quantize ±∞ → ±∞ with the sign of self.
    let (r, s) = Decimal32::NEG_INFINITY
        .quantize_special_only_for_kani(Decimal32::INFINITY)
        .expect("∞ pair resolved by quantize_special_cases");
    assert!(r.is_infinite() && r.is_sign_negative() && !s.invalid());

    // self finite, target ∞ → NaN + INVALID.
    let (r, s) = Decimal32::ONE
        .quantize_special_only_for_kani(Decimal32::INFINITY)
        .expect("finite/∞ resolved by quantize_special_cases");
    assert!(r.is_nan() && s.invalid());
}

/// `scaleb` resolves on NaN / ±∞ only; `±∞` passes through with its
/// sign, sNaN raises INVALID, qNaN is OK. `Zero` / finite fall
/// through (the `|n|` envelope and `10^n` shift depend on `n`).
#[kani::proof]
fn scaleb_special_boundary() {
    let ai: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    let a = operand(ai);
    let resolved = a.scaleb_special_only_for_kani().is_some();
    assert!(resolved == (a.is_nan() || a.is_infinite()));

    let (r, s) = Decimal32::NEG_INFINITY
        .scaleb_special_only_for_kani()
        .expect("−∞ resolved by scaleb_special_cases");
    assert!(r.is_infinite() && r.is_sign_negative() && !s.invalid());

    let (_, s) = Decimal32::SIGNALING_NAN
        .scaleb_special_only_for_kani()
        .expect("sNaN resolved by scaleb_special_cases");
    assert!(s.invalid());
}

/// `logb` resolves on NaN / ±∞ / ±0; only finite non-zero falls
/// through. `logb(±0) = −∞ + DIV_BY_ZERO`, `logb(±∞) = +∞`,
/// `logb(sNaN)` raises INVALID.
#[kani::proof]
fn logb_special_boundary() {
    let ai: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    let a = operand(ai);
    let resolved = a.logb_special_only_for_kani().is_some();
    assert!(resolved == (a.is_nan() || a.is_infinite() || a.is_zero()));

    let neg: bool = kani::any();
    let z = if neg {
        Decimal32::NEG_ZERO
    } else {
        Decimal32::ZERO
    };
    let (r, s) = z
        .logb_special_only_for_kani()
        .expect("±0 resolved by logb_special_cases");
    assert!(r.is_infinite() && r.is_sign_negative() && s.div_by_zero());

    let (r, _) = Decimal32::NEG_INFINITY
        .logb_special_only_for_kani()
        .expect("−∞ resolved by logb_special_cases");
    assert!(r.is_infinite() && !r.is_sign_negative());

    let (_, s) = Decimal32::SIGNALING_NAN
        .logb_special_only_for_kani()
        .expect("sNaN resolved by logb_special_cases");
    assert!(s.invalid());
}

/// `next_up` resolves on NaN / ±0 / ±∞; only finite non-zero falls
/// through. `next_up(±0) = MIN_POSITIVE`, `next_up(+∞) = +∞`,
/// `next_up(−∞) = MIN`, sNaN is quieted + INVALID, qNaN passes
/// through with OK.
#[kani::proof]
fn next_up_special_boundary() {
    let ai: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    let a = operand(ai);
    let resolved = a.next_up_special_only_for_kani().is_some();
    assert!(resolved == (a.is_nan() || a.is_zero() || a.is_infinite()));

    let neg: bool = kani::any();
    let z = if neg {
        Decimal32::NEG_ZERO
    } else {
        Decimal32::ZERO
    };
    let (r, s) = z
        .next_up_special_only_for_kani()
        .expect("±0 resolved by next_up_special_cases");
    assert!(r.to_bits() == Decimal32::MIN_POSITIVE.to_bits() && !s.invalid());

    let (r, _) = Decimal32::NEG_INFINITY
        .next_up_special_only_for_kani()
        .expect("−∞ resolved by next_up_special_cases");
    assert!(r.to_bits() == Decimal32::MIN.to_bits());

    let (r, s) = Decimal32::INFINITY
        .next_up_special_only_for_kani()
        .expect("+∞ resolved by next_up_special_cases");
    assert!(r.is_infinite() && !r.is_sign_negative() && !s.invalid());

    let (r, s) = Decimal32::SIGNALING_NAN
        .next_up_special_only_for_kani()
        .expect("sNaN resolved by next_up_special_cases");
    assert!(r.is_nan() && !r.is_signaling_nan() && s.invalid());
}

/// `next_down` resolves on NaN / ±0 / ±∞; only finite non-zero falls
/// through. `next_down(sNaN)` is quieted + INVALID; `next_down(±0) =
/// −MIN_POSITIVE`; `next_down(−∞) = −∞`.
#[kani::proof]
fn next_down_special_boundary() {
    let ai: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    let a = operand(ai);
    let resolved = a.next_down_special_only_for_kani().is_some();
    assert!(resolved == (a.is_nan() || a.is_zero() || a.is_infinite()));

    let neg: bool = kani::any();
    let z = if neg {
        Decimal32::NEG_ZERO
    } else {
        Decimal32::ZERO
    };
    let (r, s) = z
        .next_down_special_only_for_kani()
        .expect("±0 resolved by next_down_special_cases");
    assert!(
        r.to_bits() == Decimal32::MIN_POSITIVE.neg().to_bits()
            && r.is_sign_negative()
            && !s.invalid()
    );

    let (r, _) = Decimal32::NEG_INFINITY
        .next_down_special_only_for_kani()
        .expect("−∞ resolved by next_down_special_cases");
    assert!(r.is_infinite() && r.is_sign_negative());

    let (r, s) = Decimal32::SIGNALING_NAN
        .next_down_special_only_for_kani()
        .expect("sNaN resolved by next_down_special_cases");
    assert!(r.is_nan() && !r.is_signaling_nan() && s.invalid());
}
