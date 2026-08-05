//! Faithful-rounding contract for `Decimal128::exp10_m1` (IEEE
//! 754-2019 §9.2 `exp10m1`) against astro-float, asserted for every
//! rounding direction (ADR-0021's floor under ADR-0059's correctly
//! rounded claim). See `tests/common/mod.rs`; this is not a `± ULP`
//! tolerance envelope.
//!
//! The oracle is local rather than one of the shared
//! `transcend_oracle::oracle` builders because `exp10m1` needs its
//! subtraction carried *inside* the oracle: `10^x − 1` at the shared
//! 256 bit width keeps only about 77 decimal digits, which a tiny `x`
//! would spend entirely on the leading 1 of `10^x`. Forming the
//! difference at 1024 bits (about 308 decimal digits) leaves a
//! `10^-50` argument more than 250 digits of headroom, far beyond the
//! 34 digit bracket under test. The kernel itself never forms
//! `10^x ⊖ 1` at format width; that is the whole point of the
//! operation.
//!
//! The integer arguments are deliberately absent from the sweep:
//! they are delivered by the input-side classifier, not by the
//! kernel, and `tests/transcend_exact_exp10m1.rs` walks that family
//! exhaustively against literal expectations.

#![cfg(feature = "exp-log")]

use astro_float::{BigFloat, Consts, Radix, RoundingMode as AfRm};
use proptest::prelude::*;

mod common;
use common::{assert_faithful, parse, MODES};

/// Oracle working precision in bits (about 308 decimal digits). Wide
/// enough that `10^x − 1` keeps every digit of a `10^-50` argument.
const P_ORACLE: usize = 1024;

/// `10^x − 1` of the exact value `x_sci`, formed at oracle width.
fn oracle_exp10m1(x_sci: &str, cc: &mut Consts) -> BigFloat {
    let x = BigFloat::parse(x_sci, Radix::Dec, P_ORACLE, AfRm::None, cc);
    let ten = BigFloat::from_word(10, P_ORACLE);
    let one = BigFloat::from_word(1, P_ORACLE);
    ten.pow(&x, P_ORACLE, AfRm::None, cc)
        .sub(&one, P_ORACLE, AfRm::None)
}

fn check_exp10m1_at(x_str: &str) {
    let x = parse(x_str);
    let exact = format!("{x:e}");
    let mut cc = Consts::new().expect("init consts");
    let oracle = oracle_exp10m1(&exact, &mut cc);
    for &rm in MODES {
        let (got, status) = x.exp10_m1(rm);
        assert_faithful(
            got,
            status,
            &oracle,
            &mut cc,
            rm,
            &format!("exp10_m1({x_str} → {exact})"),
        );
    }
}

// Spot tests --------------------------------------------------------------
//
// The bands the kernel actually branches on: `|x · ln 10| ≤ 1.1513`
// (i.e. `|x| ≤ 0.5`) runs the direct `expm1` series, everything above
// runs the `exp` pipeline and subtracts 1, and far below zero the
// working value collapses onto `−1` and the ADR-0051 seam decides.

#[test]
fn spot_tiny_positive() {
    check_exp10m1_at("1e-30");
}
#[test]
fn spot_tiny_negative() {
    check_exp10m1_at("-1e-30");
}
#[test]
fn spot_series_band() {
    check_exp10m1_at("0.001");
}
#[test]
fn spot_negative_series_band() {
    check_exp10m1_at("-0.25");
}
#[test]
fn spot_series_band_edge_below() {
    check_exp10m1_at("0.4999999999999999999999999999999999");
}
#[test]
fn spot_exp_pipeline_band_edge_above() {
    check_exp10m1_at("0.5");
}
#[test]
fn spot_negative_exp_pipeline_band() {
    check_exp10m1_at("-0.75");
}
#[test]
fn spot_beside_an_integer() {
    // One quantum either side of `x = 2`, whose own value (99) is the
    // classifier's; these two are irrational and the kernel's.
    check_exp10m1_at("2.000000000000000000000000000000001");
    check_exp10m1_at("1.999999999999999999999999999999999");
}
#[test]
fn spot_mid_range() {
    check_exp10m1_at("37.5");
    check_exp10m1_at("-37.5");
}
#[test]
fn spot_above_the_collapse_threshold() {
    // The working subtraction still resolves here (`10^-45.5` is above
    // the ~`10^-47` snap band), so this is the kernel's own answer
    // beside the seam's.
    check_exp10m1_at("-45.5");
}
#[test]
fn spot_inside_the_collapse_band() {
    // The working value rounds to exactly `−1`; the ADR-0051 residual
    // seam decides the direction from `10^x − 1 > −1`.
    check_exp10m1_at("-50.5");
}
#[test]
fn spot_large_positive() {
    check_exp10m1_at("300.25");
}
#[test]
fn spot_near_the_top_decade() {
    // Inside the representable range (`10^6144.5 ≈ 3.16e6144 < MAX`),
    // where the subtraction of 1 is far below the working resolution.
    check_exp10m1_at("6144.5");
}

// Property sweep ----------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// Positive arguments across the branch structure. The exponent
    /// range keeps `|x| < 100`, so the result stays finite (no §7.4
    /// saturation, which the exact-family gate owns) while spanning
    /// fifty decades of argument magnitude.
    #[test]
    fn exp10m1_positive_random_faithful(
        coef_bits in 1u128..=u128::MAX,
        exp in -50i32..=-32,
    ) {
        let coef = coef_bits % (10u128.pow(34));
        if coef == 0 { return Ok(()); }
        let x = parse(&format!("{coef}e{exp}"));
        let exact = format!("{x:e}");
        let mut cc = Consts::new().expect("init consts");
        let oracle = oracle_exp10m1(&exact, &mut cc);
        for &rm in MODES {
            let (got, status) = x.exp10_m1(rm);
            assert_faithful(
                got,
                status,
                &oracle,
                &mut cc,
                rm,
                &format!("exp10_m1({exact})"),
            );
        }
    }

    /// Negative arguments over the same span. `|x| < 100` keeps the
    /// true value clear of the `−1` gate's window (`x · ln 10 < −120`
    /// needs `|x| > 52.1`) for most draws and inside it for the rest,
    /// so both the kernel's band and the anchor seam are exercised.
    #[test]
    fn exp10m1_negative_random_faithful(
        coef_bits in 1u128..=u128::MAX,
        exp in -50i32..=-32,
    ) {
        let coef = coef_bits % (10u128.pow(34));
        if coef == 0 { return Ok(()); }
        let x = parse(&format!("-{coef}e{exp}"));
        let exact = format!("{x:e}");
        let mut cc = Consts::new().expect("init consts");
        let oracle = oracle_exp10m1(&exact, &mut cc);
        for &rm in MODES {
            let (got, status) = x.exp10_m1(rm);
            assert_faithful(
                got,
                status,
                &oracle,
                &mut cc,
                rm,
                &format!("exp10_m1({exact})"),
            );
        }
    }
}
