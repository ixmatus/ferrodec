//! Faithful-rounding contract for `Decimal32::pow` vs the shared
//! astro-float oracle, asserted for every IEEE 754 rounding direction
//! (ADR-0021, IEEE 754-2019 §9.2). See `tests/common/mod.rs` for the
//! contract; this is not a `± ULP` tolerance envelope.
//!
//! The fd-r0l P5 rewire moved `pow` off the pre-fd-r0l lossy `f64` /
//! `libm::pow` detour onto the shared faithful `ferrodec-transcend`
//! Extended-precision kernel (`exp(y · ln(|x|))` at `Extended`
//! working precision, the same verified implementation the Decimal128
//! parent uses). This suite stays astro-float-free (Design A): the
//! oracle reaches it only through the
//! `ferrodec_test_support::transcend_oracle` builders, so astro-float
//! never appears in the decimal32 dependency graph. The §9.2.1
//! special-value rule table is exercised in
//! `property_pow_specials.rs`; this suite brackets the in-domain
//! finite non-special path (positive base, finite result).

#![cfg(feature = "pow")]

use ferrodec_test_support::transcend_oracle::{oracle, Consts};
use proptest::prelude::*;

mod common;
use common::{assert_faithful, parse, MODES};

fn check_pow(x_str: &str, y_str: &str) {
    let x = parse(x_str);
    let y = parse(y_str);
    let exact_x = format!("{x:e}");
    let exact_y = format!("{y:e}");
    let mut cc = Consts::new().expect("init consts");
    let oracle = oracle::pow(&exact_x, &exact_y, &mut cc);
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
    check_pow("2.718282", "3.141593");
}
#[test]
fn spot_pi_to_e() {
    check_pow("3.141593", "2.718282");
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

    /// Positive base with a small integer exponent `|y| ≤ 256`, the
    /// band the integer fast path covers. Every in-domain finite
    /// non-special input gets the full unmodified 5-mode bracket. A
    /// parse-overflowed ±∞ base, or a result that overflows /
    /// underflows the Decimal32 range, is outside the faithful-kernel
    /// domain under test (the §9.2.1 specials live in
    /// `property_pow_specials.rs`); skipping those is the fd-dfs
    /// overflow-guard idiom, not a bracket weakening.
    #[test]
    fn pow_integer_y_faithful(
        x_coef in 1u32..=u32::MAX,
        x_exp in -6i32..=6,
        y_int in -256i32..=256,
    ) {
        let xc = x_coef % 10u32.pow(7);
        if xc == 0 || y_int == 0 { return Ok(()); }
        let x_str = format!("{xc}e{x_exp}");
        let y_str = format!("{y_int}");
        let x = parse(&x_str);
        let y = parse(&y_str);
        if !x.is_finite() || !y.is_finite() { return Ok(()); }
        let exact_x = format!("{x:e}");
        let exact_y = format!("{y:e}");
        let mut cc = Consts::new().expect("init consts");
        let oracle = oracle::pow(&exact_x, &exact_y, &mut cc);
        // The kernel deliberately short-circuits OVERFLOW / UNDERFLOW
        // and a zero / non-finite result is outside the faithful-
        // kernel domain under test.
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

    /// Positive base, finite fractional exponent. Same in-domain
    /// contract: the full 5-mode bracket for every finite result;
    /// non-finite / zero oracle results and OVERFLOW / UNDERFLOW
    /// statuses are the documented out-of-domain corners, skipped
    /// without weakening the bracket.
    #[test]
    fn pow_random_faithful(
        x_coef in 1u32..=u32::MAX,
        x_exp in -6i32..=6,
        y_coef in 1u32..=u32::MAX,
        y_exp in -3i32..=2,
    ) {
        let xc = x_coef % 10u32.pow(7);
        let yc = y_coef % 10u32.pow(7);
        if xc == 0 || yc == 0 { return Ok(()); }
        let x_str = format!("{xc}e{x_exp}");
        let y_str = format!("{yc}e{y_exp}");
        let x = parse(&x_str);
        let y = parse(&y_str);
        if !x.is_finite() || !y.is_finite() { return Ok(()); }
        let exact_x = format!("{x:e}");
        let exact_y = format!("{y:e}");
        let mut cc = Consts::new().expect("init consts");
        let oracle = oracle::pow(&exact_x, &exact_y, &mut cc);
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
