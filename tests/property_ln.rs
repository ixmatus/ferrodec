//! Faithful-rounding cross-check for `Decimal128::ln` and `log10` vs astro-float.

#![cfg(feature = "exp-log")]

use astro_float::{BigFloat, Consts, Radix, RoundingMode as AfRm};
use ferrodec::RoundingMode;
use proptest::prelude::*;

mod common;
use common::{bigfloat_to_decimal_string, parse, within_ulps};

fn oracle_ln(x_str: &str) -> String {
    oracle_apply(x_str, astro_float::BigFloat::ln)
}

fn oracle_log10(x_str: &str) -> String {
    oracle_apply(x_str, astro_float::BigFloat::log10)
}

fn oracle_apply<F>(x_str: &str, f: F) -> String
where
    F: FnOnce(&BigFloat, usize, AfRm, &mut Consts) -> BigFloat,
{
    let p = 220; // ~66 decimal digits — well above 50-digit ext + 34 final
    let rm = AfRm::None;
    let mut cc = Consts::new().expect("init consts");
    let x = BigFloat::parse(x_str, Radix::Dec, p, rm, &mut cc);
    let r = f(&x, p, rm, &mut cc);
    bigfloat_to_decimal_string(&r, &mut cc, 50)
}

fn check_ln_at(x_str: &str, ulps: u32) {
    let x = parse(x_str);
    let exact_str = format!("{x}");
    let (got, _) = x.ln(RoundingMode::NearestEven);
    let want_str = oracle_ln(&exact_str);
    let want = parse(&want_str);
    assert!(
        within_ulps(got, want, ulps),
        "ln({x_str} → {exact_str}): got {got:?}, want {want:?} (oracle {want_str})"
    );
}

fn check_log10_at(x_str: &str, ulps: u32) {
    let x = parse(x_str);
    let exact_str = format!("{x}");
    let (got, _) = x.log10(RoundingMode::NearestEven);
    let want_str = oracle_log10(&exact_str);
    let want = parse(&want_str);
    assert!(
        within_ulps(got, want, ulps),
        "log10({x_str} → {exact_str}): got {got:?}, want {want:?} (oracle {want_str})"
    );
}

// Spot tests --------------------------------------------------------------

#[test]
fn spot_ln_two() {
    check_ln_at("2", 1);
}
#[test]
fn spot_ln_e() {
    check_ln_at("2.718281828459045235360287471352662", 1);
}
#[test]
fn spot_ln_ten() {
    check_ln_at("10", 1);
}
#[test]
fn spot_ln_pi() {
    check_ln_at("3.14159265358979323846264338327950288", 1);
}
#[test]
fn spot_ln_half() {
    check_ln_at("0.5", 1);
}
#[test]
fn spot_ln_tiny() {
    check_ln_at("1e-30", 1);
}
#[test]
fn spot_ln_huge() {
    check_ln_at("1e6000", 1);
}
#[test]
fn spot_ln_near_one_above() {
    check_ln_at("1.0000000000001", 1);
}
#[test]
fn spot_ln_near_one_below() {
    check_ln_at("0.9999999999999", 1);
}
#[test]
fn spot_ln_random_finite() {
    check_ln_at("123.456789012345", 1);
}

#[test]
fn spot_log10_two() {
    check_log10_at("2", 1);
}
#[test]
fn spot_log10_e() {
    check_log10_at("2.718281828459045235360287471352662", 1);
}
#[test]
fn spot_log10_pi() {
    check_log10_at("3.14159265358979323846264338327950288", 1);
}
#[test]
fn spot_log10_powers() {
    for p in [-100, -10, -1, 1, 10, 100, 1000] {
        check_log10_at(&format!("1e{p}"), 1);
    }
}

// Property sweep ----------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    #[test]
    fn ln_random_within_1_ulp(
        coef_bits in 1u128..=u128::MAX,
        exp in -50i32..=50,
    ) {
        // Build a positive Decimal128 with a 34-digit-or-less coefficient.
        let coef = coef_bits % (10u128.pow(34));
        if coef == 0 { return Ok(()); }
        let value_str = format!("{coef}e{exp}");
        let x = parse(&value_str);
        let exact_str = format!("{x}");
        let (got, _) = x.ln(RoundingMode::NearestEven);
        let want_str = oracle_ln(&exact_str);
        let want = parse(&want_str);
        prop_assert!(
            within_ulps(got, want, 1),
            "ln({exact_str}): got {got:?}, want {want:?} (oracle {want_str})"
        );
    }

    #[test]
    fn log10_random_within_1_ulp(
        coef_bits in 1u128..=u128::MAX,
        exp in -50i32..=50,
    ) {
        let coef = coef_bits % (10u128.pow(34));
        if coef == 0 { return Ok(()); }
        let value_str = format!("{coef}e{exp}");
        let x = parse(&value_str);
        let exact_str = format!("{x}");
        let (got, _) = x.log10(RoundingMode::NearestEven);
        let want_str = oracle_log10(&exact_str);
        let want = parse(&want_str);
        prop_assert!(
            within_ulps(got, want, 1),
            "log10({exact_str}): got {got:?}, want {want:?} (oracle {want_str})"
        );
    }
}
