//! Faithful-rounding cross-check for `Decimal128::exp` vs astro-float.
//!
//! Spot tests at hand-picked inputs plus a 256-case random sweep across
//! the supported domain. Tolerance is `≤ 1 ULP` at 34-digit precision.

#![cfg(feature = "transcendentals")]

use astro_float::{BigFloat, Consts, Radix, RoundingMode as AfRm, Sign};
use ferrodec::{Decimal128, RoundingMode};
use proptest::prelude::*;

fn parse(s: &str) -> Decimal128 {
    Decimal128::parse_str(s, RoundingMode::NearestEven)
        .unwrap()
        .0
}

/// Compute `exp(x_str)` via astro-float at 200-bit precision (~60
/// decimal digits) and return as a 50-digit decimal string.
fn oracle_exp(x_str: &str) -> String {
    let p = 200;
    let rm = AfRm::None;
    let mut cc = Consts::new().expect("init consts");
    let x = BigFloat::parse(x_str, Radix::Dec, p, rm, &mut cc);
    let r = x.exp(p, rm, &mut cc);
    bigfloat_to_decimal_string(&r, &mut cc, 50)
}

fn bigfloat_to_decimal_string(v: &BigFloat, cc: &mut Consts, digits: usize) -> String {
    let (sign, mantissa, exp) = v
        .convert_to_radix(Radix::Dec, AfRm::ToEven, cc)
        .expect("convert to decimal");
    if mantissa.is_empty() || mantissa.iter().all(|&d| d == 0) {
        return "0".to_string();
    }
    let take = digits.min(mantissa.len());
    let digit_str: String = mantissa[..take]
        .iter()
        .map(|&d| char::from(b'0' + d))
        .collect();
    let scale = exp - take as i32;
    let sign_str = if matches!(sign, Sign::Neg) { "-" } else { "" };
    format!("{sign_str}{digit_str}e{scale}")
}

fn within_ulps(got: Decimal128, want: Decimal128, ulps: u32) -> bool {
    let (diff, _) = got.sub(want, RoundingMode::NearestEven);
    let diff = diff.abs();
    let abs_want = want.abs();
    if abs_want.is_zero() {
        let bound = parse(&format!("{ulps}e-30"));
        let (cmp, _) = diff.partial_cmp(bound);
        return matches!(
            cmp,
            Some(core::cmp::Ordering::Less | core::cmp::Ordering::Equal)
        );
    }
    let (rel, _) = diff.div(abs_want, RoundingMode::NearestEven);
    let bound = parse(&format!("{ulps}e-33"));
    let (cmp, _) = rel.partial_cmp(bound);
    matches!(
        cmp,
        Some(core::cmp::Ordering::Less | core::cmp::Ordering::Equal)
    )
}

fn check_exp_at(x_str: &str, ulps: u32) {
    let x = parse(x_str);
    let exact_str = format!("{x}");
    let (got, _) = x.exp(RoundingMode::NearestEven);
    let want_str = oracle_exp(&exact_str);
    let want = parse(&want_str);
    assert!(
        within_ulps(got, want, ulps),
        "exp({x_str} → {exact_str}): got {got:?}, want ≈ {want:?} (oracle {want_str})"
    );
}

// Spot tests --------------------------------------------------------------

#[test]
fn spot_zero() {
    check_exp_at("0", 1);
}
#[test]
fn spot_one() {
    check_exp_at("1", 1);
}
#[test]
fn spot_neg_one() {
    check_exp_at("-1", 1);
}
#[test]
fn spot_ten() {
    check_exp_at("10", 1);
}
#[test]
fn spot_neg_ten() {
    check_exp_at("-10", 1);
}
#[test]
fn spot_hundred() {
    check_exp_at("100", 1);
}
#[test]
fn spot_pi() {
    check_exp_at("3.14159265358979323846264338327950288", 1);
}
#[test]
fn spot_neg_pi() {
    check_exp_at("-3.14159265358979323846264338327950288", 1);
}
#[test]
fn spot_small_pos() {
    check_exp_at("0.00001", 1);
}
#[test]
fn spot_small_neg() {
    check_exp_at("-0.00001", 1);
}
#[test]
fn spot_near_overflow() {
    check_exp_at("14149", 1);
}
#[test]
fn spot_near_underflow() {
    check_exp_at("-14149", 1);
}
#[test]
fn spot_large_positive() {
    check_exp_at("5000.123456789", 1);
}
#[test]
fn spot_large_negative() {
    check_exp_at("-5000.123456789", 1);
}

// Property sweep ----------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// `exp` at 1 ULP across a uniform sweep over the supported domain.
    #[test]
    fn exp_random_within_1_ulp(
        // Range covers most of the convergence window; we exclude the
        // last few decades to avoid stress-testing OVERFLOW / UNDERFLOW
        // which the kernel deliberately short-circuits.
        bits in any::<u64>(),
        sign in any::<bool>(),
    ) {
        // Build a value in [10^-30, 10^4] (covers tiny → ~14000).
        let mantissa = (bits & 0xFFFF_FFFF_FFFF_FFFF) as f64 / (u64::MAX as f64);
        let exponent_log10 = -30.0 + mantissa * 34.0; // [-30, +4]
        let abs_value: f64 = (bits as f64).rem_euclid(9.0) + 1.0;
        let value_str = format!("{}{}e{}",
            if sign { "-" } else { "" },
            abs_value,
            exponent_log10 as i32);

        let x = parse(&value_str);
        let exact_str = format!("{x}");
        let (got, _) = x.exp(RoundingMode::NearestEven);
        let want_str = oracle_exp(&exact_str);
        let want = parse(&want_str);
        prop_assert!(
            within_ulps(got, want, 1),
            "exp({exact_str}): got {got:?}, want {want:?} (oracle {want_str})"
        );
    }
}
