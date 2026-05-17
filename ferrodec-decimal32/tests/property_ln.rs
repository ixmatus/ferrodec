//! Faithful-rounding contract for `Decimal32::ln` vs astro-float,
//! asserted for every IEEE 754 rounding direction (ADR-0021,
//! IEEE 754-2019 §9.2). See `tests/common/mod.rs`; this is not a
//! `± ULP` tolerance envelope.
//!
//! `Decimal32` exposes `ln` only (no `log10` on the 32-bit surface),
//! so this file covers `ln` across the full positive `Decimal32`
//! range: down to `~10^-101` (subnormal) and up to `~10^+96`, the
//! wider-than-`f64` interval the fd-r0l pilot unlocked by removing the
//! `libm::log` detour.

#![cfg(feature = "exp-log")]

use ferrodec_test_support::transcend_oracle::{oracle, Consts};
use proptest::prelude::*;

mod common;
use common::{assert_faithful, parse, MODES};

fn check_ln_at(x_str: &str) {
    let x = parse(x_str);
    let exact = format!("{x:e}");
    let mut cc = Consts::new().expect("init consts");
    let oracle = oracle::ln(&exact, &mut cc);
    for &rm in MODES {
        let (got, status) = x.ln(rm);
        assert_faithful(
            got,
            status,
            &oracle,
            &mut cc,
            rm,
            &format!("ln({x_str} → {exact})"),
        );
    }
}

fn check_log2_at(x_str: &str) {
    let x = parse(x_str);
    let exact = format!("{x:e}");
    let mut cc = Consts::new().expect("init consts");
    let oracle = oracle::log2(&exact, &mut cc);
    for &rm in MODES {
        let (got, status) = x.log2(rm);
        assert_faithful(
            got,
            status,
            &oracle,
            &mut cc,
            rm,
            &format!("log2({x_str} → {exact})"),
        );
    }
}

fn check_log10_at(x_str: &str) {
    let x = parse(x_str);
    let exact = format!("{x:e}");
    let mut cc = Consts::new().expect("init consts");
    let oracle = oracle::log10(&exact, &mut cc);
    for &rm in MODES {
        let (got, status) = x.log10(rm);
        assert_faithful(
            got,
            status,
            &oracle,
            &mut cc,
            rm,
            &format!("log10({x_str} → {exact})"),
        );
    }
}

// Spot tests --------------------------------------------------------------

#[test]
fn spot_ln_one() {
    check_ln_at("1");
}
#[test]
fn spot_ln_two() {
    check_ln_at("2");
}
#[test]
fn spot_ln_e() {
    check_ln_at("2.718282");
}
#[test]
fn spot_ln_ten() {
    check_ln_at("10");
}
#[test]
fn spot_ln_hundred() {
    check_ln_at("100");
}
#[test]
fn spot_ln_pi() {
    check_ln_at("3.141593");
}
#[test]
fn spot_ln_half() {
    check_ln_at("0.5");
}
#[test]
fn spot_ln_tiny() {
    check_ln_at("1e-30");
}
#[test]
fn spot_ln_subnormal() {
    // Near the bottom of the `Decimal32` positive range.
    check_ln_at("1e-95");
}
#[test]
fn spot_ln_huge() {
    // Near the top of the `Decimal32` positive range; the pre-fd-r0l
    // `f64` detour could represent the argument but capped the
    // achievable precision below Decimal32's 7 digits.
    check_ln_at("1e90");
}
#[test]
fn spot_ln_near_max() {
    check_ln_at("9.999999e96");
}
#[test]
fn spot_ln_near_one_above() {
    check_ln_at("1.0000001");
}
#[test]
fn spot_ln_near_one_below() {
    check_ln_at("0.9999999");
}
#[test]
fn spot_ln_random_finite() {
    check_ln_at("123.4568");
}

// log2 spot tests ---------------------------------------------------------

#[test]
fn spot_log2_one() {
    check_log2_at("1");
}
#[test]
fn spot_log2_two() {
    check_log2_at("2");
}
#[test]
fn spot_log2_eight() {
    check_log2_at("8");
}
#[test]
fn spot_log2_e() {
    check_log2_at("2.718282");
}
#[test]
fn spot_log2_half() {
    check_log2_at("0.5");
}
#[test]
fn spot_log2_tiny() {
    check_log2_at("1e-30");
}
#[test]
fn spot_log2_subnormal() {
    check_log2_at("1e-95");
}
#[test]
fn spot_log2_huge() {
    check_log2_at("1e90");
}
#[test]
fn spot_log2_near_max() {
    check_log2_at("9.999999e96");
}
#[test]
fn spot_log2_random_finite() {
    check_log2_at("123.4568");
}

// log10 spot tests --------------------------------------------------------

#[test]
fn spot_log10_one() {
    check_log10_at("1");
}
#[test]
fn spot_log10_ten() {
    check_log10_at("10");
}
#[test]
fn spot_log10_powers() {
    for p in [1i32, 2, 3, 5, 10, -1, -3, 90, -90] {
        check_log10_at(&format!("1e{p}"));
    }
}
#[test]
fn spot_log10_e() {
    check_log10_at("2.718282");
}
#[test]
fn spot_log10_half() {
    check_log10_at("0.5");
}
#[test]
fn spot_log10_tiny() {
    check_log10_at("1e-30");
}
#[test]
fn spot_log10_subnormal() {
    check_log10_at("1e-95");
}
#[test]
fn spot_log10_near_max() {
    check_log10_at("9.999999e96");
}
#[test]
fn spot_log10_random_finite() {
    check_log10_at("123.4568");
}

// Property sweep ----------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// `ln` is faithfully rounded across a uniform sweep over the
    /// positive `Decimal32` range, for every rounding direction.
    #[test]
    fn ln_random_faithful(
        coef_bits in 1u64..=u64::MAX,
        exp in -100i32..=90,
    ) {
        // A positive `Decimal32` with a 7-digit-or-less coefficient,
        // scaled across the format's full exponent envelope.
        let coef = coef_bits % (10u64.pow(7));
        if coef == 0 { return Ok(()); }
        let value_str = format!("{coef}e{exp}");
        let x = parse(&value_str);
        // An argument that rounds to +∞ is a special case, not a
        // faithful-rounding input; skip it (the `+∞` semantics are
        // pinned by the sibling unit tests).
        if !x.is_finite() { return Ok(()); }
        let exact = format!("{x:e}");
        let mut cc = Consts::new().expect("init consts");
        let oracle = oracle::ln(&exact, &mut cc);
        for &rm in MODES {
            let (got, status) = x.ln(rm);
            assert_faithful(got, status, &oracle, &mut cc, rm, &format!("ln({exact})"));
        }
    }

    /// `log2` is faithfully rounded across a uniform sweep over the
    /// positive `Decimal32` range, for every rounding direction.
    #[test]
    fn log2_random_faithful(
        coef_bits in 1u64..=u64::MAX,
        exp in -100i32..=90,
    ) {
        let coef = coef_bits % (10u64.pow(7));
        if coef == 0 { return Ok(()); }
        let value_str = format!("{coef}e{exp}");
        let x = parse(&value_str);
        // An argument that rounds to +∞ is a special case, not a
        // faithful-rounding input; skip it (the `+∞` semantics are
        // pinned by the sibling unit tests).
        if !x.is_finite() { return Ok(()); }
        let exact = format!("{x:e}");
        let mut cc = Consts::new().expect("init consts");
        let oracle = oracle::log2(&exact, &mut cc);
        for &rm in MODES {
            let (got, status) = x.log2(rm);
            assert_faithful(got, status, &oracle, &mut cc, rm, &format!("log2({exact})"));
        }
    }

    /// `log10` is faithfully rounded across a uniform sweep over the
    /// positive `Decimal32` range, for every rounding direction.
    #[test]
    fn log10_random_faithful(
        coef_bits in 1u64..=u64::MAX,
        exp in -100i32..=90,
    ) {
        let coef = coef_bits % (10u64.pow(7));
        if coef == 0 { return Ok(()); }
        let value_str = format!("{coef}e{exp}");
        let x = parse(&value_str);
        // See `log2_random_faithful`: skip the `+∞`-rounding corner.
        if !x.is_finite() { return Ok(()); }
        let exact = format!("{x:e}");
        let mut cc = Consts::new().expect("init consts");
        let oracle = oracle::log10(&exact, &mut cc);
        for &rm in MODES {
            let (got, status) = x.log10(rm);
            assert_faithful(got, status, &oracle, &mut cc, rm, &format!("log10({exact})"));
        }
    }
}
