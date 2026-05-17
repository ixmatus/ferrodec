//! Faithful-rounding contract for `Decimal128::ln` and `log10` vs
//! astro-float, asserted for every IEEE 754 rounding direction
//! (ADR-0021, IEEE 754-2019 §9.2). See `tests/common/mod.rs`; this is
//! not a `± ULP` tolerance envelope.

#![cfg(feature = "exp-log")]

use astro_float::Consts;
use ferrodec_test_support::transcend_oracle::oracle;
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
fn spot_ln_two() {
    check_ln_at("2");
}
#[test]
fn spot_ln_e() {
    check_ln_at("2.718281828459045235360287471352662");
}
#[test]
fn spot_ln_ten() {
    check_ln_at("10");
}
#[test]
fn spot_ln_pi() {
    check_ln_at("3.14159265358979323846264338327950288");
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
fn spot_ln_huge() {
    check_ln_at("1e6000");
}
#[test]
fn spot_ln_near_one_above() {
    check_ln_at("1.0000000000001");
}
#[test]
fn spot_ln_near_one_below() {
    check_ln_at("0.9999999999999");
}
#[test]
fn spot_ln_random_finite() {
    check_ln_at("123.456789012345");
}

#[test]
fn spot_log10_two() {
    check_log10_at("2");
}
#[test]
fn spot_log10_e() {
    check_log10_at("2.718281828459045235360287471352662");
}
#[test]
fn spot_log10_pi() {
    check_log10_at("3.14159265358979323846264338327950288");
}
#[test]
fn spot_log10_powers() {
    for p in [-100, -10, -1, 1, 10, 100, 1000] {
        check_log10_at(&format!("1e{p}"));
    }
}

// Property sweep ----------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    #[test]
    fn ln_random_faithful(
        coef_bits in 1u128..=u128::MAX,
        exp in -50i32..=50,
    ) {
        // A positive Decimal128 with a 34-digit-or-less coefficient.
        let coef = coef_bits % (10u128.pow(34));
        if coef == 0 { return Ok(()); }
        let value_str = format!("{coef}e{exp}");
        let x = parse(&value_str);
        let exact = format!("{x:e}");
        let mut cc = Consts::new().expect("init consts");
        let oracle = oracle::ln(&exact, &mut cc);
        for &rm in MODES {
            let (got, status) = x.ln(rm);
            assert_faithful(got, status, &oracle, &mut cc, rm, &format!("ln({exact})"));
        }
    }

    #[test]
    fn log10_random_faithful(
        coef_bits in 1u128..=u128::MAX,
        exp in -50i32..=50,
    ) {
        let coef = coef_bits % (10u128.pow(34));
        if coef == 0 { return Ok(()); }
        let value_str = format!("{coef}e{exp}");
        let x = parse(&value_str);
        let exact = format!("{x:e}");
        let mut cc = Consts::new().expect("init consts");
        let oracle = oracle::log10(&exact, &mut cc);
        for &rm in MODES {
            let (got, status) = x.log10(rm);
            assert_faithful(got, status, &oracle, &mut cc, rm, &format!("log10({exact})"));
        }
    }
}
