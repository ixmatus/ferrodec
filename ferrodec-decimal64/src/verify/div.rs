//! Kani harnesses for `Decimal64::div`.

use super::{operand, rm_from_u8, NUM_OPERANDS};
use crate::decimal::Decimal64;
use ferrodec_ieee::RoundingMode;

#[kani::proof]
fn div_no_panic_special_inputs() {
    let ai: u8 = kani::any();
    let bi: u8 = kani::any();
    let rmi: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    kani::assume(bi < NUM_OPERANDS);
    kani::assume(rmi <= 4);

    let _ = operand(ai).div(operand(bi), rm_from_u8(rmi));
}

#[kani::proof]
fn div_nan_propagates() {
    let ai: u8 = kani::any();
    let bi: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    kani::assume(bi < NUM_OPERANDS);

    let a = operand(ai);
    let b = operand(bi);
    kani::assume(a.is_nan() || b.is_nan());

    let (r, _) = a.div(b, RoundingMode::NearestEven);
    assert!(r.is_nan());
}

#[kani::proof]
fn div_finite_by_zero_raises_div_by_zero() {
    let dividend_idx: u8 = kani::any();
    // Choose a non-zero, non-NaN, non-Inf finite (±1, ±MAX, ±MIN_POSITIVE).
    kani::assume(dividend_idx >= 6 && dividend_idx < NUM_OPERANDS);
    let neg_zero_divisor: bool = kani::any();
    let dividend = operand(dividend_idx);
    let divisor = if neg_zero_divisor {
        Decimal64::NEG_ZERO
    } else {
        Decimal64::ZERO
    };

    let (r, s) = dividend.div(divisor, RoundingMode::NearestEven);
    assert!(r.is_infinite());
    assert!(r.is_sign_negative() == (dividend.is_sign_negative() ^ neg_zero_divisor));
    assert!(s.div_by_zero());
}

#[kani::proof]
fn div_zero_by_zero_invalid() {
    let za_neg: bool = kani::any();
    let zb_neg: bool = kani::any();
    let a = if za_neg {
        Decimal64::NEG_ZERO
    } else {
        Decimal64::ZERO
    };
    let b = if zb_neg {
        Decimal64::NEG_ZERO
    } else {
        Decimal64::ZERO
    };
    let (r, s) = a.div(b, RoundingMode::NearestEven);
    assert!(r.is_nan());
    assert!(s.invalid());
}

#[kani::proof]
fn div_infinity_by_infinity_invalid() {
    let sa: bool = kani::any();
    let sb: bool = kani::any();
    let a = if sa {
        Decimal64::NEG_INFINITY
    } else {
        Decimal64::INFINITY
    };
    let b = if sb {
        Decimal64::NEG_INFINITY
    } else {
        Decimal64::INFINITY
    };
    let (r, s) = a.div(b, RoundingMode::NearestEven);
    assert!(r.is_nan());
    assert!(s.invalid());
}
