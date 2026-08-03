//! Faithful-rounding contract for `Decimal128::log10_1p` (IEEE
//! 754-2019 §9.2 `log10p1`) against astro-float, asserted for every
//! rounding direction (ADR-0021's floor under ADR-0059's correctly
//! rounded claim). See `tests/common/mod.rs`; this is not a `± ULP`
//! tolerance envelope.
//!
//! The oracle is local rather than one of the shared
//! `transcend_oracle::oracle` builders because `log10p1` needs its
//! argument formed *inside* the oracle: `1 + x` at the shared 256 bit
//! width keeps only about 77 decimal digits, which a tiny `x` would
//! spend entirely on the leading 1. Forming the sum at 1024 bits
//! (about 308 decimal digits) leaves a `10^-50` input more than 250
//! digits of headroom, far beyond the 34 digit bracket under test.
//! The kernel itself never forms `1 + x` at format width; that is the
//! whole point of the operation.

#![cfg(feature = "exp-log")]

use astro_float::{BigFloat, Consts, Radix, RoundingMode as AfRm};
use proptest::prelude::*;

mod common;
use common::{assert_faithful, parse, MODES};

/// Oracle working precision in bits (about 308 decimal digits). Wide
/// enough that `1 + x` keeps every digit of a `10^-50` input.
const P_ORACLE: usize = 1024;

/// `log10(1 + x)` of the exact value `x_sci`, formed at oracle width.
fn oracle_log10p1(x_sci: &str, cc: &mut Consts) -> BigFloat {
    let x = BigFloat::parse(x_sci, Radix::Dec, P_ORACLE, AfRm::None, cc);
    let one = BigFloat::from_word(1, P_ORACLE);
    x.add(&one, P_ORACLE, AfRm::None)
        .log10(P_ORACLE, AfRm::None, cc)
}

fn check_log10p1_at(x_str: &str) {
    let x = parse(x_str);
    let exact = format!("{x:e}");
    let mut cc = Consts::new().expect("init consts");
    let oracle = oracle_log10p1(&exact, &mut cc);
    for &rm in MODES {
        let (got, status) = x.log10_1p(rm);
        assert_faithful(
            got,
            status,
            &oracle,
            &mut cc,
            rm,
            &format!("log10_1p({x_str} → {exact})"),
        );
    }
}

// Spot tests --------------------------------------------------------------
//
// The bands the kernel actually branches on: `|x| < 0.5` feeds the
// `log1p` series directly, `|x| ≥ 0.5` forms `1 ⊕ x` and runs the `ln`
// core, and the far tail lets `x` outgrow the working width.

#[test]
fn spot_tiny_positive() {
    check_log10p1_at("1e-30");
}
#[test]
fn spot_tiny_negative() {
    check_log10p1_at("-1e-30");
}
#[test]
fn spot_small_series_band() {
    check_log10p1_at("0.001");
}
#[test]
fn spot_negative_series_band() {
    check_log10p1_at("-0.25");
}
#[test]
fn spot_series_band_edge_below() {
    check_log10p1_at("0.4999999999999999999999999999999999");
}
#[test]
fn spot_ln_band_edge_above() {
    check_log10p1_at("0.5");
}
#[test]
fn spot_negative_ln_band() {
    check_log10p1_at("-0.75");
}
#[test]
fn spot_near_the_domain_edge() {
    check_log10p1_at("-0.9999999999999999999999999999999998");
}
#[test]
fn spot_one() {
    check_log10p1_at("1");
}
#[test]
fn spot_e_minus_one() {
    check_log10p1_at("1.718281828459045235360287471352662");
}
#[test]
fn spot_beside_a_nines_integer() {
    check_log10p1_at("9.000000000000000000000000000000001");
    check_log10p1_at("8.999999999999999999999999999999999");
}
#[test]
fn spot_large() {
    check_log10p1_at("1e40");
}
#[test]
fn spot_past_the_working_width() {
    // Above ~10^49 the working sum absorbs the 1 with a rounding; the
    // budget prices that, and the ladder catches whatever it cannot
    // resolve.
    check_log10p1_at("1.234567890123456789012345678901234e60");
}

// Property sweep ----------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// Positive inputs across the whole branch structure.
    #[test]
    fn log10p1_positive_random_faithful(
        coef_bits in 1u128..=u128::MAX,
        exp in -50i32..=50,
    ) {
        let coef = coef_bits % (10u128.pow(34));
        if coef == 0 { return Ok(()); }
        let x = parse(&format!("{coef}e{exp}"));
        let exact = format!("{x:e}");
        let mut cc = Consts::new().expect("init consts");
        let oracle = oracle_log10p1(&exact, &mut cc);
        for &rm in MODES {
            let (got, status) = x.log10_1p(rm);
            assert_faithful(
                got,
                status,
                &oracle,
                &mut cc,
                rm,
                &format!("log10_1p({exact})"),
            );
        }
    }

    /// Negative inputs strictly inside the domain. The exponent range
    /// keeps `|x| < 1`; anything at or below `−1` is a domain error
    /// and belongs to `transcend_exact_log10p1.rs`, not here.
    #[test]
    fn log10p1_negative_random_faithful(
        coef_bits in 1u128..=u128::MAX,
        digits in 1u32..=34,
    ) {
        let coef = coef_bits % (10u128.pow(digits));
        if coef == 0 { return Ok(()); }
        // `coef` has at most `digits` digits, so `coef · 10^-digits`
        // lies in `(0, 1)` and the negation stays above `−1`.
        let x = parse(&format!("-{coef}e-{digits}"));
        let exact = format!("{x:e}");
        let mut cc = Consts::new().expect("init consts");
        let oracle = oracle_log10p1(&exact, &mut cc);
        for &rm in MODES {
            let (got, status) = x.log10_1p(rm);
            assert_faithful(
                got,
                status,
                &oracle,
                &mut cc,
                rm,
                &format!("log10_1p({exact})"),
            );
        }
    }
}
