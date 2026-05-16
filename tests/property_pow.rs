//! Faithful-rounding contract for `Decimal128::pow` vs astro-float,
//! asserted for every IEEE 754 rounding direction (ADR-0021, IEEE
//! 754-2019 §9.2). See `tests/common/mod.rs`; this is not a `± ULP`
//! tolerance envelope.

#![cfg(feature = "pow")]

use astro_float::{BigFloat, Consts, Radix, RoundingMode as AfRm};
use proptest::prelude::*;

mod common;
use common::{assert_faithful, parse, MODES};

/// Working precision for the astro-float oracle: 256 bits ≈ 77 decimal
/// digits.
const P: usize = 256;

fn oracle_pow(x_str: &str, y_str: &str, cc: &mut Consts) -> BigFloat {
    let x = BigFloat::parse(x_str, Radix::Dec, P, AfRm::None, cc);
    let y = BigFloat::parse(y_str, Radix::Dec, P, AfRm::None, cc);
    x.pow(&y, P, AfRm::None, cc)
}

fn check_pow(x_str: &str, y_str: &str) {
    let x = parse(x_str);
    let y = parse(y_str);
    let exact_x = format!("{x:e}");
    let exact_y = format!("{y:e}");
    let mut cc = Consts::new().expect("init consts");
    let oracle = oracle_pow(&exact_x, &exact_y, &mut cc);
    for &rm in MODES {
        let (got, status) = x.pow(y, rm);
        assert_faithful(
            got,
            status,
            &oracle,
            &mut cc,
            rm,
            &format!("pow({exact_x}, {exact_y})"),
        );
    }
}

// Spot tests --------------------------------------------------------------

#[test]
fn spot_two_to_ten() {
    check_pow("2", "10");
}
#[test]
fn spot_e_to_pi() {
    check_pow(
        "2.718281828459045235360287471352662",
        "3.141592653589793238462643383279503",
    );
}
#[test]
fn spot_pi_to_e() {
    check_pow(
        "3.141592653589793238462643383279503",
        "2.718281828459045235360287471352662",
    );
}
#[test]
fn spot_half_to_half() {
    check_pow("0.5", "0.5");
}
#[test]
fn spot_two_to_neg_three() {
    check_pow("2", "-3");
}
#[test]
fn spot_ten_to_zero_point_five() {
    check_pow("10", "0.5");
}
#[test]
fn spot_one_point_five_to_one_point_five() {
    check_pow("1.5", "1.5");
}
#[test]
fn spot_small_base_small_exponent() {
    check_pow("0.001", "0.7");
}
#[test]
fn spot_large_base_negative_exponent() {
    check_pow("1000", "-2.5");
}
#[test]
fn spot_close_to_one() {
    check_pow("1.0001", "1000");
}

// Property sweep ----------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// Slice E.1 regression band: positive base with small integer
    /// exponent `|y| ≤ 256`. Pre-1.15 the integer fast path
    /// (`pow_integer_fast_path`) was taken unconditionally for any
    /// |y| in that range and accumulated ~5 ULP via square-and-multiply
    /// at Decimal128 precision (H1 of the 2026-05-10 six-agent review).
    /// The fix routes inexact-fast-path results through the Extended
    /// pipeline; this proptest hits the integer band the original
    /// general sweep below almost never sampled.
    #[test]
    fn pow_integer_y_faithful(
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
        let exact_x = format!("{x:e}");
        let exact_y = format!("{y:e}");
        let mut cc = Consts::new().expect("init consts");
        let oracle = oracle_pow(&exact_x, &exact_y, &mut cc);
        // The kernel deliberately short-circuits OVERFLOW / UNDERFLOW
        // and a zero/non-finite result is outside the faithful-kernel
        // domain under test.
        if oracle.is_inf() || oracle.is_nan() || oracle.is_zero() {
            return Ok(());
        }
        for &rm in MODES {
            let (got, status) = x.pow(y, rm);
            if status.overflow() || status.underflow() { continue; }
            assert_faithful(
                got, status, &oracle, &mut cc, rm,
                &format!("pow({exact_x}, {exact_y})"),
            );
        }
    }

    #[test]
    fn pow_random_faithful(
        x_coef in 1u128..=u128::MAX,
        x_exp in -10i32..=10,
        y_coef in 1u128..=u128::MAX,
        y_exp in -3i32..=2,
    ) {
        // A positive `x` and a finite `y`. Extreme exponents that blow
        // past Decimal128's range exercise OVERFLOW, not kernel
        // accuracy, and are filtered below.
        let xc = x_coef % 10u128.pow(34);
        let yc = y_coef % 10u128.pow(34);
        if xc == 0 || yc == 0 { return Ok(()); }
        let x_str = format!("{xc}e{x_exp}");
        let y_str = format!("{yc}e{y_exp}");
        let x = parse(&x_str);
        let y = parse(&y_str);
        let exact_x = format!("{x:e}");
        let exact_y = format!("{y:e}");
        let mut cc = Consts::new().expect("init consts");
        let oracle = oracle_pow(&exact_x, &exact_y, &mut cc);
        if oracle.is_inf() || oracle.is_nan() || oracle.is_zero() {
            return Ok(());
        }
        for &rm in MODES {
            let (got, status) = x.pow(y, rm);
            if status.overflow() || status.underflow() { continue; }
            assert_faithful(
                got, status, &oracle, &mut cc, rm,
                &format!("pow({exact_x}, {exact_y})"),
            );
        }
    }
}
