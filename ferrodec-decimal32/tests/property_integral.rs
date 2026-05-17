//! Exact correctly-rounded oracle for `Decimal32::round_to_integral`
//! and `round_to_integral_exact` (fd-hnx).
//!
//! Round-to-integral is an *exact* operation: the result is the operand
//! rounded to an integer under the rounding direction, with the GDA
//! preferred quantum `max(exponent, 0)` — no precision loss, no
//! over/underflow, `INVALID` only for a signaling NaN, and `INEXACT`
//! only for the `…Exact` variant when the operand had a non-zero
//! fractional part. So the oracle is not a tolerance check: it predicts
//! the exact result cohort and status and the test asserts the decoded
//! `(sign, coefficient, exponent)` triple, across the finite domain and
//! every rounding direction.
//!
//! The reference is computed independently of the production kernel:
//! the operand is decoded with the cohort-faithful BID decoder
//! ([`decode_decimal32`], itself sanity-checked as the exact inverse of
//! `pack_finite`), the digit split is done on the *decimal string* (so
//! an exponent as low as `qmin` never overflows a `10^drop`), and the
//! rounding decision is `ferrodec_test_support::oracle::round_up_decision`
//! — the §4.3.3 table transcribed independently of `should_round_up_int`.
//! Because at least one fractional digit is always dropped when
//! `exp < 0`, the kept integer has at most `precision − 1` digits, so
//! the carry-into-a-new-decade branch is unreachable and the predicted
//! result is always `new_coef` at quantum `0`.

#![cfg(feature = "fmt")]

use ferrodec_decimal32::{Decimal32, RoundingMode, Status};
use ferrodec_test_support::conformance::status_conformance_eq;
use ferrodec_test_support::oracle::{decode_decimal32, round_up_decision};
use proptest::prelude::*;
use proptest::test_runner::TestCaseError;

const MODES: &[RoundingMode] = &[
    RoundingMode::NearestEven,
    RoundingMode::NearestAway,
    RoundingMode::TowardZero,
    RoundingMode::TowardPositive,
    RoundingMode::TowardNegative,
];

/// What the independent reference predicts.
enum Pred {
    /// Returned unchanged (already integral at quantum ≥ 0, or ±∞).
    Unchanged,
    /// sNaN → quiet NaN + INVALID; qNaN → quiet NaN, no flag.
    Nan { invalid: bool },
    /// A finite cohort `(sign, coefficient, quantum 0)`, with `INEXACT`
    /// iff the `…Exact` variant dropped a non-zero fractional part.
    Cohort {
        neg: bool,
        coef: u128,
        inexact: bool,
    },
}

fn oracle_integral(x: Decimal32, rm: RoundingMode, exact_variant: bool) -> Pred {
    if x.is_signaling_nan() {
        return Pred::Nan { invalid: true };
    }
    if x.is_nan() {
        return Pred::Nan { invalid: false };
    }
    if x.is_infinite() {
        return Pred::Unchanged;
    }

    let (neg, coeff_big, exp) = decode_decimal32(x.to_bits());
    if exp >= 0 {
        // Already integral at quantum ≥ 0 — returned unchanged, never
        // inexact (no fractional digit dropped).
        return Pred::Unchanged;
    }

    // A Form-B encoding whose raw coefficient is ≥ 10^7 is
    // non-canonical and, per the BID-32 layout (`src/bid.rs`), denotes
    // ±0 with the encoded sign and exponent — exactly how the
    // production decoder canonicalises it. `decode_decimal32` returns
    // the *raw* coefficient, so apply that rule here to keep the oracle
    // faithful to the value ferrodec actually operates on.
    let coeff_val: u128 = coeff_big
        .to_string()
        .parse()
        .expect("decoded coefficient fits u128");
    let is_zero = coeff_val == 0 || coeff_val >= 10u128.pow(7);
    let coeff = if is_zero {
        "0".to_string()
    } else {
        coeff_big.to_string()
    };
    let drop = (-exp) as usize;
    let digits: Vec<u8> = coeff.bytes().map(|b| b - b'0').collect();
    let len = digits.len();

    let (kept_int, round_digit, sticky): (u128, u8, bool) = if is_zero {
        (0, 0, false)
    } else if drop >= len {
        if drop == len {
            let rd = digits[0];
            let st = digits[1..].iter().any(|&d| d != 0);
            (0, rd, st)
        } else {
            (0, 0, true) // round position over a leading zero; coeff ≠ 0 ⇒ sticky
        }
    } else {
        let split = len - drop;
        let kept: u128 = digits[..split]
            .iter()
            .fold(0u128, |a, &d| a * 10 + u128::from(d));
        let rd = digits[split];
        let st = digits[split + 1..].iter().any(|&d| d != 0);
        (kept, rd, st)
    };

    let last_kept_lsb = (kept_int % 10) as u8;
    let round_up = round_up_decision(rm, neg, last_kept_lsb, round_digit, sticky);
    let new_coef = kept_int + u128::from(round_up);
    let inexact = exact_variant && (round_digit != 0 || sticky);
    Pred::Cohort {
        neg,
        coef: new_coef,
        inexact,
    }
}

fn check(x: Decimal32, rm: RoundingMode, exact_variant: bool) -> Result<(), TestCaseError> {
    let (got, gs) = if exact_variant {
        x.round_to_integral_exact(rm)
    } else {
        x.round_to_integral(rm)
    };
    let want = oracle_integral(x, rm, exact_variant);
    let variant = if exact_variant { "_exact" } else { "" };

    let expected_status = match &want {
        Pred::Nan { invalid: true } => Status::INVALID,
        Pred::Cohort { inexact: true, .. } => Status::INEXACT,
        _ => Status::OK,
    };

    match want {
        Pred::Nan { .. } => {
            prop_assert!(
                got.is_nan() && !got.is_signaling_nan(),
                "round_to_integral{variant}({x:e}) rm={rm:?}: expected quiet NaN, got {got:e}"
            );
        }
        Pred::Unchanged => {
            prop_assert!(
                got.to_bits() == x.to_bits(),
                "round_to_integral{variant}({x:e}) rm={rm:?}: expected unchanged {:#010x}, got {:#010x}",
                x.to_bits(),
                got.to_bits()
            );
        }
        Pred::Cohort { neg, coef, .. } => {
            let (gneg, gcoef, gexp) = decode_decimal32(got.to_bits());
            prop_assert!(
                gneg == neg && gexp == 0 && gcoef == coef.into(),
                "value round_to_integral{variant}({x:e}) rm={rm:?}: got {got:e} decoded ({gneg}, {gcoef}, {gexp}), want ({neg}, {coef}, 0)"
            );
        }
    }

    prop_assert!(
        status_conformance_eq(gs, expected_status),
        "status round_to_integral{variant}({x:e}) rm={rm:?}: got {gs:?} want {expected_status:?}"
    );
    Ok(())
}

/// Broad full-domain finite operands (every cohort the bit pattern can
/// name) unioned with a structured generator that deterministically
/// lands fractional values across the digit-drop boundary.
fn finite() -> impl Strategy<Value = Decimal32> {
    let broad = any::<u32>()
        .prop_map(Decimal32::from_bits)
        .prop_filter("finite", |d| d.is_finite());
    let structured = (any::<bool>(), 0u32..=9_999_999u32, -20i32..=5i32)
        .prop_map(|(s, c, e)| {
            let sign = if s { "-" } else { "" };
            Decimal32::parse_str(&format!("{sign}{c}E{e}"), RoundingMode::NearestEven)
                .expect("constructed decimal string parses")
                .0
        })
        .prop_filter("finite", |d| d.is_finite());
    prop_oneof![broad, structured]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(4096))]

    /// `round_to_integral` and `round_to_integral_exact` are the exact
    /// correctly-rounded integral value, cohort-faithful and with exact
    /// status, across the finite domain and every IEEE rounding
    /// direction.
    #[test]
    fn round_to_integral_is_exact(x in finite(), rm_idx in 0u8..5, exact in any::<bool>()) {
        check(x, MODES[rm_idx as usize], exact)?;
    }
}

// Spot tests — worked boundary cases. -----------------------------------

fn parse(s: &str) -> Decimal32 {
    Decimal32::parse_str(s, RoundingMode::NearestEven)
        .unwrap()
        .0
}

#[test]
fn spot_half_ties() {
    let cases = [
        (RoundingMode::NearestEven, "0"),
        (RoundingMode::NearestAway, "1"),
        (RoundingMode::TowardPositive, "1"),
        (RoundingMode::TowardNegative, "0"),
        (RoundingMode::TowardZero, "0"),
    ];
    for (rm, want) in cases {
        let (g, _) = parse("0.5").round_to_integral(rm);
        let (cmp, _) = g.partial_cmp(parse(want));
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal), "0.5 rm={rm:?}");
    }
}

#[test]
fn spot_exact_variant_inexact_flag() {
    let (_, s) = parse("2.3").round_to_integral_exact(RoundingMode::NearestEven);
    assert!(s.inexact());
    let (_, s2) = parse("2").round_to_integral_exact(RoundingMode::NearestEven);
    assert!(!s2.inexact());
    let (_, s3) = parse("2.3").round_to_integral(RoundingMode::NearestEven);
    assert!(!s3.inexact());
}

#[test]
fn spot_already_integer_keeps_cohort() {
    let x = parse("1E+3");
    let (g, s) = x.round_to_integral(RoundingMode::TowardZero);
    assert_eq!(g.to_bits(), x.to_bits());
    assert!(s.is_ok());
}

#[test]
fn spot_negative_sub_one_keeps_sign() {
    let (g, _) = parse("-0.4").round_to_integral(RoundingMode::TowardZero);
    assert!(g.is_zero() && g.is_sign_negative());
}
