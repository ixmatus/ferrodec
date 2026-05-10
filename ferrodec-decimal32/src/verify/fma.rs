//! Kani harnesses for `Decimal32::fma`.

use super::{operand, rm_from_u8, NUM_OPERANDS};
use ferrodec_ieee::RoundingMode;
use crate::decimal::Decimal32;

#[kani::proof]
fn fma_no_panic_special_inputs() {
    let ai: u8 = kani::any();
    let bi: u8 = kani::any();
    let ci: u8 = kani::any();
    let rmi: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    kani::assume(bi < NUM_OPERANDS);
    kani::assume(ci < NUM_OPERANDS);
    kani::assume(rmi <= 4);

    let _ = operand(ai).fma(operand(bi), operand(ci), rm_from_u8(rmi));
}

#[kani::proof]
fn fma_zero_times_infinity_invalid() {
    let zero_neg: bool = kani::any();
    let inf_neg: bool = kani::any();
    let a_is_zero: bool = kani::any();
    let ci: u8 = kani::any();
    kani::assume(ci < NUM_OPERANDS);
    // Skip sNaN c (which would dominate INVALID for an unrelated reason).
    let c = operand(ci);
    kani::assume(!c.is_signaling_nan());

    let zero = if zero_neg { Decimal32::NEG_ZERO } else { Decimal32::ZERO };
    let inf = if inf_neg { Decimal32::NEG_INFINITY } else { Decimal32::INFINITY };
    let (a, b) = if a_is_zero { (zero, inf) } else { (inf, zero) };

    let (r, s) = a.fma(b, c, RoundingMode::NearestEven);
    assert!(r.is_nan());
    assert!(s.invalid());
}

#[kani::proof]
fn fma_snan_anywhere_raises_invalid() {
    let ai: u8 = kani::any();
    let bi: u8 = kani::any();
    let ci: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    kani::assume(bi < NUM_OPERANDS);
    kani::assume(ci < NUM_OPERANDS);

    let a = operand(ai);
    let b = operand(bi);
    let c = operand(ci);
    kani::assume(a.is_signaling_nan() || b.is_signaling_nan() || c.is_signaling_nan());

    let (_, s) = a.fma(b, c, RoundingMode::NearestEven);
    assert!(s.invalid());
}
