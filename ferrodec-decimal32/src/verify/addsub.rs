//! Kani harnesses for `Decimal32::add` and `Decimal32::sub`.
//!
//! Harnesses route through `add_special_only_for_kani` /
//! `sub_special_only_for_kani` per ADR-0016. The shims skip the
//! finite-finite alignment + rounding pipeline, so CBMC never has
//! to symbolically encode `drop_excess_digits` or
//! `round_and_pack_finite` — the wedge that drove the sibling-tree
//! kani timeouts pre-1.15.
//!
//! Claims covered:
//!
//! 1. Special-case dispatch resolves to `Some` whenever an operand
//!    is NaN or Infinity (the classes the dispatcher answers in
//!    closed form).
//! 2. NaN propagation: a NaN input produces a NaN result through
//!    the dispatcher.
//! 3. Signaling NaN raises `INVALID`.
//! 4. `(±∞) + (±∞)` of opposite sign → NaN + INVALID; same sign →
//!    same-signed infinity.
//!
//! The IEEE 754-2019 §6.3 zero-sign rule (`(+0) + (−0) = +0` except
//! in `TowardNegative`) is **not** pinned here: decimal32's
//! dispatcher returns `None` for `(Zero, Zero)` and the rule lives on
//! the finite path, so a §6.3 harness cannot route through the shim.
//! See the note at the bottom of this file; the rule is covered by the
//! vendored `dsAdd` decTest vectors and this crate's in-module tests
//! (there is no `tests/property_addsub.rs` here; that oracle is the
//! Decimal128 parent's, fd-aqs.15).

use super::{operand, rm_from_u8, NUM_OPERANDS};
use crate::decimal::Decimal32;
use ferrodec_ieee::RoundingMode;

/// Whenever at least one operand is NaN or Infinity, the add
/// special-case path resolves to `Some` — only `(Zero|Finite,
/// Zero|Finite)` falls through to the finite path. (See the
/// docstring at the bottom of this file for the §6.3 zero-zero
/// note.)
#[kani::proof]
fn add_special_resolves_on_nan_or_infinity() {
    let ai: u8 = kani::any();
    let bi: u8 = kani::any();
    let rmi: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    kani::assume(bi < NUM_OPERANDS);
    kani::assume(rmi <= 4);

    let a = operand(ai);
    let b = operand(bi);
    let mode = rm_from_u8(rmi);
    kani::assume(a.is_nan() || a.is_infinite() || b.is_nan() || b.is_infinite());

    assert!(a.add_special_only_for_kani(b, mode).is_some());
}

/// Same for `sub`. (`sub` short-circuits via the same dispatcher
/// after a sign flip on `rhs`, so the operand classes that reach
/// the dispatcher match `add`.)
#[kani::proof]
fn sub_special_resolves_on_nan_or_infinity() {
    let ai: u8 = kani::any();
    let bi: u8 = kani::any();
    let rmi: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    kani::assume(bi < NUM_OPERANDS);
    kani::assume(rmi <= 4);

    let a = operand(ai);
    let b = operand(bi);
    let mode = rm_from_u8(rmi);
    kani::assume(a.is_nan() || a.is_infinite() || b.is_nan() || b.is_infinite());

    assert!(a.sub_special_only_for_kani(b, mode).is_some());
}

/// NaN propagates through `add_special_only_for_kani` for any operand
/// class.
#[kani::proof]
fn add_nan_propagates() {
    let ai: u8 = kani::any();
    let bi: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    kani::assume(bi < NUM_OPERANDS);

    let a = operand(ai);
    let b = operand(bi);
    kani::assume(a.is_nan() || b.is_nan());

    let (r, _) = a
        .add_special_only_for_kani(b, RoundingMode::NearestEven)
        .expect("NaN path resolved by special_cases");
    assert!(r.is_nan());
}

/// Signaling NaN raises `INVALID`.
#[kani::proof]
fn add_snan_raises_invalid() {
    let ai: u8 = kani::any();
    let bi: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    kani::assume(bi < NUM_OPERANDS);

    let a = operand(ai);
    let b = operand(bi);
    kani::assume(a.is_signaling_nan() || b.is_signaling_nan());

    let (_, status) = a
        .add_special_only_for_kani(b, RoundingMode::NearestEven)
        .expect("sNaN path resolved by special_cases");
    assert!(status.invalid());
}

/// `(±∞) + (±∞)`: same sign → same-signed infinity; opposite sign →
/// NaN + INVALID. Pinned through the dispatcher.
#[kani::proof]
fn add_infinity_arithmetic() {
    let sa: bool = kani::any();
    let sb: bool = kani::any();
    let a = if sa {
        Decimal32::NEG_INFINITY
    } else {
        Decimal32::INFINITY
    };
    let b = if sb {
        Decimal32::NEG_INFINITY
    } else {
        Decimal32::INFINITY
    };

    let (r, status) = a
        .add_special_only_for_kani(b, RoundingMode::NearestEven)
        .expect("∞ + ∞ resolved by special_cases");
    if sa == sb {
        assert!(r.is_infinite());
        assert!(r.is_sign_negative() == sa);
        assert!(!status.invalid());
    } else {
        assert!(r.is_nan());
        assert!(status.invalid());
    }
}

// IEEE 754-2019 §6.3 zero-sign rule (`(+0) + (−0) = +0` except in
// `TowardNegative` where it yields `−0`) is **not** pinned through
// the Decimal32 shim: decimal32's `handle_specials` returns `None`
// for `(Zero, Zero)` — the rule is implemented in the finite path's
// zero-coefficient branch via `zero_sum_sign`. Surfacing the §6.3
// rule through the dispatcher requires the same refactor decimal128
// already has (`add_special_cases` returns Some for `(Zero, Zero)`).
// Tracked for a follow-up: extend decimal32's dispatcher with the
// (Zero, Zero) / (Zero, Finite) / (Finite, Zero) arms so this
// harness can route through the shim. Until then, the rule is
// covered by the `dsAdd` decTest vectors and the in-module tests.
