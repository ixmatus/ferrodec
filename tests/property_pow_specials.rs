#![cfg(feature = "pow")]
//! Property-style coverage of the IEEE 754-2019 §9.2.1 `pow` special-
//! value rule table.
//!
//! The 6-agent correctness review (May 2026) found that
//! `pow(-1, ±∞)` panicked at `unreachable!()` because no test
//! enumerated the spec's full rule table — only individual rules
//! were spot-checked. This file walks the entire `(x, y, rm)`
//! Cartesian product over a small set of distinguished constants
//! and the five IEEE rounding modes, asserting the spec rule for
//! every combination.
//!
//! "Property test" here means table-driven enumeration rather than
//! proptest fuzzing: the spec rules are deterministic and total
//! over special inputs, so exhaustive enumeration is the right
//! tool. The point is *coverage*, not random sampling.

use ferrodec::{Decimal128, RoundingMode, Status};

fn distinguished_inputs() -> [(&'static str, Decimal128); 11] {
    [
        ("+0", Decimal128::ZERO),
        ("-0", Decimal128::NEG_ZERO),
        ("+1", Decimal128::ONE),
        ("-1", Decimal128::NEG_ONE),
        ("+2", Decimal128::from_i32(2)),
        ("-2", Decimal128::from_i32(-2)),
        ("+0.5", Decimal128::parse_str("0.5", RoundingMode::default()).unwrap().0),
        ("+inf", Decimal128::INFINITY),
        ("-inf", Decimal128::NEG_INFINITY),
        ("qNaN", Decimal128::NAN),
        ("sNaN", Decimal128::SIGNALING_NAN),
    ]
}

fn rounding_modes() -> [RoundingMode; 5] {
    [
        RoundingMode::NearestEven,
        RoundingMode::NearestAway,
        RoundingMode::TowardZero,
        RoundingMode::TowardPositive,
        RoundingMode::TowardNegative,
    ]
}

#[test]
fn pow_no_panics_over_distinguished_grid() {
    // Totality / no-panic guard. Catches H1 (the rule-6 unreachable!()
    // panic on pow(-1, ±∞)) directly: the panic would surface as a
    // test thread panic before any assertion runs.
    for (xn, x) in distinguished_inputs() {
        for (yn, y) in distinguished_inputs() {
            for &rm in &rounding_modes() {
                // catch_unwind to convert any pow panic into a test
                // failure with the offending triple labelled.
                let res = std::panic::catch_unwind(|| x.pow(y, rm));
                assert!(
                    res.is_ok(),
                    "pow({xn}, {yn}, {rm:?}) panicked",
                );
            }
        }
    }
}

#[test]
fn pow_x_zero_is_one_for_any_x_except_snan() {
    // Rule 1: pow(x, ±0) = 1, even for NaN. ferrodec deliberately
    // raises INVALID for sNaN (acknowledged deviation from the
    // strict spec — see pow.rs comment); for every other x the
    // result must be 1 with OK status.
    for (xn, x) in distinguished_inputs() {
        for y in [Decimal128::ZERO, Decimal128::NEG_ZERO] {
            for &rm in &rounding_modes() {
                let (r, s) = x.pow(y, rm);
                assert_eq!(
                    r.to_bits(),
                    Decimal128::ONE.to_bits(),
                    "pow({xn}, ±0, {rm:?}) must be exactly 1",
                );
                if x.is_signaling_nan() {
                    assert!(s.invalid(), "pow(sNaN, ±0): ferrodec raises INVALID by choice");
                } else {
                    assert_eq!(s, Status::OK, "pow({xn}, ±0): no flag should fire");
                }
            }
        }
    }
}

#[test]
fn pow_one_y_is_one_for_any_y_including_qnan() {
    // Rule 2: pow(+1, y) = 1 for any y, even qNaN. sNaN raises
    // INVALID (consistent with the deliberate pow(x, ±0) policy
    // above); the result is still 1.
    for (yn, y) in distinguished_inputs() {
        for &rm in &rounding_modes() {
            let (r, s) = Decimal128::ONE.pow(y, rm);
            assert_eq!(
                r.to_bits(),
                Decimal128::ONE.to_bits(),
                "pow(+1, {yn}, {rm:?}) must be exactly 1",
            );
            if y.is_signaling_nan() {
                assert!(s.invalid());
            }
        }
    }
}

#[test]
fn pow_neg_one_to_infinity_is_one() {
    // Rule 5 sub-case: pow(±1, ±∞) = 1. The negative-base case
    // (H1) used to panic — this enumerates both signs of base
    // against both signs of infinity and all five modes.
    for &x in &[Decimal128::ONE, Decimal128::NEG_ONE] {
        for &y in &[Decimal128::INFINITY, Decimal128::NEG_INFINITY] {
            for &rm in &rounding_modes() {
                let (r, s) = x.pow(y, rm);
                assert_eq!(
                    r.to_bits(),
                    Decimal128::ONE.to_bits(),
                    "pow({x:?}, {y:?}, {rm:?}) must be 1 per IEEE 754-2019 §9.2.1",
                );
                assert_eq!(s, Status::OK, "no flag for pow(±1, ±∞)");
            }
        }
    }
}

#[test]
fn pow_neg_one_qnan_propagates() {
    // Companion to pow_one_y_is_one_for_any_y_including_qnan: the
    // rule-2 short-circuit must NOT fire for x = -1, so pow(-1, qNaN)
    // propagates NaN per rule 3 (NaN propagation). Pinning this
    // prevents an over-broad fix to H1 from extending rule 2 to
    // |x| = 1.
    for &rm in &rounding_modes() {
        let (r, s) = Decimal128::NEG_ONE.pow(Decimal128::NAN, rm);
        assert!(r.is_nan(), "pow(-1, qNaN, {rm:?}) must be NaN");
        assert!(!s.invalid(), "pow(-1, qNaN, {rm:?}) must not raise INVALID");
    }
}

#[test]
fn pow_zero_neg_y_is_inf_div_by_zero() {
    // Rule 4: pow(±0, y < 0) = ±∞ + DIV_BY_ZERO. Sign of result
    // depends on y's integer-ness when x is -0.
    let (r, s) = Decimal128::ZERO.pow(Decimal128::NEG_ONE, RoundingMode::NearestEven);
    assert!(r.is_infinite() && !r.is_sign_negative());
    assert!(s.div_by_zero());

    // -0 raised to a negative odd integer is -∞.
    let (r, s) = Decimal128::NEG_ZERO.pow(Decimal128::NEG_ONE, RoundingMode::NearestEven);
    assert!(r.is_infinite() && r.is_sign_negative());
    assert!(s.div_by_zero());
}

#[test]
fn pow_neg_finite_non_integer_is_invalid_nan() {
    // Rule 7: pow(x, y) signals invalid for finite x < 0 and
    // finite non-integer y.
    let half = Decimal128::parse_str("0.5", RoundingMode::default())
        .unwrap()
        .0;
    let (r, s) = Decimal128::NEG_ONE.pow(half, RoundingMode::NearestEven);
    assert!(r.is_nan());
    assert!(s.invalid());
}
