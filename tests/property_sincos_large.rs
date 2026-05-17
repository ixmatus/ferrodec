//! Faithful-rounding contract for `Decimal128::sin` / `cos` at large
//! `|x|`, asserted for every IEEE 754 rounding direction (ADR-0021,
//! IEEE 754-2019 §9.2).
//!
//! The Payne-Hanek argument reduction (`src/math/argred.rs`) lifts the
//! legacy `|x| ≤ 10^9` accuracy cap. This file confirms the reduction
//! is *faithful* at moderate-to-extreme magnitudes (`10^5 .. 10^3000`)
//! against an astro-float oracle whose working precision is widened to
//! absorb `x`'s magnitude, and separately checks the
//! `sin² + cos² = 1` identity (an oracle-free algebraic sanity check;
//! see `within_rel_ulps`, deliberately *not* the faithfulness
//! contract). This is not a `± ULP` tolerance envelope.

#![cfg(feature = "trig")]

use astro_float::{BigFloat, Consts, Radix, RoundingMode as AfRm};
use ferrodec::RoundingMode;

mod common;
use common::{assert_faithful, parse, within_rel_ulps, MODES};

fn oracle_apply<F>(x_str: &str, p_bits: usize, f: F, cc: &mut Consts) -> BigFloat
where
    F: FnOnce(&BigFloat, usize, AfRm, &mut Consts) -> BigFloat,
{
    let x = BigFloat::parse(x_str, Radix::Dec, p_bits, AfRm::None, cc);
    f(&x, p_bits, AfRm::None, cc)
}

/// Bits of working precision for astro-float to reduce `|x| ~
/// 10^magnitude` and still keep ~120 digits of result precision.
/// ~3.4 bits per decimal digit, plus 100 bits of slack.
fn oracle_p_bits(magnitude: usize) -> usize {
    ((magnitude + 130) as f64 * 3.4) as usize + 100
}

fn check_sin_cos_at(x_str: &str, magnitude: usize) {
    let p_bits = oracle_p_bits(magnitude);
    let x = parse(x_str);
    let exact = format!("{x:e}");
    let mut cc = Consts::new().expect("init consts");
    let o_sin = oracle_apply(&exact, p_bits, astro_float::BigFloat::sin, &mut cc);
    let o_cos = oracle_apply(&exact, p_bits, astro_float::BigFloat::cos, &mut cc);
    for &rm in MODES {
        let (got_sin, s_sin) = x.sin(rm);
        let (got_cos, s_cos) = x.cos(rm);
        assert_faithful(
            got_sin,
            s_sin,
            &o_sin,
            &mut cc,
            rm,
            &format!("sin({x_str} → {exact})"),
        );
        assert_faithful(
            got_cos,
            s_cos,
            &o_cos,
            &mut cc,
            rm,
            &format!("cos({x_str} → {exact})"),
        );
    }
}

#[test]
fn sincos_oracle_1e5() {
    check_sin_cos_at("1e5", 5);
}

#[test]
fn sincos_oracle_1e9() {
    check_sin_cos_at("1e9", 9);
}

#[test]
fn sincos_oracle_1e15() {
    // Beyond the legacy |x| ≤ 10^9 cap.
    check_sin_cos_at("1e15", 15);
}

#[test]
fn sincos_oracle_1e30() {
    check_sin_cos_at("1e30", 30);
}

#[test]
fn sincos_oracle_1e100() {
    check_sin_cos_at("1e100", 100);
}

#[test]
fn sincos_oracle_1e500_specific() {
    // A non-power-of-10 large input near a multiple of π, so the
    // reduction must extract a small residual from a large magnitude,
    // exercising the windowed multiplication's high-i digits of 2/π.
    check_sin_cos_at("3.14159265358979323846264338327950288e500", 500);
}

// `sin² + cos² = 1` — an oracle-free algebraic identity. It composes
// several rounded operations and so accumulates more than one ULP by
// construction; `within_rel_ulps` (a loose relative bound, *not* the
// faithfulness contract) is the right check here.

#[test]
fn pythagorean_at_1e15() {
    let x = parse("1e15");
    let (s, _) = x.sin(RoundingMode::NearestEven);
    let (c, _) = x.cos(RoundingMode::NearestEven);
    let (ss, _) = s.mul(s, RoundingMode::NearestEven);
    let (cc, _) = c.mul(c, RoundingMode::NearestEven);
    let (sum, _) = ss.add(cc, RoundingMode::NearestEven);
    assert!(
        within_rel_ulps(sum, parse("1"), 50),
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
        within_rel_ulps(sum, parse("1"), 50),
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
        within_rel_ulps(sum, parse("1"), 100),
        "sin² + cos² at 1e3000 = {sum:?}"
    );
}
