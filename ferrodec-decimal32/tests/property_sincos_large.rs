//! Faithful-rounding contract for `Decimal32::sin` / `cos` at large
//! `|x|`, asserted for every IEEE 754 rounding direction (ADR-0021,
//! IEEE 754-2019 §9.2).
//!
//! The fd-r0l P3 rewire put `sin` / `cos` on the shared
//! `ferrodec-transcend` Payne-Hanek argument reduction, lifting the
//! pre-fd-r0l f64-round-trip accuracy limitation. This file confirms
//! the reduction is *faithful* across `Decimal32`'s
//! moderate-to-extreme magnitude range (`10^5 .. 10^90`) against the
//! shared astro-float oracle whose working precision is widened to
//! absorb `x`'s magnitude (the `oracle::sin_at` / `cos_at`
//! precision-parametrised builders), and separately checks the
//! `sin² + cos² = 1` identity (an oracle-free algebraic sanity
//! check, deliberately *not* the faithfulness contract). This is not
//! a `± ULP` tolerance envelope.
//!
//! Decimal32 stays astro-float-free (Design A): the magnitude-widened
//! oracle is reached only through the shared `transcend_oracle`
//! builders, so astro-float never appears in the decimal32
//! dependency graph. The fixed 256-bit `oracle::sin` builder loses
//! the reduced residual past `~10^70`, so the large suite uses the
//! `oracle::sin_at` variant with a magnitude-scaled precision.

#![cfg(feature = "trig")]

use core::cmp::Ordering;
use ferrodec_decimal32::{Decimal32, RoundingMode};
use ferrodec_test_support::transcend_oracle::{oracle, Consts};

mod common;
use common::{assert_faithful, parse, MODES};

/// Bits of working precision for the shared oracle to reduce `|x| ~
/// 10^magnitude` and still keep ~120 digits of result precision.
/// ~3.4 bits per decimal digit, plus 100 bits of slack — the same
/// rule the direct-tier Decimal128 `property_sincos_large` suite
/// uses.
fn oracle_p_bits(magnitude: usize) -> usize {
    ((magnitude + 130) as f64 * 3.4) as usize + 100
}

/// Oracle-free relative-closeness check for the `sin² + cos² = 1`
/// algebraic identity. It is deliberately *not* the faithfulness
/// contract (the identity composes several rounded ops and exceeds
/// one ULP by construction); a loose relative bound at the
/// `Decimal32` 7-digit scale is the right check. Local here rather
/// than in `tests/common/mod.rs` because it is specific to this
/// identity sanity check and the shared adapter carries only the
/// faithful machinery.
fn within_rel_ulps(got: Decimal32, want: Decimal32, ulps: u32) -> bool {
    let (diff, _) = got.sub(want, RoundingMode::NearestEven);
    let diff = diff.abs();
    let abs_want = want.abs();
    if abs_want.is_zero() {
        let bound = parse(&format!("{ulps}e-6"));
        let (cmp, _) = diff.partial_cmp(bound);
        return matches!(cmp, Some(Ordering::Less | Ordering::Equal));
    }
    let (rel, _) = diff.div(abs_want, RoundingMode::NearestEven);
    let bound = parse(&format!("{ulps}e-6"));
    let (cmp, _) = rel.partial_cmp(bound);
    matches!(cmp, Some(Ordering::Less | Ordering::Equal))
}

fn check_sin_cos_at(x_str: &str, magnitude: usize) {
    let p_bits = oracle_p_bits(magnitude);
    let x = parse(x_str);
    let exact = format!("{x:e}");
    let mut cc = Consts::new().expect("init consts");
    let o_sin = oracle::sin_at(&exact, p_bits, &mut cc);
    let o_cos = oracle::cos_at(&exact, p_bits, &mut cc);
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
fn sincos_oracle_1e60() {
    check_sin_cos_at("1e60", 60);
}

#[test]
fn sincos_oracle_1e90_specific() {
    // A non-power-of-10 large input near the top of the Decimal32
    // normal range (E_MAX = 96), so the reduction must extract a
    // small residual from a large magnitude, exercising the windowed
    // multiplication's high-i digits of 2/π.
    check_sin_cos_at("3.141593e90", 90);
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
fn pythagorean_at_1e60() {
    let x = parse("1e60");
    let (s, _) = x.sin(RoundingMode::NearestEven);
    let (c, _) = x.cos(RoundingMode::NearestEven);
    let (ss, _) = s.mul(s, RoundingMode::NearestEven);
    let (cc, _) = c.mul(c, RoundingMode::NearestEven);
    let (sum, _) = ss.add(cc, RoundingMode::NearestEven);
    assert!(
        within_rel_ulps(sum, parse("1"), 50),
        "sin² + cos² at 1e60 = {sum:?}"
    );
}

#[test]
fn pythagorean_at_1e90() {
    let x = parse("1e90");
    let (s, _) = x.sin(RoundingMode::NearestEven);
    let (c, _) = x.cos(RoundingMode::NearestEven);
    let (ss, _) = s.mul(s, RoundingMode::NearestEven);
    let (cc, _) = c.mul(c, RoundingMode::NearestEven);
    let (sum, _) = ss.add(cc, RoundingMode::NearestEven);
    assert!(
        within_rel_ulps(sum, parse("1"), 100),
        "sin² + cos² at 1e90 = {sum:?}"
    );
}
