//! Kani harnesses for `Decimal32::sqrt`.

use super::{operand, rm_from_u8, NUM_OPERANDS};
use crate::decimal::Decimal32;
use ferrodec_ieee::RoundingMode;

#[kani::proof]
fn sqrt_no_panic_special_inputs() {
    let ai: u8 = kani::any();
    let rmi: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    kani::assume(rmi <= 4);

    let _ = operand(ai).sqrt(rm_from_u8(rmi));
}

#[kani::proof]
fn sqrt_nan_propagates() {
    let ai: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);

    let a = operand(ai);
    kani::assume(a.is_nan());

    let (r, _) = a.sqrt(RoundingMode::NearestEven);
    assert!(r.is_nan());
}

#[kani::proof]
fn sqrt_negative_finite_invalid() {
    // ±MAX has indices 8 (positive MAX), and we have only positive
    // MAX in the operand set. Build a negative finite by negating.
    let n: Decimal32 = Decimal32::MAX.neg();
    let (r, s) = n.sqrt(RoundingMode::NearestEven);
    assert!(r.is_nan());
    assert!(s.invalid());

    let (r, s) = Decimal32::NEG_ONE.sqrt(RoundingMode::NearestEven);
    assert!(r.is_nan());
    assert!(s.invalid());

    let (r, s) = Decimal32::NEG_INFINITY.sqrt(RoundingMode::NearestEven);
    assert!(r.is_nan());
    assert!(s.invalid());
}

#[kani::proof]
fn sqrt_zero_preserves_sign() {
    let neg: bool = kani::any();
    let z = if neg {
        Decimal32::NEG_ZERO
    } else {
        Decimal32::ZERO
    };
    let (r, _) = z.sqrt(RoundingMode::NearestEven);
    assert!(r.is_zero());
    assert!(r.is_sign_negative() == neg);
}

#[kani::proof]
fn sqrt_positive_infinity_pass_through() {
    let (r, s) = Decimal32::INFINITY.sqrt(RoundingMode::NearestEven);
    assert!(r.is_infinite() && !r.is_sign_negative());
    assert!(!s.invalid());
}
