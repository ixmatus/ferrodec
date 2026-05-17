//! Exact correctly-rounded oracle for `Decimal128::round_to_integral`
//! and `round_to_integral_exact` (S11, ADR-0021).
//!
//! Round-to-integral is an *exact* operation: the result is the operand
//! rounded to an integer under the rounding direction, with the GDA
//! preferred quantum `max(exponent, 0)` — no precision loss, no
//! over/underflow, `INVALID` only for a signaling NaN, and `INEXACT`
//! only for the `…Exact` variant when the operand had a non-zero
//! fractional part. So the oracle is not a tolerance check: it predicts
//! the exact result cohort and status and the test asserts `to_bits`
//! equality, across the full finite domain and every rounding
//! direction.
//!
//! The reference is computed independently of the production kernel:
//! the operand is decoded with the cohort-faithful BID decoder, the
//! digit split is done on the *decimal string* (so an exponent as low
//! as `qmin` never overflows a `10^drop`), and the rounding decision is
//! `ferrodec_test_support::oracle::round_up_decision` — the §4.3.3
//! table transcribed independently of `should_round_up_int`.

#![cfg(feature = "fmt")]

use proptest::prelude::*;

use ferrodec::{Decimal128, RoundingMode};
use ferrodec_test_support::conformance::status_conformance_eq;
use ferrodec_test_support::oracle::{self, round_up_decision};

const MODES: &[RoundingMode] = &[
    RoundingMode::NearestEven,
    RoundingMode::NearestAway,
    RoundingMode::TowardZero,
    RoundingMode::TowardPositive,
    RoundingMode::TowardNegative,
];

const BIAS_U32: u32 = 6176;

fn decimal_finite(sign: bool, biased_exp: u32, coef: u128) -> Decimal128 {
    debug_assert!(coef < 1u128 << 113);
    debug_assert!(biased_exp <= 12287);
    let s = (sign as u128) << 127;
    let exp_high2 = ((biased_exp >> 12) & 0b11) as u128;
    let coef_high3 = (coef >> 110) & 0b111;
    let type_bits = (exp_high2 << 3) | coef_high3;
    let ec = (biased_exp & 0xFFF) as u128;
    let t = coef & ((1u128 << 110) - 1);
    Decimal128::from_bits(s | (type_bits << 122) | (ec << 110) | t)
}

/// Decoded result the oracle predicts: a finite `(sign, coeff, exp)`
/// cohort, or "unchanged" (the operand's own bits), plus the status.
struct Expect {
    bits: u128,
    status_inexact: bool,
    status_invalid: bool,
}

/// Independent round-to-integral reference.
fn oracle_integral(x: Decimal128, rm: RoundingMode, exact_variant: bool) -> Expect {
    if x.is_signaling_nan() {
        // sNaN → quiet NaN (same sign/payload) + INVALID. The kernel
        // builds it via `pack_quiet_nan`; we only assert it is a quiet
        // NaN and the flag, mirroring the project NaN posture.
        return Expect {
            bits: x.to_bits(), // sentinel; comparison special-cases NaN
            status_inexact: false,
            status_invalid: true,
        };
    }
    if x.is_nan() || x.is_infinite() {
        return Expect {
            bits: x.to_bits(),
            status_inexact: false,
            status_invalid: false,
        };
    }

    // Finite (including zero): cohort-faithful decode.
    let (neg, coeff_big, exp) = oracle::decode_decimal128(x.to_bits());
    let coeff = coeff_big.to_string();
    let is_zero = coeff == "0";

    if exp >= 0 {
        // Already integral at quantum ≥ 0 — returned unchanged, never
        // inexact (no fractional part dropped).
        return Expect {
            bits: x.to_bits(),
            status_inexact: false,
            status_invalid: false,
        };
    }

    // exp < 0: drop `-exp` fractional digits.
    let drop = (-exp) as usize;
    let digits: Vec<u8> = coeff.bytes().map(|b| b - b'0').collect();
    let len = digits.len();

    let (kept_int, round_digit, sticky): (u128, u8, bool) = if is_zero {
        (0, 0, false)
    } else if drop >= len {
        // |x| < 1. Most-significant fractional digit is the round digit
        // only when the round position is exactly the MSD (drop == len);
        // otherwise it sits over a leading zero (drop > len).
        if drop == len {
            let rd = digits[0];
            let st = digits[1..].iter().any(|&d| d != 0);
            (0, rd, st)
        } else {
            (0, 0, true) // coeff is non-zero ⇒ sticky
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
    Expect {
        bits: decimal_finite(neg, BIAS_U32, new_coef).to_bits(),
        status_inexact: inexact,
        status_invalid: false,
    }
}

fn check(x: Decimal128, rm: RoundingMode, exact_variant: bool) -> Result<(), TestCaseError> {
    let (got, gs) = if exact_variant {
        x.round_to_integral_exact(rm)
    } else {
        x.round_to_integral(rm)
    };
    let want = oracle_integral(x, rm, exact_variant);

    let variant = if exact_variant { "_exact" } else { "" };
    if x.is_nan() {
        prop_assert!(
            got.is_nan(),
            "round_to_integral{}({:e}) rm={:?}: expected NaN, got {:e}",
            variant,
            x,
            rm,
            got
        );
        if x.is_signaling_nan() {
            prop_assert!(!got.is_signaling_nan());
        }
    } else {
        prop_assert!(
            got.to_bits() == want.bits,
            "value round_to_integral{}({:e}) rm={:?}: got {:e} ({:032X}) want {:032X}",
            variant,
            x,
            rm,
            got,
            got.to_bits(),
            want.bits
        );
    }
    let mut expected = ferrodec::Status::OK;
    if want.status_inexact {
        expected = ferrodec::Status::INEXACT;
    }
    if want.status_invalid {
        expected = ferrodec::Status::INVALID;
    }
    prop_assert!(
        status_conformance_eq(gs, expected),
        "status round_to_integral{}({:e}) rm={:?}: got {:?} want {:?}",
        variant,
        x,
        rm,
        gs,
        expected
    );
    Ok(())
}

fn finite() -> impl Strategy<Value = Decimal128> {
    (
        any::<bool>(),
        prop_oneof![
            0u32..=64u32,
            (BIAS_U32 - 60)..=(BIAS_U32 + 60),
            (12287u32 - 64)..=12287u32,
        ],
        prop_oneof![
            0u128..=9,
            0u128..=1_000_000,
            0u128..=10u128.pow(20),
            0u128..=(10u128.pow(34) - 1),
        ],
    )
        .prop_map(|(s, e, c)| decimal_finite(s, e, c))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(4096))]

    /// `round_to_integral` and `round_to_integral_exact` are the exact
    /// correctly-rounded integral value, bit-for-bit (cohort included)
    /// with exact status, across the full finite domain and every IEEE
    /// rounding direction.
    #[test]
    fn round_to_integral_is_exact(x in finite(), rm_idx in 0u8..5, exact in any::<bool>()) {
        check(x, MODES[rm_idx as usize], exact)?;
    }
}

// Spot tests — the worked boundary cases. -------------------------------

fn parse(s: &str) -> Decimal128 {
    Decimal128::parse_str(s, RoundingMode::NearestEven)
        .unwrap()
        .0
}

#[test]
fn spot_half_ties() {
    // 0.5 → ties: even 0, away 1, +∞ 1, −∞ 0, zero 0.
    let cases = [
        (RoundingMode::NearestEven, "0"),
        (RoundingMode::NearestAway, "1"),
        (RoundingMode::TowardPositive, "1"),
        (RoundingMode::TowardNegative, "0"),
        (RoundingMode::TowardZero, "0"),
    ];
    for (rm, want) in cases {
        let (g, _) = parse("0.5").round_to_integral(rm);
        assert_eq!(g.to_bits(), parse(want).round_to_integral(rm).0.to_bits());
        let (g2, _) = parse("0.5").round_to_integral(rm);
        let (cmp, _) = g2.partial_cmp(parse(want));
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal), "0.5 rm={rm:?}");
    }
}

#[test]
fn spot_exact_variant_inexact_flag() {
    let (_, s) = parse("2.3").round_to_integral_exact(RoundingMode::NearestEven);
    assert!(s.inexact());
    let (_, s2) = parse("2").round_to_integral_exact(RoundingMode::NearestEven);
    assert!(!s2.inexact());
    // Non-exact variant never raises INEXACT.
    let (_, s3) = parse("2.3").round_to_integral(RoundingMode::NearestEven);
    assert!(!s3.inexact());
}

#[test]
fn spot_already_integer_keeps_cohort() {
    // 1E+3 is integral at quantum +3 — returned unchanged.
    let x = parse("1E+3");
    let (g, s) = x.round_to_integral(RoundingMode::TowardZero);
    assert_eq!(g.to_bits(), x.to_bits());
    assert!(s.is_ok());
}

#[test]
fn spot_negative_sub_one_keeps_sign() {
    // -0.4 → -0 (sign preserved) under every mode that does not round
    // away.
    let (g, _) = parse("-0.4").round_to_integral(RoundingMode::TowardZero);
    assert!(g.is_zero() && g.is_sign_negative());
}
