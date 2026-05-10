//! Kani harnesses for `Decimal32::mul`.

use super::{operand, rm_from_u8, NUM_OPERANDS};
use ferrodec_ieee::RoundingMode;
use crate::decimal::Decimal32;

#[kani::proof]
fn mul_no_panic_special_inputs() {
    let ai: u8 = kani::any();
    let bi: u8 = kani::any();
    let rmi: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    kani::assume(bi < NUM_OPERANDS);
    kani::assume(rmi <= 4);

    let _ = operand(ai).mul(operand(bi), rm_from_u8(rmi));
}

#[kani::proof]
fn mul_nan_propagates() {
    let ai: u8 = kani::any();
    let bi: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    kani::assume(bi < NUM_OPERANDS);

    let a = operand(ai);
    let b = operand(bi);
    kani::assume(a.is_nan() || b.is_nan());

    let (r, _) = a.mul(b, RoundingMode::NearestEven);
    assert!(r.is_nan());
}

#[kani::proof]
fn mul_zero_times_infinity_invalid() {
    let zero_neg: bool = kani::any();
    let inf_neg: bool = kani::any();
    let a_is_zero: bool = kani::any();

    let zero = if zero_neg { Decimal32::NEG_ZERO } else { Decimal32::ZERO };
    let inf = if inf_neg { Decimal32::NEG_INFINITY } else { Decimal32::INFINITY };
    let (a, b) = if a_is_zero { (zero, inf) } else { (inf, zero) };

    let (r, s) = a.mul(b, RoundingMode::NearestEven);
    assert!(r.is_nan());
    assert!(s.invalid());
}

#[kani::proof]
fn mul_infinity_finite_xor_sign() {
    let inf_neg: bool = kani::any();
    let other_idx: u8 = kani::any();
    // Use ±1 / ±MAX (indices 6, 7, 8, 9) — non-zero finite so we
    // don't collide with the 0 × ∞ INVALID rule.
    kani::assume(other_idx == 6 || other_idx == 7 || other_idx == 8);
    let inf = if inf_neg { Decimal32::NEG_INFINITY } else { Decimal32::INFINITY };
    let other = operand(other_idx);

    let (r, s) = inf.mul(other, RoundingMode::NearestEven);
    assert!(r.is_infinite());
    assert!(r.is_sign_negative() == (inf_neg ^ other.is_sign_negative()));
    assert!(!s.invalid());
}
