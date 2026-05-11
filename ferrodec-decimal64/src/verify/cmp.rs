//! Kani harnesses for `Decimal64` comparison and ordering.
//!
//! Comparison ops are loop-free and don't need the
//! `_special_only_for_kani` shim convention (ADR-0016 carves out
//! predicates explicitly).

use super::{operand, NUM_OPERANDS};
use crate::decimal::Decimal64;

/// `partial_cmp` never panics on any pair from the operand pool.
#[kani::proof]
fn partial_cmp_no_panic() {
    let ai: u8 = kani::any();
    let bi: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    kani::assume(bi < NUM_OPERANDS);

    let _ = operand(ai).partial_cmp(operand(bi));
}

/// `total_cmp` is total (returns `Ordering`, never panics) and
/// reflexive on identical pool entries.
///
/// Renamed from `total_cmp_no_panic_and_total` to match the actual
/// claim: the prior name implied a *totality* (every pair compares)
/// proof, but the body only asserts `Equal` for the diagonal. The
/// totality property is implied by `total_cmp` returning `Ordering`
/// (rather than `Option<Ordering>`); any reachable call site that
/// returns is *automatically* total. The reflexivity check is the
/// load-bearing assertion.
#[kani::proof]
fn total_cmp_reflexive() {
    let ai: u8 = kani::any();
    let bi: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    kani::assume(bi < NUM_OPERANDS);

    let a = operand(ai);
    let b = operand(bi);
    let _ = a.total_cmp(b);
    if ai == bi {
        assert!(a.total_cmp(b) == core::cmp::Ordering::Equal);
    }
}

/// `partial_cmp` on any NaN pair returns `None`. (Quiet semantics
/// per IEEE 754 §5.11 / dec-spec compareQuiet*.)
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

/// `min` and `max` never panic on any pair from the operand pool.
#[kani::proof]
fn min_max_no_panic() {
    let ai: u8 = kani::any();
    let bi: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    kani::assume(bi < NUM_OPERANDS);

    let _ = operand(ai).min(operand(bi));
    let _ = operand(ai).max(operand(bi));
}

/// `min`/`max` with an sNaN operand raise INVALID and produce a
/// quieted NaN. (The Slice A fix at `src/cmp.rs:149-151` for
/// decimal128 was mirrored to decimal64 in the same slice. This
/// harness pins the result class through the dispatcher.)
#[kani::proof]
fn min_max_snan_raises_invalid() {
    let ai: u8 = kani::any();
    let bi: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    kani::assume(bi < NUM_OPERANDS);

    let a = operand(ai);
    let b = operand(bi);
    kani::assume(a.is_signaling_nan() || b.is_signaling_nan());

    let (r_min, s_min) = a.min(b);
    let (r_max, s_max) = a.max(b);
    assert!(s_min.invalid());
    assert!(s_max.invalid());
    assert!(r_min.is_quiet_nan(), "sNaN must be quieted on output");
    assert!(r_max.is_quiet_nan(), "sNaN must be quieted on output");
}

/// Slice A regression guard: `min`/`max` preserve the sNaN's payload
/// (Decimal64 has a 50-bit payload field). Symbolic over a 4-bit
/// payload — uniform on payload width, so a low-bits bug is a
/// full-payload bug.
#[kani::proof]
fn min_max_snan_preserves_payload() {
    let sign: bool = kani::any();
    let payload4: u8 = kani::any();
    kani::assume(payload4 < 16);
    let payload = u64::from(payload4);
    let snan = Decimal64::from_bits(crate::bid::pack_signaling_nan(sign, payload));

    let (r_min, _) = Decimal64::ONE.min(snan);
    assert!(r_min.is_quiet_nan());
    assert!(r_min.to_bits() & crate::bid::T_MASK == payload);

    let (r_max, _) = Decimal64::ONE.max(snan);
    assert!(r_max.is_quiet_nan());
    assert!(r_max.to_bits() & crate::bid::T_MASK == payload);
}
