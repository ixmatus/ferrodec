//! Faithful-rounding contract for `Decimal32` atan / asin / acos /
//! atan2 vs the shared astro-float oracle, asserted for every IEEE
//! 754 rounding direction (ADR-0021, IEEE 754-2019 §9.2). See
//! `tests/common/mod.rs` for the contract; this is not a `± ULP`
//! tolerance envelope.
//!
//! The fd-r0l P3 rewire moved the inverse-trig family off the
//! pre-fd-r0l lossy `f64` / `libm` detour onto the shared faithful
//! `ferrodec-transcend` Extended-precision kernel. This suite stays
//! astro-float-free (Design A): the oracle reaches it only through
//! the `ferrodec_test_support::transcend_oracle` builders, so
//! astro-float never appears in the decimal32 dependency graph.

#![cfg(feature = "trig")]

use ferrodec_decimal32::{Decimal32, RoundingMode, Status};
use ferrodec_test_support::transcend_oracle::{oracle, BigFloat, Consts};
use proptest::prelude::*;

mod common;
use common::{assert_faithful, parse, MODES};

/// Build the shared 256-bit astro-float oracle for the named inverse
/// unary op. Same dispatch shape as the `Decimal128`
/// `property_inverse_trig` suite so the bracket reasons over the same
/// exact values; `BigFloat` is the re-exported oracle type, so
/// decimal32 still names no `astro_float` path of its own.
fn oracle_unary(name: &str, exact: &str, cc: &mut Consts) -> BigFloat {
    match name {
        "atan" => oracle::atan(exact, cc),
        "asin" => oracle::asin(exact, cc),
        "acos" => oracle::acos(exact, cc),
        other => panic!("unknown inverse-unary op {other}"),
    }
}

fn check_unary<F>(name: &str, x_str: &str, ferrodec_op: F)
where
    F: Fn(Decimal32, RoundingMode) -> (Decimal32, Status),
{
    let x = parse(x_str);
    let exact = format!("{x:e}");
    let mut cc = Consts::new().expect("init consts");
    let oracle = oracle_unary(name, &exact, &mut cc);
    for &rm in MODES {
        let (got, status) = ferrodec_op(x, rm);
        assert_faithful(
            got,
            status,
            &oracle,
            &mut cc,
            rm,
            &format!("{name}({exact})"),
        );
    }
}

// atan -------------------------------------------------------------------

#[test]
fn atan_one() {
    check_unary("atan", "1", Decimal32::atan);
}
#[test]
fn atan_two() {
    check_unary("atan", "2", Decimal32::atan);
}
#[test]
fn atan_huge() {
    check_unary("atan", "1e30", Decimal32::atan);
}
#[test]
fn atan_tiny() {
    check_unary("atan", "1e-30", Decimal32::atan);
}
#[test]
fn atan_half() {
    check_unary("atan", "0.5", Decimal32::atan);
}
#[test]
fn atan_pi() {
    check_unary("atan", "3.141593", Decimal32::atan);
}

// asin -------------------------------------------------------------------

#[test]
fn asin_half() {
    check_unary("asin", "0.5", Decimal32::asin);
}
#[test]
fn asin_neg_half() {
    check_unary("asin", "-0.5", Decimal32::asin);
}
#[test]
fn asin_near_one() {
    check_unary("asin", "0.999", Decimal32::asin);
}
#[test]
fn asin_tiny() {
    check_unary("asin", "1e-15", Decimal32::asin);
}

// acos -------------------------------------------------------------------

#[test]
fn acos_half() {
    check_unary("acos", "0.5", Decimal32::acos);
}
#[test]
fn acos_quarter() {
    check_unary("acos", "0.25", Decimal32::acos);
}
#[test]
fn acos_neg_half() {
    check_unary("acos", "-0.5", Decimal32::acos);
}

// atan2 ------------------------------------------------------------------

fn check_atan2(y_str: &str, x_str: &str) {
    let y = parse(y_str);
    let x = parse(x_str);
    let exact_y = format!("{y:e}");
    let exact_x = format!("{x:e}");

    // astro-float has no atan2; the shared builder synthesizes it via
    // atan(y/x) + quadrant, the same construction the Decimal128
    // suite uses. The sign bits come from the parsed Decimal32 values
    // so the quadrant decision matches the format's own sign.
    let mut cc = Consts::new().expect("init consts");
    let oracle = oracle::atan2(
        &exact_y,
        &exact_x,
        y.is_sign_negative(),
        x.is_sign_negative(),
        &mut cc,
    );
    for &rm in MODES {
        let (got, status) = y.atan2(x, rm);
        assert_faithful(
            got,
            status,
            &oracle,
            &mut cc,
            rm,
            &format!("atan2({exact_y}, {exact_x})"),
        );
    }
}

#[test]
fn atan2_one_one() {
    check_atan2("1", "1");
}
#[test]
fn atan2_one_two() {
    check_atan2("1", "2");
}
#[test]
fn atan2_neg_one_neg_two() {
    check_atan2("-1", "-2");
}
#[test]
fn atan2_three_four() {
    check_atan2("3", "4");
}
#[test]
fn atan2_neg_one_one() {
    check_atan2("-1", "1");
}

// Property sweeps ---------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// `atan` faithfully rounded across a wide magnitude sweep, every
    /// rounding direction. `atan` is total on the finite reals, so
    /// the only out-of-domain corner is a parse-overflowed ±∞ input
    /// (`atan(±∞) = ±π/2`, a special result the kernel resolves but
    /// the faithful bracket rightly will not accept); skip it with
    /// the same idiom as `coef == 0`, without weakening the bracket.
    #[test]
    fn atan_random_faithful(
        coef_bits in 1u32..=u32::MAX,
        exp in -20i32..=20,
        sign in any::<bool>(),
    ) {
        let coef = coef_bits % (10u32.pow(7));
        if coef == 0 { return Ok(()); }
        let value_str = format!("{}{}e{}", if sign { "-" } else { "" }, coef, exp);
        let x = parse(&value_str);
        if !x.is_finite() { return Ok(()); }
        let exact = format!("{x:e}");
        let mut cc = Consts::new().expect("init consts");
        let oracle = oracle::atan(&exact, &mut cc);
        for &rm in MODES {
            let (got, status) = x.atan(rm);
            assert_faithful(got, status, &oracle, &mut cc, rm, &format!("atan({exact})"));
        }
    }

    /// `asin` / `acos` faithfully rounded across the in-domain
    /// interval `(-1, +1)`, every rounding direction. The generated
    /// magnitude is mapped strictly inside `(-1, 1)`; an `|x| > 1`
    /// input is the documented domain-INVALID special (NaN +
    /// INVALID), not a faithfully-rounded value, so it is kept out of
    /// the sweep by construction — no bracket weakening, the in-domain
    /// bracket stays the full 5-mode contract.
    #[test]
    fn asin_acos_random_faithful(
        coef_bits in 1u32..=u32::MAX,
        extra_exp in 0i32..=11,
        sign in any::<bool>(),
    ) {
        // `coef ∈ [1, 10^7)` written with exponent `-(7 + extra_exp)`
        // is `coef · 10^-(7+extra_exp)`, always strictly inside the
        // asin/acos `(-1, 1)` domain (a 7-digit coefficient times
        // `10^-7` is `< 1`). `coef == 0` is the trivial zero input;
        // skip it with the established idiom.
        let coef = coef_bits % (10u32.pow(7));
        if coef == 0 { return Ok(()); }
        let value_str = format!("{}{}e{}",
            if sign { "-" } else { "" }, coef, -(7 + extra_exp));
        let x = parse(&value_str);
        // Defensive: the construction keeps |x| < 1, but guard the
        // bracket against any parse corner producing a non-finite or
        // out-of-domain value (it is then the domain-INVALID special,
        // not a faithful finite result).
        if !x.is_finite() { return Ok(()); }
        let abs_x = x.abs();
        if !matches!(
            abs_x.partial_cmp(Decimal32::ONE).0,
            Some(core::cmp::Ordering::Less)
        ) {
            return Ok(());
        }
        let exact = format!("{x:e}");
        let mut cc = Consts::new().expect("init consts");
        let o_asin = oracle::asin(&exact, &mut cc);
        let o_acos = oracle::acos(&exact, &mut cc);
        for &rm in MODES {
            let (got_asin, s_asin) = x.asin(rm);
            let (got_acos, s_acos) = x.acos(rm);
            assert_faithful(got_asin, s_asin, &o_asin, &mut cc, rm, &format!("asin({exact})"));
            assert_faithful(got_acos, s_acos, &o_acos, &mut cc, rm, &format!("acos({exact})"));
        }
    }
}
