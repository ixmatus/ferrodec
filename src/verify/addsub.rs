//! Kani harnesses for `Decimal128::add` / `Decimal128::sub`.
//!
//! ### Strategy
//!
//! `add()`'s body unconditionally references the alignment +
//! rounding pipeline (via `add_finite_finite` → `mul_pow10` →
//! `round_and_pack_finite`). Path-based symbolic execution treats those
//! reachable functions as part of the proof obligation, even when the
//! call is statically unreachable under our `assume()`. Result: an
//! "any-`u128`" symbolic operand drags the SAT problem through 35
//! `mul_pow10` iterations × 78 `decimal_digit_count` iterations, which
//! is intractable.
//!
//! Two mitigations:
//!
//! 1. The special-case path is exposed as
//!    [`Decimal128::add_special_only_for_kani`] (`cfg(kani)` only),
//!    returning `None` when finite-finite arithmetic is required. The
//!    NaN / Inf / Zero proofs target that function — CBMC never has to
//!    look at the alignment pipeline.
//! 2. Operand bit-patterns are sampled from a *small* class selector
//!    rather than a free 128-bit symbolic value. The selector ranges
//!    over a representative set of constants
//!    (`NAN`, `SIGNALING_NAN`, `±INFINITY`, `±ZERO`, `±ONE`, `±MAX`).
//!    Together with mode symbolic-but-bounded, the SAT problem is
//!    measured in tens of seconds.
//!
//! Finite-finite arithmetic correctness is delegated to the proptest
//! harness in `tests/property_addsub.rs`.

use crate::status::{RoundingMode, Status};
use crate::Decimal128;

const NUM_OPERANDS: u8 = 10;

/// Map a small selector to one of ten representative `Decimal128` values.
fn operand(idx: u8) -> Decimal128 {
    match idx {
        0 => Decimal128::NAN,
        1 => Decimal128::SIGNALING_NAN,
        2 => Decimal128::INFINITY,
        3 => Decimal128::NEG_INFINITY,
        4 => Decimal128::ZERO,
        5 => Decimal128::NEG_ZERO,
        6 => Decimal128::ONE,
        7 => Decimal128::NEG_ONE,
        8 => Decimal128::MAX,
        _ => Decimal128::MIN,
    }
}

fn rm_from_u8(x: u8) -> RoundingMode {
    match x {
        0 => RoundingMode::NearestEven,
        1 => RoundingMode::NearestAway,
        2 => RoundingMode::TowardZero,
        3 => RoundingMode::TowardPositive,
        _ => RoundingMode::TowardNegative,
    }
}

/// Whenever at least one operand is non-finite or zero, the special-case
/// path resolves to `Some` — i.e. NaN / Inf / Zero never fall through to
/// the alignment pipeline.
#[kani::proof]
fn special_resolves_when_either_is_non_finite_or_zero() {
    let ai: u8 = kani::any();
    let bi: u8 = kani::any();
    let rmi: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    kani::assume(bi < NUM_OPERANDS);
    kani::assume(rmi <= 4);

    let a = operand(ai);
    let b = operand(bi);
    kani::assume(
        a.is_nan()
            || a.is_infinite()
            || a.is_zero()
            || b.is_nan()
            || b.is_infinite()
            || b.is_zero(),
    );
    let mode = rm_from_u8(rmi);

    let result = a.add_special_only_for_kani(b, mode);
    assert!(result.is_some());
}

/// NaN propagates through `add_special_only_for_kani` for any
/// operand class.
#[kani::proof]
fn nan_propagates_through_special() {
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

/// Signaling NaN raises `INVALID` in `add_special_only_for_kani`.
#[kani::proof]
fn snan_raises_invalid_through_special() {
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

/// `±∞ + ±∞`: same-sign infinities give the same infinity; opposite
/// signs give NaN+`INVALID`.
#[kani::proof]
fn infinity_arithmetic_via_special() {
    let sa: bool = kani::any();
    let sb: bool = kani::any();

    let a = if sa {
        Decimal128::NEG_INFINITY
    } else {
        Decimal128::INFINITY
    };
    let b = if sb {
        Decimal128::NEG_INFINITY
    } else {
        Decimal128::INFINITY
    };

    let (r, status) = a
        .add_special_only_for_kani(b, RoundingMode::NearestEven)
        .expect("inf+inf is special-cased");

    if sa == sb {
        assert!(r.is_infinite());
        assert!(r.is_sign_negative() == sa);
        assert!(!status.invalid());
    } else {
        assert!(r.is_nan());
        assert!(status.invalid());
    }
}

/// `±∞ + finite_non_nan = ±∞` (with the infinity's sign).
#[kani::proof]
fn infinity_plus_finite_via_special() {
    let inf_sign: bool = kani::any();
    let inf = if inf_sign {
        Decimal128::NEG_INFINITY
    } else {
        Decimal128::INFINITY
    };

    // Finite operand drawn from the representative set.
    let oi: u8 = kani::any();
    kani::assume(oi >= 4 && oi < NUM_OPERANDS); // ZERO/NEG_ZERO/ONE/NEG_ONE/MAX/MIN
    let other = operand(oi);

    let (r, _) = inf
        .add_special_only_for_kani(other, RoundingMode::NearestEven)
        .expect("inf+finite is special-cased");
    assert!(r.is_infinite());
    assert!(r.is_sign_negative() == inf_sign);

    let (r, _) = other
        .add_special_only_for_kani(inf, RoundingMode::NearestEven)
        .expect("finite+inf is special-cased");
    assert!(r.is_infinite());
    assert!(r.is_sign_negative() == inf_sign);
}

/// `0 + 0` sign rule (IEEE 754 §6.3): TowardNegative gives `−0` for
/// mixed-sign zeros; every other mode gives `+0`. Same-sign zeros
/// preserve the sign.
#[kani::proof]
fn zero_plus_zero_sign_rule_via_special() {
    let sa: bool = kani::any();
    let sb: bool = kani::any();
    let rmi: u8 = kani::any();
    kani::assume(rmi <= 4);

    let a = if sa { Decimal128::NEG_ZERO } else { Decimal128::ZERO };
    let b = if sb { Decimal128::NEG_ZERO } else { Decimal128::ZERO };
    let mode = rm_from_u8(rmi);

    let (r, _) = a
        .add_special_only_for_kani(b, mode)
        .expect("0+0 is special-cased");
    assert!(r.is_zero());

    let expected_negative = if sa == sb {
        sa
    } else {
        matches!(mode, RoundingMode::TowardNegative)
    };
    assert!(r.is_sign_negative() == expected_negative);
}

/// `add(a, ±0)` returns `a` unchanged (preserving cohort) for any
/// representative non-NaN, non-Inf operand `a`.
#[kani::proof]
fn add_zero_is_identity_via_special() {
    let ai: u8 = kani::any();
    kani::assume(ai >= 4 && ai < NUM_OPERANDS); // ZERO/NEG_ZERO/ONE/NEG_ONE/MAX/MIN
    let zero_idx: u8 = kani::any();
    kani::assume(zero_idx == 4 || zero_idx == 5); // +0 or -0

    let a = operand(ai);
    let zero = operand(zero_idx);

    let (r, status) = a
        .add_special_only_for_kani(zero, RoundingMode::NearestEven)
        .expect("a + 0 is special-cased");
    if !a.is_zero() {
        assert!(r.to_bits() == a.to_bits());
        assert!(status.is_ok());
    }
}

/// Basic sanity that `Status` carries the same flag bits round-tripped
/// through the special-case path. (Catches accidental flag drops.)
#[kani::proof]
fn special_status_only_invalid_for_snan() {
    let ai: u8 = kani::any();
    let bi: u8 = kani::any();
    let rmi: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    kani::assume(bi < NUM_OPERANDS);
    kani::assume(rmi <= 4);

    let a = operand(ai);
    let b = operand(bi);
    let mode = rm_from_u8(rmi);

    if let Some((_, status)) = a.add_special_only_for_kani(b, mode) {
        // INVALID can be set by sNaN inputs OR by Inf-Inf cancellation.
        let snan_in = a.is_signaling_nan() || b.is_signaling_nan();
        let inf_minus_inf = a.is_infinite()
            && b.is_infinite()
            && a.is_sign_negative() != b.is_sign_negative();
        if !snan_in && !inf_minus_inf {
            assert!(!status.invalid());
        }
        // The other flags are never raised by the special-case path.
        assert!(!status.div_by_zero());
        assert!(!status.overflow());
        assert!(!status.underflow());
        assert!(!status.inexact());
        let _ = Status::OK;
    }
}
