//! Faithful-rounding contract for `Decimal128::sin` / `cos` vs
//! astro-float, asserted for every IEEE 754 rounding direction
//! (ADR-0021, IEEE 754-2019 §9.2). Companion to
//! `tests/property_sincos_large.rs` (large `|x|`). See
//! `tests/common/mod.rs`; this is not a `± ULP` tolerance envelope.

#![cfg(feature = "trig")]

use astro_float::Consts;
use ferrodec_test_support::transcend_oracle::oracle;
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
    check_sin_cos("0.5235987755982988730771072305465838");
}
#[test]
fn spot_pi_over_four() {
    check_sin_cos("0.7853981633974483096156608458198757");
}
#[test]
fn spot_pi_over_three() {
    check_sin_cos("1.047197551196597746154214461093168");
}
// These three inputs land within ~1 ULP of an integer multiple of π/2,
// the worst case for argument reduction.
#[test]
fn spot_pi_over_two() {
    check_sin_cos("1.570796326794896619231321691639751");
}
#[test]
fn spot_pi() {
    check_sin_cos("3.141592653589793238462643383279503");
}
#[test]
fn spot_two_pi() {
    check_sin_cos("6.283185307179586476925286766559006");
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
    check_sin_cos("123.456789012345");
}
#[test]
fn spot_just_under_one() {
    check_sin_cos("0.9999999999999999999999999999999999");
}

// Property sweep ----------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// `sin` / `cos` faithfully rounded across moderate-magnitude
    /// inputs, every rounding direction.
    #[test]
    fn sincos_random_faithful(
        coef_bits in 1u128..=u128::MAX,
        exp in -10i32..=15,
        sign in any::<bool>(),
    ) {
        let coef = coef_bits % (10u128.pow(34));
        if coef == 0 { return Ok(()); }
        let value_str = format!("{}{}e{}", if sign { "-" } else { "" }, coef, exp);
        let x = parse(&value_str);
        // A parse-overflowed ±∞ input is out of the faithful domain
        // (`sin`/`cos` of ±∞ is NaN + INVALID, a special result, not a
        // faithfully-rounded finite value). Skip it without weakening
        // the bracket, the same idiom as the `coef == 0` skip; the
        // generated `exp ≤ 15` range cannot overflow `Decimal128`, so
        // this is a defensive guard rather than a reachable corner.
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
