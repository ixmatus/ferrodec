#![cfg(feature = "fmt")]
//! H8 regression: `parse_str` must not panic (debug) or silently
//! miscompute (release) on adversarial digit runs that overflow the
//! internal `u32` exponent counters.
//!
//! Agent 5 findings B2 (leading fractional zeros) and B7 (trailing
//! integer zeros). These inputs need a multi-megabyte string, so the
//! test lives in an integration crate (std available) rather than the
//! `no_std` in-crate unit module. Digit runs past
//! `MAX_EXPONENT_MAGNITUDE` (one million) positions resolve to a
//! clean `CoefficientOverflow` (the variant ADR-0029 item 2 / fd-7f1
//! made matchable), which the conformance harness skips; no decTest
//! vector is megabytes long. The explicit exponent *field* is
//! different: past the cap it saturates, and the value overflows or
//! underflows with the usual flags (ADR-0057).

use core::cmp::Ordering;

use ferrodec_decimal64::{Decimal64, ParseDecimalError, RoundingMode};

#[test]
fn leading_fractional_zeros_past_cap_is_clean_error() {
    // B2: drives the `leading_frac_zero` branch, which is not bounded
    // by `digits_total`. Pre-fix this overflowed `digits_after_point:
    // u32` and panicked in a debug build.
    let s = format!("0.{}1", "0".repeat(1_000_001));
    assert!(matches!(
        Decimal64::parse_str(&s, RoundingMode::NearestEven),
        Err(ParseDecimalError::CoefficientOverflow)
    ));
}

#[test]
fn trailing_integer_zeros_past_cap_is_clean_error() {
    // B7: drives `extra_int_digits`; pre-fix it saturated to
    // u32::MAX and reinterpreted as -1 under the `as i32` cast,
    // silently miscomputing the exponent. `extra_int_digits` only
    // starts counting past the first MAX_PARSED_DIGITS (19) folded
    // digits, so the run must clear 1_000_000 + 19.
    let s = format!("1{}", "0".repeat(1_100_000));
    assert!(matches!(
        Decimal64::parse_str(&s, RoundingMode::NearestEven),
        Err(ParseDecimalError::CoefficientOverflow)
    ));
}

#[test]
fn modest_runs_still_parse() {
    // The cap must not regress legitimate small-quantum inputs:
    // 0.<50 zeros>1 = 1e-51 has a single significant digit, so the
    // parsed cohort is exactly (coef 1, exp -51) — bit-exact.
    let s = format!("0.{}1", "0".repeat(50));
    let (d, _) = Decimal64::parse_str(&s, RoundingMode::NearestEven).unwrap();
    let one_e_neg_51 = Decimal64::try_new(1, -51).unwrap();
    assert_eq!(d.to_bits(), one_e_neg_51.to_bits());

    // A long trailing-integer run under the cap scales correctly.
    // 1 followed by 300 zeros folds 19 digits then accumulates
    // extra_int_digits; the 19-to-16 digit rounding changes the
    // cohort but not the value, so compare numerically.
    let s = format!("1{}", "0".repeat(300));
    let (d, _) = Decimal64::parse_str(&s, RoundingMode::NearestEven).unwrap();
    let one_e_300 = Decimal64::try_new(1, 300).unwrap();
    assert_eq!(
        d.partial_cmp(one_e_300).0,
        Some(Ordering::Equal),
        "1e300 via long zero run equals try_new(1, 300)"
    );
}
