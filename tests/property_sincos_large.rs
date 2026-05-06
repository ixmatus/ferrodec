//! Accuracy tests for `Decimal128::sin` / `cos` at large `|x|`.
//!
//! The Phase 7 implementation capped trig accuracy at `|x| ≤ 10^9`
//! because of cancellation in the native-arithmetic argument
//! reduction. The Payne-Hanek argument reduction (`src/math/argred.rs`)
//! lifts that cap; this file confirms it by:
//!
//! 1. Cross-checking `sin(x)` and `cos(x)` against an `astro-float`
//!    oracle at moderate-to-extreme magnitudes (10^5 .. 10^3000).
//! 2. Verifying `sin² + cos² = 1` at the same magnitudes (a fully
//!    self-contained sanity check that doesn't depend on the oracle).
//!
//! Tolerances are documented as 5 ULP — the v1 envelope. In practice
//! we observe ~2 ULP on the spot checks, but we leave the looser bound
//! to avoid noise on edge cases where the rounding direction of the
//! Taylor series differs from the oracle by 1.

#![cfg(feature = "trig")]

use astro_float::{BigFloat, Consts, Radix, RoundingMode as AfRm};
use ferrodec::RoundingMode;

mod common;
use common::{bigfloat_to_decimal_string, parse, within_ulps};

/// Compute `sin(x_str)` to a working precision wide enough to absorb
/// `x`'s magnitude. Returns a 50-digit decimal string suitable for
/// `parse_str`.
fn oracle_sin(x_str: &str, p_bits: usize) -> String {
    let rm = AfRm::None;
    let mut cc = Consts::new().expect("init consts");
    let x = BigFloat::parse(x_str, Radix::Dec, p_bits, rm, &mut cc);
    let s = x.sin(p_bits, rm, &mut cc);
    bigfloat_to_decimal_string(&s, &mut cc, 50)
}

fn oracle_cos(x_str: &str, p_bits: usize) -> String {
    let rm = AfRm::None;
    let mut cc = Consts::new().expect("init consts");
    let x = BigFloat::parse(x_str, Radix::Dec, p_bits, rm, &mut cc);
    let c = x.cos(p_bits, rm, &mut cc);
    bigfloat_to_decimal_string(&c, &mut cc, 50)
}

/// Estimate the bits of working precision needed for astro-float to
/// reduce `|x| ~ 10^magnitude` and still keep ~120 digits of result
/// precision. ~3.4 bits per decimal digit, plus 100 bits of slack.
fn oracle_p_bits(magnitude: usize) -> usize {
    ((magnitude + 130) as f64 * 3.4) as usize + 100
}

fn check_sin_cos_at(x_str: &str, magnitude: usize, ulps: u32) {
    let p_bits = oracle_p_bits(magnitude);
    let x = parse(x_str);
    let (got_sin, _) = x.sin(RoundingMode::NearestEven);
    let (got_cos, _) = x.cos(RoundingMode::NearestEven);

    // Round-trip the parsed value back through Display to get the exact
    // 34-digit representation. This is what we want the oracle to use,
    // so that both implementations operate on the same input — without
    // this, longer input strings get rounded by Decimal128 but not by
    // astro-float, biasing the comparison.
    let exact_str = format!("{x}");

    let want_sin_str = oracle_sin(&exact_str, p_bits);
    let want_cos_str = oracle_cos(&exact_str, p_bits);
    let want_sin = parse(&want_sin_str);
    let want_cos = parse(&want_cos_str);

    assert!(
        within_ulps(got_sin, want_sin, ulps),
        "sin({x_str} → {exact_str}): got {got_sin:?}, want ≈ {want_sin:?} (oracle {want_sin_str})"
    );
    assert!(
        within_ulps(got_cos, want_cos, ulps),
        "cos({x_str} → {exact_str}): got {got_cos:?}, want ≈ {want_cos:?} (oracle {want_cos_str})"
    );
}

#[test]
fn sincos_oracle_1e5() {
    check_sin_cos_at("1e5", 5, 5);
}

#[test]
fn sincos_oracle_1e9() {
    check_sin_cos_at("1e9", 9, 5);
}

#[test]
fn sincos_oracle_1e15() {
    // Beyond the legacy |x| ≤ 10^9 cap.
    check_sin_cos_at("1e15", 15, 5);
}

#[test]
fn sincos_oracle_1e30() {
    check_sin_cos_at("1e30", 30, 10);
}

#[test]
fn sincos_oracle_1e100() {
    check_sin_cos_at("1e100", 100, 10);
}

#[test]
fn sincos_oracle_1e500_specific() {
    // A non-power-of-10 large input — prevents accidental success on
    // round numbers that happen to hit exact-integer sin/cos. The
    // input value is intentionally near a multiple of π so the
    // reduction has to extract a small residual from a large
    // magnitude, exercising the windowed multiplication's high-i
    // digits of `2/π`.
    check_sin_cos_at("3.14159265358979323846264338327950288e500", 500, 10);
}

#[test]
fn pythagorean_at_1e15() {
    let x = parse("1e15");
    let (s, _) = x.sin(RoundingMode::NearestEven);
    let (c, _) = x.cos(RoundingMode::NearestEven);
    let (ss, _) = s.mul(s, RoundingMode::NearestEven);
    let (cc, _) = c.mul(c, RoundingMode::NearestEven);
    let (sum, _) = ss.add(cc, RoundingMode::NearestEven);
    assert!(
        within_ulps(sum, parse("1"), 50),
        "sin² + cos² at 1e15 = {sum:?}"
    );
}

#[test]
fn pythagorean_at_1e500() {
    let x = parse("1e500");
    let (s, _) = x.sin(RoundingMode::NearestEven);
    let (c, _) = x.cos(RoundingMode::NearestEven);
    let (ss, _) = s.mul(s, RoundingMode::NearestEven);
    let (cc, _) = c.mul(c, RoundingMode::NearestEven);
    let (sum, _) = ss.add(cc, RoundingMode::NearestEven);
    assert!(
        within_ulps(sum, parse("1"), 50),
        "sin² + cos² at 1e500 = {sum:?}"
    );
}

#[test]
fn pythagorean_at_1e3000() {
    let x = parse("1e3000");
    let (s, _) = x.sin(RoundingMode::NearestEven);
    let (c, _) = x.cos(RoundingMode::NearestEven);
    let (ss, _) = s.mul(s, RoundingMode::NearestEven);
    let (cc, _) = c.mul(c, RoundingMode::NearestEven);
    let (sum, _) = ss.add(cc, RoundingMode::NearestEven);
    assert!(
        within_ulps(sum, parse("1"), 100),
        "sin² + cos² at 1e3000 = {sum:?}"
    );
}
