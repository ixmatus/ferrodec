//! Special value, exact result, and anchor band gate for
//! `Decimal64::acos_pi` (IEEE 754-2019 §9.2 `acosPi`; ADR-0061
//! Track D D4). The `Decimal128` sibling
//! (`tests/transcend_acospi.rs` in the parent crate) carries the full
//! rationale; this file is the same gate at `P = 16`, where the
//! anchor gate is `adj(x) ≤ -(P + 3) = -19` and the neighbours of
//! `1/2` are `0.4999999999999999` and `0.5000000000000001`.
//!
//! Re-deriving the format specific constants rather than scaling the
//! `Decimal128` ones is deliberate: ADR-0060's named failure mode is
//! a constant bookkeeping error, and a per format derivation is what
//! catches one.

#![cfg(feature = "trig-pi")]

use core::cmp::Ordering;
use ferrodec_decimal64::{Decimal64, RoundingMode, Status};

const NE: RoundingMode = RoundingMode::NearestEven;
const NA: RoundingMode = RoundingMode::NearestAway;
const TZ: RoundingMode = RoundingMode::TowardZero;
const TP: RoundingMode = RoundingMode::TowardPositive;
const TN: RoundingMode = RoundingMode::TowardNegative;
const ALL: [RoundingMode; 5] = [NE, NA, TZ, TP, TN];

const P: i32 = 16;
const GATE: i32 = -(P + 3);
const HALF: &str = "0.5";
const HALF_DOWN: &str = "0.4999999999999999";
const HALF_UP: &str = "0.5000000000000001";

fn parse(s: &str) -> Decimal64 {
    Decimal64::parse_str(s, NE)
        .unwrap_or_else(|_| panic!("test literal parses: {s:?}"))
        .0
}

fn parse_rm(s: &str, rm: RoundingMode) -> Decimal64 {
    Decimal64::parse_str(s, rm)
        .unwrap_or_else(|_| panic!("test literal parses: {s:?}"))
        .0
}

fn eq(got: Decimal64, want: Decimal64) -> bool {
    got.partial_cmp(want).0 == Some(Ordering::Equal)
}

/// The two neighbour literals really are the neighbours of `1/2` at
/// this precision. A value in `[0.1, 1)` carries `P` significant
/// digits at quantum `10^-P`, so the neighbours sit `10^-16` away and
/// the rounding boundaries the anchor derivation must clear are half
/// that, `5·10^-17`.
#[test]
fn the_neighbour_literals_are_one_quantum_from_the_anchor() {
    let quantum = parse(&format!("1e-{P}"));
    let (down, _) = parse(HALF).sub(quantum, NE);
    let (up, _) = parse(HALF).add(quantum, NE);
    assert!(eq(down, parse(HALF_DOWN)), "predecessor literal: {down}");
    assert!(eq(up, parse(HALF_UP)), "successor literal: {up}");
}

/// IEEE 754-2019 §9.2.1, every row, every rounding direction.
#[test]
fn specials_per_section_9_2_1() {
    for rm in ALL {
        for x in [Decimal64::ZERO, Decimal64::NEG_ZERO] {
            let (r, st) = x.acos_pi(rm);
            assert!(eq(r, parse(HALF)), "acosPi(±0) [{rm:?}] = {r}");
            assert_eq!(st, Status::OK, "acosPi(±0) [{rm:?}] flags {st:?}");
        }
        for label in ["1.000000000000001", "-1.5", "2"] {
            let (r, st) = parse(label).acos_pi(rm);
            assert!(r.is_nan() && st.invalid(), "acosPi({label}) [{rm:?}]");
        }
        for x in [Decimal64::INFINITY, Decimal64::NEG_INFINITY] {
            let (r, st) = x.acos_pi(rm);
            assert!(r.is_nan() && st.invalid(), "acosPi(inf) [{rm:?}]");
        }
        let (r, st) = Decimal64::NAN.acos_pi(rm);
        assert!(r.is_nan() && st.is_ok(), "acosPi(NaN) [{rm:?}]");
        let (r, st) = Decimal64::SIGNALING_NAN.acos_pi(rm);
        assert!(r.is_nan() && st.invalid(), "acosPi(sNaN) [{rm:?}]");
    }
}

/// `acosPi(+1) = +0` and `acosPi(-1) = 1`, exact, cohort
/// insensitively.
#[test]
fn unit_rows_are_exact_across_cohorts() {
    for label in ["1", "1.0", "1.00000000"] {
        for rm in ALL {
            let (r, st) = parse(label).acos_pi(rm);
            assert!(
                r.is_zero() && !r.is_sign_negative(),
                "acosPi({label}) [{rm:?}] = {r}"
            );
            assert_eq!(st, Status::OK, "acosPi({label}) [{rm:?}] flags {st:?}");
        }
    }
    for label in ["-1", "-1.0"] {
        for rm in ALL {
            let (r, st) = parse(label).acos_pi(rm);
            assert!(eq(r, parse("1")), "acosPi({label}) [{rm:?}] = {r}");
            assert_eq!(st, Status::OK, "acosPi({label}) [{rm:?}] flags {st:?}");
        }
    }
}

/// The non terminating rational rows at `P = 16`. `1/3 = 0.333…` has
/// a tail below the midpoint, so only `TowardPositive` steps up;
/// `2/3 = 0.666…` has a tail above it, so both nearest modes and
/// `TowardPositive` step up. Both columns are cross checked against
/// the format's own parser on a long expansion of the rational.
#[test]
fn one_third_and_two_thirds_rows() {
    const THIRD_DOWN: &str = "0.3333333333333333";
    const THIRD_UP: &str = "0.3333333333333334";
    for (rm, want) in [
        (NE, THIRD_DOWN),
        (NA, THIRD_DOWN),
        (TZ, THIRD_DOWN),
        (TP, THIRD_UP),
        (TN, THIRD_DOWN),
    ] {
        let (r, st) = parse("0.5").acos_pi(rm);
        assert!(eq(r, parse(want)), "acosPi(0.5) [{rm:?}]: got {r}");
        assert!(st.inexact(), "acosPi(0.5) [{rm:?}] must be INEXACT");
    }
    const TWO_THIRDS_DOWN: &str = "0.6666666666666666";
    const TWO_THIRDS_UP: &str = "0.6666666666666667";
    for (rm, want) in [
        (NE, TWO_THIRDS_UP),
        (NA, TWO_THIRDS_UP),
        (TZ, TWO_THIRDS_DOWN),
        (TP, TWO_THIRDS_UP),
        (TN, TWO_THIRDS_DOWN),
    ] {
        let (r, st) = parse("-0.5").acos_pi(rm);
        assert!(eq(r, parse(want)), "acosPi(-0.5) [{rm:?}]: got {r}");
        assert!(st.inexact(), "acosPi(-0.5) [{rm:?}] must be INEXACT");
    }
    let third_long = format!("0.{}", "3".repeat(40));
    let two_thirds_long = format!("0.{}", "6".repeat(40));
    for rm in ALL {
        let (r, _) = parse("0.5").acos_pi(rm);
        assert!(
            eq(r, parse_rm(&third_long, rm)),
            "acosPi(0.5) [{rm:?}] disagrees with the parsed 1/3 expansion"
        );
        let (r, _) = parse("-0.5").acos_pi(rm);
        assert!(
            eq(r, parse_rm(&two_thirds_long, rm)),
            "acosPi(-0.5) [{rm:?}] disagrees with the parsed 2/3 expansion"
        );
    }
    let (sum, _) = parse(THIRD_DOWN).add(parse(TWO_THIRDS_UP), NE);
    assert!(eq(sum, parse("1")), "1/3 + 2/3 did not compose to 1: {sum}");
}

/// The anchor band: the side is the operand's sign, visible only in
/// the directed modes, and every row is `INEXACT`.
#[test]
fn anchor_band_sides_are_the_operand_sign() {
    for k in [-GATE, -GATE + 5, 100, 380] {
        let x = parse(&format!("1e-{k}"));
        for (rm, want) in [
            (NE, HALF),
            (NA, HALF),
            (TZ, HALF_DOWN),
            (TN, HALF_DOWN),
            (TP, HALF),
        ] {
            let (r, st) = x.acos_pi(rm);
            assert!(eq(r, parse(want)), "acosPi(1e-{k}) [{rm:?}]: got {r}");
            assert!(st.inexact(), "acosPi(1e-{k}) [{rm:?}] flags {st:?}");
        }
        let neg = parse(&format!("-1e-{k}"));
        for (rm, want) in [
            (NE, HALF),
            (NA, HALF),
            (TZ, HALF),
            (TN, HALF),
            (TP, HALF_UP),
        ] {
            let (r, st) = neg.acos_pi(rm);
            assert!(eq(r, parse(want)), "acosPi(-1e-{k}) [{rm:?}]: got {r}");
            assert!(st.inexact(), "acosPi(-1e-{k}) [{rm:?}] flags {st:?}");
        }
    }
}

/// The residual channel and the ladder agree across the gate.
#[test]
fn the_two_treatments_agree_across_the_gate() {
    for sign in ["", "-"] {
        let inside = parse(&format!("{sign}1e{GATE}"));
        let outside = parse(&format!("{sign}1e{}", GATE + 1));
        for rm in ALL {
            let (r_in, st_in) = inside.acos_pi(rm);
            let (r_out, st_out) = outside.acos_pi(rm);
            assert!(
                eq(r_in, r_out),
                "acosPi across the gate [{sign}, {rm:?}]: {r_in} vs {r_out}"
            );
            assert_eq!(st_in, st_out, "flags across the gate [{sign}, {rm:?}]");
        }
    }
}

/// The range is `[0, 1]` and `acosPi` is strictly decreasing.
#[test]
fn range_and_monotonicity() {
    let mut previous: Option<Decimal64> = None;
    for step in 0..=20i32 {
        let x = parse(&format!("{}", (step - 10) as f64 / 10.0));
        let (r, _) = x.acos_pi(NE);
        assert!(
            r.partial_cmp(parse("0")).0 != Some(Ordering::Less)
                && r.partial_cmp(parse("1")).0 != Some(Ordering::Greater),
            "acosPi({x}) = {r} left [0, 1]"
        );
        if let Some(p) = previous {
            assert!(
                r.partial_cmp(p).0 == Some(Ordering::Less),
                "acosPi is not decreasing at {x}: {r} after {p}"
            );
        }
        previous = Some(r);
    }
}

/// The radian kernel, scaled, as the independent witness.
#[test]
#[cfg(feature = "trig")]
fn agrees_with_the_radian_kernel_scaled() {
    let pi = parse("3.141592653589793");
    for label in ["0.5", "-0.5", "0.25", "0.9", "-0.75"] {
        let x = parse(label);
        let (turns, _) = x.acos_pi(NE);
        let (radians, _) = x.acos(NE);
        let (rescaled, _) = turns.mul(pi, NE);
        let (diff, _) = rescaled.sub(radians, NE);
        let (rel, _) = diff.abs().div(radians.abs(), NE);
        assert!(
            rel.partial_cmp(parse("1e-14")).0 == Some(Ordering::Less),
            "acosPi({label})·π = {rescaled} against acos({label}) = {radians}"
        );
    }
}
