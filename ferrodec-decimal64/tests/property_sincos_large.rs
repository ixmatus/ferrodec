//! Faithful-rounding contract for `Decimal64::sin` / `cos` at large
//! `|x|`, asserted for every IEEE 754 rounding direction (ADR-0021,
//! IEEE 754-2019 §9.2).
//!
//! The fd-r0l P3 rewire put `sin` / `cos` on the shared
//! `ferrodec-transcend` Payne-Hanek argument reduction, lifting the
//! pre-fd-r0l `|x| < 2^53` accuracy limitation (the f64 round-trip
//! lost the low digits before reduction began). This file confirms
//! the reduction is *faithful* across `Decimal64`'s
//! moderate-to-extreme magnitude range (`10^5 .. 10^380`) against an
//! astro-float oracle whose working precision is widened to absorb
//! `x`'s magnitude, and separately checks the `sin² + cos² = 1`
//! identity (an oracle-free algebraic sanity check; see
//! `within_rel_ulps`, deliberately *not* the faithfulness contract).
//! This is not a `± ULP` tolerance envelope.
//!
//! `Decimal64` carries astro-float as a direct dev-dependency
//! (TIER-1, like the `Decimal128` parent), so the magnitude-widened
//! oracle is built directly here; the shared `transcend_oracle`
//! builders are fixed at 256 bits and cannot absorb `10^380`.

#![cfg(feature = "trig")]

use astro_float::{BigFloat, Consts, Radix, RoundingMode as AfRm};
use core::cmp::Ordering;
use ferrodec_decimal64::{Decimal64, RoundingMode};

mod common;
use common::{assert_faithful, parse, MODES};

/// Oracle-free relative-closeness check for the `sin² + cos² = 1`
/// algebraic identity. It is deliberately *not* the faithfulness
/// contract (the identity composes several rounded ops and exceeds
/// one ULP by construction); a loose relative bound at the
/// `Decimal64` 16-digit scale is the right check. Local here rather
/// than in `tests/common/mod.rs` because it is specific to this
/// identity sanity check and the shared adapter carries only the
/// faithful machinery.
fn within_rel_ulps(got: Decimal64, want: Decimal64, ulps: u32) -> bool {
    let (diff, _) = got.sub(want, RoundingMode::NearestEven);
    let diff = diff.abs();
    let abs_want = want.abs();
    if abs_want.is_zero() {
        let bound = parse(&format!("{ulps}e-15"));
        let (cmp, _) = diff.partial_cmp(bound);
        return matches!(cmp, Some(Ordering::Less | Ordering::Equal));
    }
    let (rel, _) = diff.div(abs_want, RoundingMode::NearestEven);
    let bound = parse(&format!("{ulps}e-15"));
    let (cmp, _) = rel.partial_cmp(bound);
    matches!(cmp, Some(Ordering::Less | Ordering::Equal))
}

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
    // Beyond the pre-fd-r0l |x| < 2^53 ≈ 9.0e15 reduction limit.
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
fn sincos_oracle_1e380_specific() {
    // A non-power-of-10 large input near the top of the Decimal64
    // normal range (E_MAX = 384), so the reduction must extract a
    // small residual from a large magnitude, exercising the windowed
    // multiplication's high-i digits of 2/π.
    check_sin_cos_at("3.141592653589793e380", 380);
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
fn pythagorean_at_1e100() {
    let x = parse("1e100");
    let (s, _) = x.sin(RoundingMode::NearestEven);
    let (c, _) = x.cos(RoundingMode::NearestEven);
    let (ss, _) = s.mul(s, RoundingMode::NearestEven);
    let (cc, _) = c.mul(c, RoundingMode::NearestEven);
    let (sum, _) = ss.add(cc, RoundingMode::NearestEven);
    assert!(
        within_rel_ulps(sum, parse("1"), 50),
        "sin² + cos² at 1e100 = {sum:?}"
    );
}

#[test]
fn pythagorean_at_1e380() {
    let x = parse("1e380");
    let (s, _) = x.sin(RoundingMode::NearestEven);
    let (c, _) = x.cos(RoundingMode::NearestEven);
    let (ss, _) = s.mul(s, RoundingMode::NearestEven);
    let (cc, _) = c.mul(c, RoundingMode::NearestEven);
    let (sum, _) = ss.add(cc, RoundingMode::NearestEven);
    assert!(
        within_rel_ulps(sum, parse("1"), 100),
        "sin² + cos² at 1e380 = {sum:?}"
    );
}
