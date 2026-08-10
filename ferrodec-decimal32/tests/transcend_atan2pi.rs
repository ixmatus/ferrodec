//! Special value, exact result, anchor band, and flag gate for
//! `Decimal32::atan2_pi` (IEEE 754-2019 §9.2 `atan2Pi`; ADR-0061
//! Track D D4). The `Decimal128` sibling
//! (`tests/transcend_atan2pi.rs` in the parent crate) carries the
//! full rationale; this file is the same gate at `P = 7`, where the
//! two anchor gates are `adj(y) - adj(x) ≥ 9` and `≤ -10`.
//!
//! The claim worth repeating at every precision: `atan2Pi`'s §9.2.1
//! table is `atan2`'s scaled by `1/π`, and the scaling turns rows
//! that had to round `±π`, `±π/2`, `±π/4` and `±3π/4` into exact
//! multiples of a quarter turn with clean flags.

#![cfg(feature = "trig-pi")]

use core::cmp::Ordering;
use ferrodec_decimal32::{Decimal32, RoundingMode, Status};

const NE: RoundingMode = RoundingMode::NearestEven;
const NA: RoundingMode = RoundingMode::NearestAway;
const TZ: RoundingMode = RoundingMode::TowardZero;
const TP: RoundingMode = RoundingMode::TowardPositive;
const TN: RoundingMode = RoundingMode::TowardNegative;
const ALL: [RoundingMode; 5] = [NE, NA, TZ, TP, TN];

const P: i32 = 7;
const HALF_GATE: i32 = P + 2;
const FULL_GATE: i32 = -(P + 3);

const HALF: &str = "0.5";
const HALF_DOWN: &str = "0.4999999";
const HALF_UP: &str = "0.5000001";
const ONE: &str = "1";
const ONE_DOWN: &str = "0.9999999";

fn parse(s: &str) -> Decimal32 {
    Decimal32::parse_str(s, NE)
        .unwrap_or_else(|_| panic!("test literal parses: {s:?}"))
        .0
}

fn eq(got: Decimal32, want: Decimal32) -> bool {
    got.partial_cmp(want).0 == Some(Ordering::Equal)
}

fn assert_exact(got: (Decimal32, Status), want: &str, label: &str) {
    let (r, st) = got;
    assert!(eq(r, parse(want)), "{label}: got {r}, want {want}");
    assert_eq!(st, Status::OK, "{label}: §7.5 forbids a flag here");
}

/// IEEE 754-2019 §9.2.1, every row, every rounding direction.
#[test]
fn specials_per_section_9_2_1() {
    let inf = Decimal32::INFINITY;
    let ninf = Decimal32::NEG_INFINITY;
    let zero = Decimal32::ZERO;
    let nzero = Decimal32::NEG_ZERO;
    for rm in ALL {
        assert_exact(
            inf.atan2_pi(inf, rm),
            "0.25",
            &format!("(+inf,+inf) {rm:?}"),
        );
        assert_exact(
            ninf.atan2_pi(inf, rm),
            "-0.25",
            &format!("(-inf,+inf) {rm:?}"),
        );
        assert_exact(
            inf.atan2_pi(ninf, rm),
            "0.75",
            &format!("(+inf,-inf) {rm:?}"),
        );
        assert_exact(
            ninf.atan2_pi(ninf, rm),
            "-0.75",
            &format!("(-inf,-inf) {rm:?}"),
        );
        for x in ["0", "-0", "1", "-1"] {
            assert_exact(
                inf.atan2_pi(parse(x), rm),
                HALF,
                &format!("(+inf,{x}) {rm:?}"),
            );
            assert_exact(
                ninf.atan2_pi(parse(x), rm),
                "-0.5",
                &format!("(-inf,{x}) {rm:?}"),
            );
        }
        assert_exact(
            parse("1").atan2_pi(ninf, rm),
            ONE,
            &format!("(1,-inf) {rm:?}"),
        );
        let (r, st) = parse("1").atan2_pi(inf, rm);
        assert!(r.is_zero() && !r.is_sign_negative(), "(1,+inf) {rm:?}");
        assert_eq!(st, Status::OK, "(1,+inf) {rm:?} flags");

        for (y, y_neg) in [(zero, false), (nzero, true)] {
            let (r, st) = y.atan2_pi(zero, rm);
            assert!(r.is_zero() && r.is_sign_negative() == y_neg, "(±0,+0)");
            assert_eq!(st, Status::OK, "(±0,+0) {rm:?} flags");
            assert_exact(
                y.atan2_pi(nzero, rm),
                if y_neg { "-1" } else { ONE },
                &format!("(±0,-0) {rm:?}"),
            );
            assert_exact(
                y.atan2_pi(parse("-3"), rm),
                if y_neg { "-1" } else { ONE },
                &format!("(±0,x<0) {rm:?}"),
            );
            let (r, st) = y.atan2_pi(parse("3"), rm);
            assert!(r.is_zero() && r.is_sign_negative() == y_neg, "(±0,x>0)");
            assert_eq!(st, Status::OK, "(±0,x>0) {rm:?} flags");
        }
        for x in [zero, nzero] {
            assert_exact(
                parse("7").atan2_pi(x, rm),
                HALF,
                &format!("(y>0,±0) {rm:?}"),
            );
            assert_exact(
                parse("-7").atan2_pi(x, rm),
                "-0.5",
                &format!("(y<0,±0) {rm:?}"),
            );
        }

        let (r, st) = Decimal32::NAN.atan2_pi(parse("1"), rm);
        assert!(r.is_nan() && st.is_ok(), "(NaN,1) {rm:?}");
        let (r, st) = parse("1").atan2_pi(Decimal32::SIGNALING_NAN, rm);
        assert!(r.is_nan() && st.invalid(), "(1,sNaN) {rm:?}");
    }
}

/// The flag difference against the radian spelling.
#[test]
#[cfg(feature = "trig")]
fn axis_rows_are_exact_where_atan2_was_inexact() {
    let inf = Decimal32::INFINITY;
    let ninf = Decimal32::NEG_INFINITY;
    for (y, x) in [
        (inf, inf),
        (inf, ninf),
        (inf, parse("1")),
        (parse("1"), ninf),
        (Decimal32::ZERO, parse("-1")),
        (parse("1"), Decimal32::ZERO),
    ] {
        for rm in ALL {
            let (_, st_turns) = y.atan2_pi(x, rm);
            let (_, st_radians) = y.atan2(x, rm);
            assert_eq!(st_turns, Status::OK, "atan2Pi({y},{x}) [{rm:?}]");
            assert!(
                st_radians.inexact(),
                "atan2({y},{x}) [{rm:?}]: {st_radians:?}"
            );
        }
    }
}

/// The finite diagonals, exact and cohort insensitive.
#[test]
fn diagonals_are_exact() {
    for (y, x, want) in [
        ("1", "1", "0.25"),
        ("3", "3.0", "0.25"),
        ("1e50", "1e50", "0.25"),
        ("-1", "1", "-0.25"),
        ("1", "-1", "0.75"),
        ("-2.5", "-2.5", "-0.75"),
    ] {
        for rm in ALL {
            assert_exact(
                parse(y).atan2_pi(parse(x), rm),
                want,
                &format!("atan2Pi({y},{x}) [{rm:?}]"),
            );
        }
    }
    for (y, x) in [("1.000001", "1"), ("1", "-1.000001")] {
        for rm in ALL {
            let (_, st) = parse(y).atan2_pi(parse(x), rm);
            assert!(st.inexact(), "atan2Pi({y},{x}) [{rm:?}] flags {st:?}");
        }
    }
}

/// The `±1/2` band takes its side from the abscissa's sign.
#[test]
fn half_turn_band_takes_its_side_from_the_abscissa() {
    for gap in [HALF_GATE, HALF_GATE + 10, 50] {
        let big = parse(&format!("1e{gap}"));
        for (rm, want) in [
            (NE, HALF),
            (NA, HALF),
            (TZ, HALF_DOWN),
            (TN, HALF_DOWN),
            (TP, HALF),
        ] {
            let (r, st) = big.atan2_pi(parse("1"), rm);
            assert!(eq(r, parse(want)), "atan2Pi(1e{gap},1) [{rm:?}]: {r}");
            assert!(st.inexact(), "atan2Pi(1e{gap},1) [{rm:?}] flags {st:?}");
        }
        for (rm, want) in [
            (NE, HALF),
            (NA, HALF),
            (TZ, HALF),
            (TN, HALF),
            (TP, HALF_UP),
        ] {
            let (r, _) = big.atan2_pi(parse("-1"), rm);
            assert!(eq(r, parse(want)), "atan2Pi(1e{gap},-1) [{rm:?}]: {r}");
        }
    }
}

/// The `±1` band hugs the full turn from inside.
#[test]
fn full_turn_band_hugs_from_inside() {
    for gap in [FULL_GATE, FULL_GATE - 10, -50] {
        let tiny = parse(&format!("1e{gap}"));
        for (rm, want) in [
            (NE, ONE),
            (NA, ONE),
            (TZ, ONE_DOWN),
            (TN, ONE_DOWN),
            (TP, ONE),
        ] {
            let (r, st) = tiny.atan2_pi(parse("-1"), rm);
            assert!(eq(r, parse(want)), "atan2Pi(1e{gap},-1) [{rm:?}]: {r}");
            assert!(st.inexact(), "atan2Pi(1e{gap},-1) [{rm:?}] flags {st:?}");
        }
        let neg = parse(&format!("-1e{gap}"));
        for (rm, want) in [
            (NE, "-1"),
            (TZ, "-0.9999999"),
            (TP, "-0.9999999"),
            (TN, "-1"),
        ] {
            let (r, _) = neg.atan2_pi(parse("-1"), rm);
            assert!(eq(r, parse(want)), "atan2Pi(-1e{gap},-1) [{rm:?}]: {r}");
        }
    }
}

/// The absent arm: the same vanishing ratio against a positive
/// abscissa is generic, not a hug.
#[test]
fn tiny_ratio_with_a_positive_abscissa_is_not_an_anchor() {
    let inv_pi = parse("0.3183099");
    for gap in [FULL_GATE, -50] {
        let y = parse(&format!("1e{gap}"));
        let (r, st) = y.atan2_pi(parse("1"), NE);
        assert!(st.inexact(), "atan2Pi(1e{gap},1) flags {st:?}");
        assert!(!r.is_zero() && !eq(r, y), "atan2Pi(1e{gap},1) = {r}");
        let (scaled, _) = y.mul(inv_pi, NE);
        let (diff, _) = r.sub(scaled, NE);
        let (rel, _) = diff.abs().div(r.abs(), NE);
        assert!(
            rel.partial_cmp(parse("1e-5")).0 == Some(Ordering::Less),
            "atan2Pi(1e{gap},1) = {r} is not y/(πx)"
        );
    }
}

/// Both bands agree with the ladder across their gates.
#[test]
fn the_two_treatments_agree_across_both_gates() {
    for (y_in, x, y_out) in [
        (
            format!("1e{HALF_GATE}"),
            "1",
            format!("1e{}", HALF_GATE - 1),
        ),
        (
            format!("1e{HALF_GATE}"),
            "-1",
            format!("1e{}", HALF_GATE - 1),
        ),
        (
            format!("1e{FULL_GATE}"),
            "-1",
            format!("1e{}", FULL_GATE + 1),
        ),
    ] {
        for rm in ALL {
            let (r_in, st_in) = parse(&y_in).atan2_pi(parse(x), rm);
            let (r_out, st_out) = parse(&y_out).atan2_pi(parse(x), rm);
            assert!(
                eq(r_in, r_out),
                "atan2Pi across the gate ({y_in} vs {y_out}, {x}) [{rm:?}]"
            );
            assert_eq!(st_in, st_out, "flags across the gate [{rm:?}]");
        }
    }
}

/// Quadrant symmetry, and the one argument kernel as witness on the
/// open first quadrant.
#[test]
fn quadrant_symmetries_and_the_one_argument_kernel() {
    for (y, x) in [("1", "2"), ("3", "1"), ("0.5", "0.25"), ("1e10", "3")] {
        let (pos, st_p) = parse(y).atan2_pi(parse(x), NE);
        let (neg, st_n) = parse(&format!("-{y}")).atan2_pi(parse(x), NE);
        assert!(eq(neg, pos.neg()), "atan2Pi(-{y},{x}) is not the negation");
        assert_eq!(st_p, st_n, "atan2Pi(±{y},{x}) flag mismatch");
    }
    for (y, x, quotient) in [("1", "2", "0.5"), ("3", "4", "0.75"), ("5", "1", "5")] {
        for rm in ALL {
            let (binary, st_b) = parse(y).atan2_pi(parse(x), rm);
            let (unary, st_u) = parse(quotient).atan_pi(rm);
            assert!(
                eq(binary, unary),
                "atan2Pi({y},{x}) [{rm:?}] = {binary} against atanPi({quotient}) = {unary}"
            );
            assert_eq!(st_b, st_u, "atan2Pi({y},{x}) [{rm:?}] flags");
        }
    }
}
