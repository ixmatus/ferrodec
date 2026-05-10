//! Kani harnesses for `Decimal32` comparison and ordering.

use super::{operand, NUM_OPERANDS};
use crate::decimal::Decimal32;

#[kani::proof]
fn partial_cmp_no_panic() {
    let ai: u8 = kani::any();
    let bi: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    kani::assume(bi < NUM_OPERANDS);

    let _ = operand(ai).partial_cmp(operand(bi));
}

#[kani::proof]
fn total_cmp_no_panic_and_total() {
    let ai: u8 = kani::any();
    let bi: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    kani::assume(bi < NUM_OPERANDS);

    let a = operand(ai);
    let b = operand(bi);

    // total_cmp must always return an Ordering (never panics, never
    // returns Option). Reflexivity check: a.total_cmp(a) == Equal.
    let _ = a.total_cmp(b);
    if ai == bi {
        assert!(a.total_cmp(b) == core::cmp::Ordering::Equal);
    }
}

#[kani::proof]
fn partial_cmp_nan_is_none() {
    let ai: u8 = kani::any();
    let bi: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    kani::assume(bi < NUM_OPERANDS);

    let a = operand(ai);
    let b = operand(bi);
    kani::assume(a.is_nan() || b.is_nan());

    let (cmp, _) = a.partial_cmp(b);
    assert!(cmp.is_none());
}

#[kani::proof]
fn min_max_no_panic() {
    let ai: u8 = kani::any();
    let bi: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    kani::assume(bi < NUM_OPERANDS);

    let _ = operand(ai).min(operand(bi));
    let _ = operand(ai).max(operand(bi));
}

#[kani::proof]
fn min_max_snan_raises_invalid() {
    let ai: u8 = kani::any();
    let bi: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    kani::assume(bi < NUM_OPERANDS);

    let a = operand(ai);
    let b = operand(bi);
    kani::assume(a.is_signaling_nan() || b.is_signaling_nan());

    let (_, s_min) = a.min(b);
    let (_, s_max) = a.max(b);
    assert!(s_min.invalid());
    assert!(s_max.invalid());
}
