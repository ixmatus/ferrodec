//! Kani harnesses for `Decimal32::fma`.
//!
//! Routes every assertion through `fma_special_only_for_kani` per
//! ADR-0016. CBMC never encodes the u128 product / alignment / round
//! pipeline.

use super::{operand, NUM_OPERANDS};
use crate::decimal::Decimal32;
use ferrodec_ieee::RoundingMode;

/// `0 × ∞ + c` (any c, sign-NaN excluded for unrelated INVALID
/// dominance) raises `INVALID` with NaN result.
#[kani::proof]
fn fma_zero_times_infinity_invalid() {
    let zero_neg: bool = kani::any();
    let inf_neg: bool = kani::any();
    let a_is_zero: bool = kani::any();
    let ci: u8 = kani::any();
    kani::assume(ci < NUM_OPERANDS);
    let c = operand(ci);
    kani::assume(!c.is_signaling_nan());

    let zero = if zero_neg {
        Decimal32::NEG_ZERO
    } else {
        Decimal32::ZERO
    };
    let inf = if inf_neg {
        Decimal32::NEG_INFINITY
    } else {
        Decimal32::INFINITY
    };
    let (a, b) = if a_is_zero { (zero, inf) } else { (inf, zero) };

    let (r, s) = a
        .fma_special_only_for_kani(b, c, RoundingMode::NearestEven)
        .expect("0 × ∞ resolved by special_cases");
    assert!(r.is_nan());
    assert!(s.invalid());
}

/// Signaling NaN in any of (a, b, c) raises `INVALID`.
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

    let (_, s) = a
        .fma_special_only_for_kani(b, c, RoundingMode::NearestEven)
        .expect("sNaN path resolved by special_cases");
    assert!(s.invalid());
}

/// Whenever any operand is NaN, the shim returns `Some` with a NaN
/// result.
#[kani::proof]
fn fma_nan_propagates() {
    let ai: u8 = kani::any();
    let bi: u8 = kani::any();
    let ci: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    kani::assume(bi < NUM_OPERANDS);
    kani::assume(ci < NUM_OPERANDS);

    let a = operand(ai);
    let b = operand(bi);
    let c = operand(ci);
    kani::assume(a.is_nan() || b.is_nan() || c.is_nan());

    let (r, _) = a
        .fma_special_only_for_kani(b, c, RoundingMode::NearestEven)
        .expect("NaN path resolved by special_cases");
    assert!(r.is_nan());
}
