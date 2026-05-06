//! Faithful-rounding cross-check for `Decimal128::sin` / `cos` vs astro-float.
//!
//! Companion to `tests/property_sincos_large.rs`, which only checked
//! large-|x| inputs at 5-10 ULP. With the Taylor body running at
//! `Extended` precision and the Payne-Hanek window widened to
//! `FRAC_DIGITS = 76` (via the U512 multiplication path), we now hold
//! to 1 ULP across the full domain — including inputs that land
//! within 1 ULP of a multiple of π/2.

#![cfg(feature = "trig")]

use astro_float::{BigFloat, Consts, Radix, RoundingMode as AfRm};
use ferrodec::{Decimal128, RoundingMode};
use proptest::prelude::*;

mod common;
use common::{bigfloat_to_decimal_string, parse, within_ulps};

fn oracle_sin(x_str: &str) -> String {
    oracle_apply(x_str, astro_float::BigFloat::sin)
}

fn oracle_cos(x_str: &str) -> String {
    oracle_apply(x_str, astro_float::BigFloat::cos)
}

fn oracle_apply<F>(x_str: &str, f: F) -> String
where
    F: FnOnce(&BigFloat, usize, AfRm, &mut Consts) -> BigFloat,
{
    let p = 220;
    let rm = AfRm::None;
    let mut cc = Consts::new().expect("init consts");
    let x = BigFloat::parse(x_str, Radix::Dec, p, rm, &mut cc);
    let r = f(&x, p, rm, &mut cc);
    bigfloat_to_decimal_string(&r, &mut cc, 50)
}

fn check_sin_cos(x_str: &str, ulps: u32) {
    let x = parse(x_str);
    let exact_str = format!("{x}");
    let (got_sin, _) = x.sin(RoundingMode::NearestEven);
    let (got_cos, _) = x.cos(RoundingMode::NearestEven);
    let want_sin_str = oracle_sin(&exact_str);
    let want_cos_str = oracle_cos(&exact_str);
    let want_sin = parse(&want_sin_str);
    let want_cos = parse(&want_cos_str);
    assert!(
        within_ulps(got_sin, want_sin, ulps),
        "sin({exact_str}): got {got_sin:?}, want {want_sin:?} (oracle {want_sin_str})"
    );
    assert!(
        within_ulps(got_cos, want_cos, ulps),
        "cos({exact_str}): got {got_cos:?}, want {want_cos:?} (oracle {want_cos_str})"
    );
}

// Spot tests --------------------------------------------------------------

#[test]
fn spot_zero() {
    check_sin_cos("0", 1);
}
#[test]
fn spot_one() {
    check_sin_cos("1", 1);
}
#[test]
fn spot_neg_one() {
    check_sin_cos("-1", 1);
}
#[test]
fn spot_pi_over_six() {
    check_sin_cos("0.5235987755982988730771072305465838", 1);
}
#[test]
fn spot_pi_over_four() {
    check_sin_cos("0.7853981633974483096156608458198757", 1);
}
#[test]
fn spot_pi_over_three() {
    check_sin_cos("1.047197551196597746154214461093168", 1);
}
// These three inputs land within ~1 ULP of an integer multiple of π/2.
// The Payne-Hanek window now extracts 76 fractional digits — enough to
// retain ≥ 43 sig digits after the worst-case 33-digit cancellation —
// so they round faithfully (≤ 1 ULP).
#[test]
fn spot_pi_over_two() {
    check_sin_cos("1.570796326794896619231321691639751", 1);
}
#[test]
fn spot_pi() {
    check_sin_cos("3.141592653589793238462643383279503", 1);
}
#[test]
fn spot_two_pi() {
    check_sin_cos("6.283185307179586476925286766559006", 1);
}
#[test]
fn spot_tiny_pos() {
    check_sin_cos("0.00001", 1);
}
#[test]
fn spot_tiny_neg() {
    check_sin_cos("-0.00001", 1);
}
#[test]
fn spot_random_finite() {
    check_sin_cos("123.456789012345", 1);
}
#[test]
fn spot_just_under_one() {
    check_sin_cos("0.9999999999999999999999999999999999", 1);
}

// Property sweep ----------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// `sin` / `cos` at 1 ULP across moderate-magnitude inputs.
    #[test]
    fn sincos_random_within_1_ulp(
        coef_bits in 1u128..=u128::MAX,
        exp in -10i32..=15,
        sign in any::<bool>(),
    ) {
        let coef = coef_bits % (10u128.pow(34));
        if coef == 0 { return Ok(()); }
        let value_str = format!("{}{}e{}", if sign { "-" } else { "" }, coef, exp);
        let x = parse(&value_str);
        let exact_str = format!("{x}");
        let (got_sin, _) = x.sin(RoundingMode::NearestEven);
        let (got_cos, _) = x.cos(RoundingMode::NearestEven);
        let want_sin_str = oracle_sin(&exact_str);
        let want_cos_str = oracle_cos(&exact_str);
        let want_sin = parse(&want_sin_str);
        let want_cos = parse(&want_cos_str);
        prop_assert!(
            within_ulps(got_sin, want_sin, 1),
            "sin({exact_str}): got {got_sin:?}, want {want_sin:?} (oracle {want_sin_str})"
        );
        prop_assert!(
            within_ulps(got_cos, want_cos, 1),
            "cos({exact_str}): got {got_cos:?}, want {want_cos:?} (oracle {want_cos_str})"
        );
    }
}
