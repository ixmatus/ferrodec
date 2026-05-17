//! Faithful-rounding contract for `Decimal32` sinh / cosh / tanh /
//! asinh / acosh / atanh vs the shared astro-float oracle, asserted
//! for every IEEE 754 rounding direction (ADR-0021, IEEE 754-2019
//! §9.2). See `tests/common/mod.rs` for the contract; this is not a
//! `± ULP` tolerance envelope.
//!
//! The fd-r0l P4 rewire moved the hyperbolic family off the
//! pre-fd-r0l lossy `f64` / `libm` detour onto the shared faithful
//! `ferrodec-transcend` Extended-precision kernel (built on the
//! already-faithful `exp` / `ln` primitives). This suite stays
//! astro-float-free (Design A): the oracle reaches it only through
//! the `ferrodec_test_support::transcend_oracle` builders, so
//! astro-float never appears in the decimal32 dependency graph.

#![cfg(feature = "hyperbolic")]

use ferrodec_decimal32::{Decimal32, RoundingMode, Status};
use ferrodec_test_support::transcend_oracle::{oracle, BigFloat, Consts};
use proptest::prelude::*;

mod common;
use common::{assert_faithful, parse, MODES};

/// Build the shared 256-bit astro-float oracle for the named
/// hyperbolic unary op. Same dispatch shape as the `Decimal128`
/// `property_hyperbolic` suite so the bracket reasons over the same
/// exact values; `BigFloat` is the re-exported oracle type, so
/// decimal32 still names no `astro_float` path of its own.
fn oracle_unary(name: &str, exact: &str, cc: &mut Consts) -> BigFloat {
    match name {
        "sinh" => oracle::sinh(exact, cc),
        "cosh" => oracle::cosh(exact, cc),
        "tanh" => oracle::tanh(exact, cc),
        "asinh" => oracle::asinh(exact, cc),
        "acosh" => oracle::acosh(exact, cc),
        "atanh" => oracle::atanh(exact, cc),
        other => panic!("unknown hyperbolic-unary op {other}"),
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

// sinh -------------------------------------------------------------------

#[test]
fn sinh_one() {
    check_unary("sinh", "1", Decimal32::sinh);
}
#[test]
fn sinh_two() {
    check_unary("sinh", "2", Decimal32::sinh);
}
#[test]
fn sinh_tiny() {
    check_unary("sinh", "0.001", Decimal32::sinh);
}
#[test]
fn sinh_neg() {
    check_unary("sinh", "-1.5", Decimal32::sinh);
}

// cosh -------------------------------------------------------------------

#[test]
fn cosh_one() {
    check_unary("cosh", "1", Decimal32::cosh);
}
#[test]
fn cosh_two() {
    check_unary("cosh", "2", Decimal32::cosh);
}
#[test]
fn cosh_tiny() {
    check_unary("cosh", "0.001", Decimal32::cosh);
}

// tanh -------------------------------------------------------------------

#[test]
fn tanh_half() {
    check_unary("tanh", "0.5", Decimal32::tanh);
}
#[test]
fn tanh_one() {
    check_unary("tanh", "1", Decimal32::tanh);
}
#[test]
fn tanh_three() {
    check_unary("tanh", "3", Decimal32::tanh);
}

// asinh ------------------------------------------------------------------

#[test]
fn asinh_one() {
    check_unary("asinh", "1", Decimal32::asinh);
}
#[test]
fn asinh_huge() {
    check_unary("asinh", "1e30", Decimal32::asinh);
}
#[test]
fn asinh_tiny() {
    check_unary("asinh", "1e-15", Decimal32::asinh);
}

// acosh ------------------------------------------------------------------

#[test]
fn acosh_two() {
    check_unary("acosh", "2", Decimal32::acosh);
}
#[test]
fn acosh_huge() {
    check_unary("acosh", "1e30", Decimal32::acosh);
}

// atanh ------------------------------------------------------------------

#[test]
fn atanh_half() {
    check_unary("atanh", "0.5", Decimal32::atanh);
}
#[test]
fn atanh_quarter() {
    check_unary("atanh", "0.25", Decimal32::atanh);
}
#[test]
fn atanh_neg_three_quarter() {
    check_unary("atanh", "-0.75", Decimal32::atanh);
}

// Property sweeps ---------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// `sinh` / `cosh` / `tanh` faithfully rounded across
    /// moderate-magnitude inputs, every rounding direction. `sinh` /
    /// `cosh` grow as `±eˣ / 2`, so a large `|x|` overflows to ±∞ — a
    /// special-case result, not a faithfully-rounded finite value;
    /// skip the out-of-domain corner when the probed result is
    /// non-finite (the same idiom as the `coef == 0` skip and the
    /// `property_exp` overflow guard), without weakening the bracket.
    /// The overflow gate is rounding-mode-independent, so probing
    /// `MODES[0]` is representative; `exp ≤ 2` keeps the magnitude
    /// inside the faithful window for all but the rare overflow seed.
    #[test]
    fn sinh_cosh_tanh_random_faithful(
        coef_bits in 1u32..=u32::MAX,
        exp in -20i32..=2,
        sign in any::<bool>(),
    ) {
        let coef = coef_bits % (10u32.pow(7));
        if coef == 0 { return Ok(()); }
        let value_str = format!("{}{}e{}", if sign { "-" } else { "" }, coef, exp);
        let x = parse(&value_str);
        if !x.is_finite() { return Ok(()); }
        let (probe, _) = x.sinh(MODES[0]);
        if !probe.is_finite() { return Ok(()); }
        let (probe_c, _) = x.cosh(MODES[0]);
        if !probe_c.is_finite() { return Ok(()); }
        let exact = format!("{x:e}");
        let mut cc = Consts::new().expect("init consts");
        let o_sinh = oracle::sinh(&exact, &mut cc);
        let o_cosh = oracle::cosh(&exact, &mut cc);
        let o_tanh = oracle::tanh(&exact, &mut cc);
        for &rm in MODES {
            let (g_sinh, s_sinh) = x.sinh(rm);
            let (g_cosh, s_cosh) = x.cosh(rm);
            let (g_tanh, s_tanh) = x.tanh(rm);
            assert_faithful(g_sinh, s_sinh, &o_sinh, &mut cc, rm, &format!("sinh({exact})"));
            assert_faithful(g_cosh, s_cosh, &o_cosh, &mut cc, rm, &format!("cosh({exact})"));
            assert_faithful(g_tanh, s_tanh, &o_tanh, &mut cc, rm, &format!("tanh({exact})"));
        }
    }

    /// `asinh` faithfully rounded across a wide magnitude sweep,
    /// every rounding direction. `asinh` is total on the finite
    /// reals, so the only out-of-domain corner is a parse-overflowed
    /// ±∞ input (`asinh(±∞) = ±∞`, a special result the kernel
    /// resolves but the faithful bracket rightly will not accept);
    /// skip it with the same idiom as `coef == 0`, without weakening
    /// the bracket.
    #[test]
    fn asinh_random_faithful(
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
        let oracle = oracle::asinh(&exact, &mut cc);
        for &rm in MODES {
            let (got, status) = x.asinh(rm);
            assert_faithful(got, status, &oracle, &mut cc, rm, &format!("asinh({exact})"));
        }
    }

    /// `acosh` faithfully rounded across the in-domain interval
    /// `[1, +∞)`, every rounding direction. The generated magnitude
    /// is mapped to `1 + coef · 10^exp` (a value `≥ 1`); an `x < 1`
    /// input is the documented domain-INVALID special (NaN +
    /// INVALID), not a faithfully-rounded value, so it is kept out of
    /// the sweep by construction — no bracket weakening, the
    /// in-domain bracket stays the full 5-mode contract.
    #[test]
    fn acosh_random_faithful(
        coef_bits in 1u32..=u32::MAX,
        exp in -8i32..=20,
    ) {
        // `coef ∈ [1, 10^7)` written with exponent `exp` is a
        // positive value; `1 + that` is always `≥ 1`, strictly inside
        // the acosh `[1, +∞)` domain. `coef == 0` is the trivial
        // value-1 input; skip it with the established idiom.
        let coef = coef_bits % (10u32.pow(7));
        if coef == 0 { return Ok(()); }
        let frac_str = format!("{coef}e{exp}");
        let frac = parse(&frac_str);
        if !frac.is_finite() { return Ok(()); }
        let (x, _) = Decimal32::ONE.add(frac, RoundingMode::NearestEven);
        // Defensive: the construction keeps x ≥ 1, but guard against
        // any parse / add corner producing a non-finite or
        // below-domain value (it is then the domain-INVALID special,
        // not a faithful finite result).
        if !x.is_finite() { return Ok(()); }
        if matches!(
            x.partial_cmp(Decimal32::ONE).0,
            Some(core::cmp::Ordering::Less)
        ) {
            return Ok(());
        }
        let exact = format!("{x:e}");
        let mut cc = Consts::new().expect("init consts");
        let oracle = oracle::acosh(&exact, &mut cc);
        for &rm in MODES {
            let (got, status) = x.acosh(rm);
            assert_faithful(got, status, &oracle, &mut cc, rm, &format!("acosh({exact})"));
        }
    }

    /// `atanh` faithfully rounded across the in-domain interval
    /// `(-1, +1)`, every rounding direction. The generated magnitude
    /// is mapped strictly inside `(-1, 1)`; an `|x| ≥ 1` input is the
    /// documented pole (`±∞ + DIV_BY_ZERO`) or domain-INVALID special,
    /// not a faithfully-rounded value, so it is kept out of the sweep
    /// by construction — no bracket weakening, the in-domain bracket
    /// stays the full 5-mode contract.
    #[test]
    fn atanh_random_faithful(
        coef_bits in 1u32..=u32::MAX,
        extra_exp in 0i32..=11,
        sign in any::<bool>(),
    ) {
        // `coef ∈ [1, 10^7)` written with exponent `-(7 + extra_exp)`
        // is `coef · 10^-(7+extra_exp)`, always strictly inside the
        // atanh `(-1, 1)` domain (a 7-digit coefficient times `10^-7`
        // is `< 1`). `coef == 0` is the trivial zero input; skip it
        // with the established idiom.
        let coef = coef_bits % (10u32.pow(7));
        if coef == 0 { return Ok(()); }
        let value_str = format!("{}{}e{}",
            if sign { "-" } else { "" }, coef, -(7 + extra_exp));
        let x = parse(&value_str);
        // Defensive: the construction keeps |x| < 1, but guard the
        // bracket against any parse corner producing a non-finite or
        // out-of-domain value (it is then the pole / domain-INVALID
        // special, not a faithful finite result).
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
        let oracle = oracle::atanh(&exact, &mut cc);
        for &rm in MODES {
            let (got, status) = x.atanh(rm);
            assert_faithful(got, status, &oracle, &mut cc, rm, &format!("atanh({exact})"));
        }
    }
}
