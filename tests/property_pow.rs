//! Faithful-rounding cross-check for `Decimal128::pow` vs astro-float.

#![cfg(feature = "pow")]

use astro_float::{BigFloat, Consts, Radix, RoundingMode as AfRm};
use ferrodec::RoundingMode;
use proptest::prelude::*;

mod common;
use common::{bigfloat_to_decimal_string, parse, within_ulps};

fn oracle_pow(x_str: &str, y_str: &str) -> String {
    let p = 220;
    let rm = AfRm::None;
    let mut cc = Consts::new().expect("init consts");
    let x = BigFloat::parse(x_str, Radix::Dec, p, rm, &mut cc);
    let y = BigFloat::parse(y_str, Radix::Dec, p, rm, &mut cc);
    let r = x.pow(&y, p, rm, &mut cc);
    bigfloat_to_decimal_string(&r, &mut cc, 50)
}

fn check_pow(x_str: &str, y_str: &str, ulps: u32) {
    let x = parse(x_str);
    let y = parse(y_str);
    let exact_x = format!("{x}");
    let exact_y = format!("{y}");
    let (got, _) = x.pow(y, RoundingMode::NearestEven);
    let want_str = oracle_pow(&exact_x, &exact_y);
    let want = parse(&want_str);
    assert!(
        within_ulps(got, want, ulps),
        "pow({exact_x}, {exact_y}): got {got:?}, want {want:?} (oracle {want_str})"
    );
}

// Spot tests --------------------------------------------------------------

#[test]
fn spot_two_to_ten() {
    check_pow("2", "10", 1);
}
#[test]
fn spot_e_to_pi() {
    check_pow(
        "2.718281828459045235360287471352662",
        "3.141592653589793238462643383279503",
        1,
    );
}
#[test]
fn spot_pi_to_e() {
    check_pow(
        "3.141592653589793238462643383279503",
        "2.718281828459045235360287471352662",
        1,
    );
}
#[test]
fn spot_half_to_half() {
    check_pow("0.5", "0.5", 1);
}
#[test]
fn spot_two_to_neg_three() {
    check_pow("2", "-3", 1);
}
#[test]
fn spot_ten_to_zero_point_five() {
    check_pow("10", "0.5", 1);
}
#[test]
fn spot_one_point_five_to_one_point_five() {
    check_pow("1.5", "1.5", 1);
}
#[test]
fn spot_small_base_small_exponent() {
    check_pow("0.001", "0.7", 1);
}
#[test]
fn spot_large_base_negative_exponent() {
    check_pow("1000", "-2.5", 1);
}
#[test]
fn spot_close_to_one() {
    check_pow("1.0001", "1000", 1);
}

// Property sweep ----------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// Slice E.1 regression band: positive base with small integer
    /// exponent `|y| ≤ 256`. Pre-1.15 the integer fast path
    /// (`pow_integer_fast_path`) was taken unconditionally for any
    /// |y| in that range and accumulated ~5 ULP via square-and-multiply
    /// at Decimal128 precision (H1 of the 2026-05-10 six-agent
    /// review). The fix routes inexact-fast-path results through the
    /// Extended pipeline; this proptest hits the integer band the
    /// original general sweep below almost never sampled.
    #[test]
    fn pow_integer_y_within_1_ulp(
        x_coef in 1u128..=u128::MAX,
        x_exp in -10i32..=10,
        y_int in -256i32..=256,
    ) {
        let xc = x_coef % 10u128.pow(34);
        if xc == 0 || y_int == 0 { return Ok(()); }
        let x_str = format!("{xc}e{x_exp}");
        let y_str = format!("{y_int}");
        let x = parse(&x_str);
        let y = parse(&y_str);
        let exact_x = format!("{x}");
        let exact_y = format!("{y}");
        let (got, status) = x.pow(y, RoundingMode::NearestEven);
        if status.overflow() || status.underflow() {
            return Ok(());
        }
        let want_str = oracle_pow(&exact_x, &exact_y);
        let want = parse(&want_str);
        if !want.is_finite() || want.is_zero() {
            return Ok(());
        }
        prop_assert!(
            within_ulps(got, want, 1),
            "pow({exact_x}, {exact_y}): got {got:?}, want {want:?} (oracle {want_str})"
        );
    }

    #[test]
    fn pow_random_within_1_ulp(
        x_coef in 1u128..=u128::MAX,
        x_exp in -10i32..=10,
        y_coef in 1u128..=u128::MAX,
        y_exp in -3i32..=2,
    ) {
        // Build a positive `x` and a finite `y`. Avoid extreme exponents
        // where the result blows past Decimal128's range — we're testing
        // accuracy of the kernel, not the OVERFLOW path.
        let xc = x_coef % 10u128.pow(34);
        let yc = y_coef % 10u128.pow(34);
        if xc == 0 || yc == 0 { return Ok(()); }
        let x_str = format!("{xc}e{x_exp}");
        let y_str = format!("{yc}e{y_exp}");
        let x = parse(&x_str);
        let y = parse(&y_str);
        // Skip cases where the result would over/underflow — the
        // oracle's astro-float at 220 bits can't always parse the result.
        let exact_x = format!("{x}");
        let exact_y = format!("{y}");
        let (got, status) = x.pow(y, RoundingMode::NearestEven);
        if status.overflow() || status.underflow() {
            return Ok(());
        }
        let want_str = oracle_pow(&exact_x, &exact_y);
        let want = parse(&want_str);
        if !want.is_finite() || want.is_zero() {
            return Ok(());
        }
        prop_assert!(
            within_ulps(got, want, 1),
            "pow({exact_x}, {exact_y}): got {got:?}, want {want:?} (oracle {want_str})"
        );
    }
}
