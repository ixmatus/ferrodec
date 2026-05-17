//! Faithful-rounding contract for `Decimal32::sin` / `cos` vs the
//! shared astro-float oracle, asserted for every IEEE 754 rounding
//! direction (ADR-0021, IEEE 754-2019 §9.2). Companion to
//! `tests/property_sincos_large.rs` (large `|x|`). See
//! `tests/common/mod.rs` for the contract; this is not a `± ULP`
//! tolerance envelope.
//!
//! The fd-r0l P3 rewire moved `sin` / `cos` off the pre-fd-r0l lossy
//! `f64` / `libm` detour onto the shared faithful
//! `ferrodec-transcend` Payne-Hanek kernel. This suite stays
//! astro-float-free: the oracle reaches it only through the
//! `ferrodec_test_support::transcend_oracle` builders (Design A), so
//! astro-float never appears in the decimal32 dependency graph.

#![cfg(feature = "trig")]

use ferrodec_test_support::transcend_oracle::{oracle, Consts};
use proptest::prelude::*;

mod common;
use common::{assert_faithful, parse, MODES};

fn check_sin_cos(x_str: &str) {
    let x = parse(x_str);
    let exact = format!("{x:e}");
    let mut cc = Consts::new().expect("init consts");
    let o_sin = oracle::sin(&exact, &mut cc);
    let o_cos = oracle::cos(&exact, &mut cc);
    for &rm in MODES {
        let (got_sin, s_sin) = x.sin(rm);
        let (got_cos, s_cos) = x.cos(rm);
        assert_faithful(
            got_sin,
            s_sin,
            &o_sin,
            &mut cc,
            rm,
            &format!("sin({exact})"),
        );
        assert_faithful(
            got_cos,
            s_cos,
            &o_cos,
            &mut cc,
            rm,
            &format!("cos({exact})"),
        );
    }
}

// Spot tests --------------------------------------------------------------

#[test]
fn spot_zero() {
    check_sin_cos("0");
}
#[test]
fn spot_one() {
    check_sin_cos("1");
}
#[test]
fn spot_neg_one() {
    check_sin_cos("-1");
}
#[test]
fn spot_pi_over_six() {
    check_sin_cos("0.5235988");
}
#[test]
fn spot_pi_over_four() {
    check_sin_cos("0.7853982");
}
#[test]
fn spot_pi_over_three() {
    check_sin_cos("1.047198");
}
// These three inputs land within ~1 ULP of an integer multiple of
// π/2, the worst case for argument reduction.
#[test]
fn spot_pi_over_two() {
    check_sin_cos("1.570796");
}
#[test]
fn spot_pi() {
    check_sin_cos("3.141593");
}
#[test]
fn spot_two_pi() {
    check_sin_cos("6.283185");
}
#[test]
fn spot_tiny_pos() {
    check_sin_cos("0.00001");
}
#[test]
fn spot_tiny_neg() {
    check_sin_cos("-0.00001");
}
#[test]
fn spot_random_finite() {
    check_sin_cos("123.4568");
}
#[test]
fn spot_just_under_one() {
    check_sin_cos("0.9999999");
}

// Property sweep ----------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// `sin` / `cos` faithfully rounded across moderate-magnitude
    /// inputs, every rounding direction.
    #[test]
    fn sincos_random_faithful(
        coef_bits in 1u32..=u32::MAX,
        exp in -10i32..=15,
        sign in any::<bool>(),
    ) {
        let coef = coef_bits % (10u32.pow(7));
        if coef == 0 { return Ok(()); }
        let value_str = format!("{}{}e{}", if sign { "-" } else { "" }, coef, exp);
        let x = parse(&value_str);
        // A parse-overflowed ±∞ input is out of the faithful domain
        // (`sin`/`cos` of ±∞ is NaN + INVALID, a special result, not
        // a faithfully-rounded finite value). Skip it without
        // weakening the bracket, the same idiom as the `coef == 0`
        // skip; the generated `exp ≤ 15` range cannot overflow
        // `Decimal32`, so this is a defensive guard rather than a
        // reachable corner.
        if !x.is_finite() { return Ok(()); }
        let exact = format!("{x:e}");
        let mut cc = Consts::new().expect("init consts");
        let o_sin = oracle::sin(&exact, &mut cc);
        let o_cos = oracle::cos(&exact, &mut cc);
        for &rm in MODES {
            let (got_sin, s_sin) = x.sin(rm);
            let (got_cos, s_cos) = x.cos(rm);
            assert_faithful(got_sin, s_sin, &o_sin, &mut cc, rm, &format!("sin({exact})"));
            assert_faithful(got_cos, s_cos, &o_cos, &mut cc, rm, &format!("cos({exact})"));
        }
    }
}
