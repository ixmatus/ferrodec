//! Kani harnesses for `roundToIntegral` (fd-hnx).
//!
//! Same strategy as the addsub / mul harnesses (ADR-0016): drive the
//! loop-free [`Decimal64::round_to_integral_special_only_for_kani`]
//! shim (which routes through `round_to_integral_special_cases`, the
//! factored-out non-finite/zero arms — no digit-drop loop) with
//! operands from the shared representative set, so CBMC never unrolls
//! the finite path. The finite path's correctness is carried by the
//! exact oracle in `tests/property_integral.rs` (round-to-integral is
//! exact) and the rounding-decision proofs.
//!
//! The special-class result does not depend on the rounding direction,
//! so the shim takes no `rm` and the harnesses do not range over it.

use super::{operand, NUM_OPERANDS};

/// The shim resolves (`Some`) exactly the non-finite and zero classes;
/// it returns `None` for every finite operand so CBMC never encodes
/// the digit-drop loop.
#[kani::proof]
fn integral_special_resolves_iff_non_finite_or_zero() {
    let i: u8 = kani::any();
    kani::assume(i < NUM_OPERANDS);
    let x = operand(i);
    let resolved = x.round_to_integral_special_only_for_kani().is_some();
    let special = x.is_nan() || x.is_infinite() || x.is_zero();
    assert_eq!(resolved, special);
}

/// A NaN operand propagates to a NaN result.
#[kani::proof]
fn integral_nan_propagates() {
    let i: u8 = kani::any();
    kani::assume(i < NUM_OPERANDS);
    let x = operand(i);
    kani::assume(x.is_nan());
    let (v, _) = x
        .round_to_integral_special_only_for_kani()
        .expect("NaN resolved by the special shim");
    assert!(v.is_nan());
}

/// A signaling NaN raises `INVALID` and quiets.
#[kani::proof]
fn integral_snan_raises_invalid() {
    let x = crate::decimal::Decimal64::SIGNALING_NAN;
    let (v, s) = x
        .round_to_integral_special_only_for_kani()
        .expect("sNaN resolved by the special shim");
    assert!(v.is_nan() && !v.is_signaling_nan());
    assert!(s.invalid());
}

/// An infinity passes through unchanged with no flag.
#[kani::proof]
fn integral_infinity_passthrough() {
    let i: u8 = kani::any();
    kani::assume(i < NUM_OPERANDS);
    let x = operand(i);
    kani::assume(x.is_infinite());
    let (v, s) = x
        .round_to_integral_special_only_for_kani()
        .expect("Infinity resolved by the special shim");
    assert!(v.is_infinite());
    assert!(v.is_sign_negative() == x.is_sign_negative());
    assert!(s.is_ok());
}

/// A zero yields a zero of the same sign, with no flag.
#[kani::proof]
fn integral_zero_is_zero() {
    let i: u8 = kani::any();
    kani::assume(i < NUM_OPERANDS);
    let x = operand(i);
    kani::assume(x.is_zero());
    let (v, s) = x
        .round_to_integral_special_only_for_kani()
        .expect("Zero resolved by the special shim");
    assert!(v.is_zero());
    assert!(v.is_sign_negative() == x.is_sign_negative());
    assert!(s.is_ok());
}
