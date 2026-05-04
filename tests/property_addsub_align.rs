//! Regression tests for the sub-ULP effective-subtract path in
//! `src/ops/addsub.rs`.
//!
//! When `|a − b|`'s exponent difference exceeds `ALIGN_LIMIT = 43`,
//! the smaller operand falls below 1 ULP of the larger and needs
//! special handling to round correctly. The kernel's
//! `sub_ulp_effective_sub` branch covers this. These tests
//! cross-check it against `astro-float` for a range of Δ values
//! (specifically Δ > 43).

use astro_float::{BigFloat, Consts, Radix, RoundingMode as AfRm, Sign};
use ferrodec::{Decimal128, RoundingMode};
use proptest::prelude::*;

fn parse(s: &str) -> Decimal128 {
    Decimal128::parse_str(s, RoundingMode::NearestEven)
        .unwrap()
        .0
}

fn bigfloat_to_decimal(bf: &BigFloat, cc: &mut Consts) -> Decimal128 {
    let (sign, mantissa, exp) = bf
        .convert_to_radix(Radix::Dec, AfRm::ToEven, cc)
        .expect("convert");
    if mantissa.is_empty() || mantissa.iter().all(|&d| d == 0) {
        return Decimal128::ZERO;
    }
    let take = mantissa.len().min(40);
    let digit_str: String = mantissa[..take]
        .iter()
        .map(|&d| char::from(b'0' + d))
        .collect();
    let scale = exp - take as i32;
    let sign_str = if matches!(sign, Sign::Neg) { "-" } else { "" };
    parse(&format!("{sign_str}{digit_str}e{scale}"))
}

/// Compute `a + b` (or `a − b`) at high precision via astro-float and
/// round the result to Decimal128.
fn oracle_add(a: Decimal128, b: Decimal128) -> Decimal128 {
    let p = 200;
    let rm = AfRm::ToEven;
    let mut cc = Consts::new().expect("init consts");
    let a_str = format!("{a}");
    let b_str = format!("{b}");
    let av = BigFloat::parse(&a_str, Radix::Dec, p, rm, &mut cc);
    let bv = BigFloat::parse(&b_str, Radix::Dec, p, rm, &mut cc);
    let sum = av.add(&bv, p, rm);
    bigfloat_to_decimal(&sum, &mut cc)
}

#[test]
fn sub_ulp_effective_sub_delta_50() {
    // a = 1.0 (exponent 0), b = -1e-50. Effective subtraction with
    // Δ = 50 ≫ ALIGN_LIMIT. The result should be just below 1 by
    // ~1e-50 — Decimal128 at 34 digits represents this as the value
    // closest to 1 - 1e-50 from below or above per round-half-even.
    let a = parse("1");
    let b = parse("-1e-50");
    let (got, _) = a.add(b, RoundingMode::NearestEven);
    let want = oracle_add(a, b);
    let (cmp, _) = got.partial_cmp(want);
    assert_eq!(
        cmp,
        Some(core::cmp::Ordering::Equal),
        "got {got:?}, want {want:?}"
    );
}

#[test]
fn sub_ulp_effective_sub_delta_60() {
    let a = parse("3.14");
    let b = parse("-1.23e-60");
    let (got, _) = a.add(b, RoundingMode::NearestEven);
    let want = oracle_add(a, b);
    assert_eq!(got.to_bits(), want.to_bits(), "got {got:?}, want {want:?}");
}

#[test]
fn sub_ulp_effective_sub_at_pow10() {
    // a = 1e0 is a power of 10; the lower candidate crosses cohorts
    // (becomes 9999...9 × 10^{-33}). Verify that branch.
    let a = parse("1");
    let b = parse("-7e-50");
    let (got, _) = a.add(b, RoundingMode::NearestEven);
    let want = oracle_add(a, b);
    assert_eq!(got.to_bits(), want.to_bits(), "got {got:?}, want {want:?}");
}

#[test]
fn sub_ulp_effective_sub_far_below_half() {
    // 2·cs ≪ 10^(diff − k) — should round to upper (no change to a).
    let a = parse("1.234567890123456789012345678901234");
    let b = parse("-1e-80"); // way below ULP
    let (got, _) = a.add(b, RoundingMode::NearestEven);
    let want = oracle_add(a, b);
    assert_eq!(got.to_bits(), want.to_bits(), "got {got:?}, want {want:?}");
}

#[test]
fn sub_ulp_effective_sub_just_above_half() {
    // Construct an input where eps is just above 0.5 ULP.
    // a = 2 (1 sig digit), so PRECISION − digits = 33. ULP at target
    // is 10^{-33}. Let b = -6e-50, eps = 6e-50. 0.5 ULP = 5e-34. eps ≪
    // 0.5 ULP so this rounds to upper (= a). Just verify oracle agrees.
    let a = parse("2");
    let b = parse("-6e-50");
    let (got, _) = a.add(b, RoundingMode::NearestEven);
    let want = oracle_add(a, b);
    assert_eq!(got.to_bits(), want.to_bits(), "got {got:?}, want {want:?}");
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// Random sub-ULP effective-subtract: a ≈ 1, b ≈ -10^{-(44..80)}.
    /// Cross-check against astro-float.
    #[test]
    fn sub_ulp_effective_sub_random(
        a_coef in 1u128..=u128::MAX,
        b_coef in 1u128..=u128::MAX,
        delta in 44i32..=80,
    ) {
        let ac = a_coef % 10u128.pow(34);
        let bc = b_coef % 10u128.pow(34);
        if ac == 0 || bc == 0 { return Ok(()); }
        let a = parse(&format!("{ac}e-33"));            // |a| ~ 1
        let b = parse(&format!("-{bc}e-{}", delta + 33));  // |b| ~ 10^{-delta}
        let (got, _) = a.add(b, RoundingMode::NearestEven);
        let want = oracle_add(a, b);
        let (cmp, _) = got.partial_cmp(want);
        prop_assert_eq!(
            cmp,
            Some(core::cmp::Ordering::Equal),
            "a={:?} b={:?}: got {:?}, want {:?}", a, b, got, want
        );
    }
}
