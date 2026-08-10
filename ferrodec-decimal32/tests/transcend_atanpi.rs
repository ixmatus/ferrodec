//! Special value, exact result, and anchor band gate for
//! `Decimal32::atan_pi` (IEEE 754-2019 §9.2 `atanPi`; ADR-0061
//! Track D D4). The `Decimal128` sibling
//! (`tests/transcend_atanpi.rs` in the parent crate) carries the full
//! rationale; this file is the same gate at `P = 7`, where the
//! anchor gate is `adj(x) ≥ P + 2 = 9` and the predecessor of `1/2`
//! is `0.4999999`.
//!
//! Re-deriving the format specific constants rather than scaling the
//! `Decimal128` ones is deliberate: ADR-0060's named failure mode is
//! a constant bookkeeping error, and a per format derivation is what
//! catches one.

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
const GATE: i32 = P + 2;
const HALF: &str = "0.5";
const HALF_DOWN: &str = "0.4999999";
const NEG_HALF: &str = "-0.5";
const NEG_HALF_DOWN: &str = "-0.4999999";

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
    for rm in ALL {
        let (r, st) = Decimal32::ZERO.atan_pi(rm);
        assert!(r.is_zero() && !r.is_sign_negative(), "atanPi(+0) [{rm:?}]");
        assert_eq!(st, Status::OK, "atanPi(+0) [{rm:?}] flags");
        let (r, st) = Decimal32::NEG_ZERO.atan_pi(rm);
        assert!(r.is_zero() && r.is_sign_negative(), "atanPi(-0) [{rm:?}]");
        assert_eq!(st, Status::OK, "atanPi(-0) [{rm:?}] flags");

        assert_exact(
            Decimal32::INFINITY.atan_pi(rm),
            HALF,
            &format!("atanPi(+inf) [{rm:?}]"),
        );
        assert_exact(
            Decimal32::NEG_INFINITY.atan_pi(rm),
            NEG_HALF,
            &format!("atanPi(-inf) [{rm:?}]"),
        );

        let (r, st) = Decimal32::NAN.atan_pi(rm);
        assert!(r.is_nan() && st.is_ok(), "atanPi(NaN) [{rm:?}]");
        let (r, st) = Decimal32::SIGNALING_NAN.atan_pi(rm);
        assert!(r.is_nan() && st.invalid(), "atanPi(sNaN) [{rm:?}]");
    }
}

/// `atanPi(±1) = ±1/4`, exact and cohort insensitive; one ulp off is
/// not exact.
#[test]
fn unit_rows_are_exact_across_cohorts() {
    for (label, want) in [
        ("1", "0.25"),
        ("1.0", "0.25"),
        ("1.000000", "0.25"),
        ("-1", "-0.25"),
        ("-1.0", "-0.25"),
    ] {
        for rm in ALL {
            assert_exact(
                parse(label).atan_pi(rm),
                want,
                &format!("atanPi({label}) [{rm:?}]"),
            );
        }
    }
    for label in ["1.000001", "0.9999999"] {
        for rm in ALL {
            let (_, st) = parse(label).atan_pi(rm);
            assert!(st.inexact(), "atanPi({label}) [{rm:?}] flags {st:?}");
        }
    }
}

/// The anchor band: `|atanPi| < 1/2` strictly, so the magnitude
/// shrinking modes step off the anchor and the rest stay on it.
#[test]
fn anchor_band_hugs_the_half_turn_from_inside() {
    for k in [GATE, GATE + 4, 50, 95] {
        let x = parse(&format!("1e{k}"));
        for (rm, want) in [
            (NE, HALF),
            (NA, HALF),
            (TZ, HALF_DOWN),
            (TN, HALF_DOWN),
            (TP, HALF),
        ] {
            let (r, st) = x.atan_pi(rm);
            assert!(eq(r, parse(want)), "atanPi(1e{k}) [{rm:?}]: got {r}");
            assert!(st.inexact(), "atanPi(1e{k}) [{rm:?}] flags {st:?}");
        }
        let neg = parse(&format!("-1e{k}"));
        for (rm, want) in [
            (NE, NEG_HALF),
            (NA, NEG_HALF),
            (TZ, NEG_HALF_DOWN),
            (TP, NEG_HALF_DOWN),
            (TN, NEG_HALF),
        ] {
            let (r, st) = neg.atan_pi(rm);
            assert!(eq(r, parse(want)), "atanPi(-1e{k}) [{rm:?}]: got {r}");
            assert!(st.inexact(), "atanPi(-1e{k}) [{rm:?}] flags {st:?}");
        }
    }
}

/// The residual channel and the ladder agree across the gate.
#[test]
fn the_two_treatments_agree_across_the_gate() {
    for sign in ["", "-"] {
        let inside = parse(&format!("{sign}1e{GATE}"));
        let outside = parse(&format!("{sign}1e{}", GATE - 1));
        for rm in ALL {
            let (r_in, st_in) = inside.atan_pi(rm);
            let (r_out, st_out) = outside.atan_pi(rm);
            assert!(
                eq(r_in, r_out),
                "atanPi across the gate [{sign}, {rm:?}]: {r_in} vs {r_out}"
            );
            assert_eq!(st_in, st_out, "flags across the gate [{sign}, {rm:?}]");
        }
    }
}

/// `atanPi` is odd, bit for bit.
#[test]
fn odd_symmetry_is_bitwise() {
    for label in ["1", "0.5", "3", "1e20", "1e-20"] {
        let (pos, st_p) = parse(label).atan_pi(NE);
        let (neg, st_n) = parse(&format!("-{label}")).atan_pi(NE);
        assert!(
            eq(neg, pos.neg()),
            "atanPi(-{label}) is not -atanPi({label})"
        );
        assert_eq!(st_p, st_n, "atanPi(±{label}) flag mismatch");
    }
}

/// The tiny end is generic (slope `1/π`), never stuck at the input.
#[test]
fn tiny_arguments_are_generic() {
    let inv_pi = parse("0.3183099");
    for label in ["1e-20", "1e-50", "7e-95"] {
        let x = parse(label);
        for rm in ALL {
            let (r, st) = x.atan_pi(rm);
            assert!(st.inexact(), "atanPi({label}) [{rm:?}] flags {st:?}");
            assert!(!eq(r, x), "atanPi({label}) [{rm:?}] stuck at the input");
        }
        let (r, _) = x.atan_pi(NE);
        let (scaled, _) = x.mul(inv_pi, NE);
        let (diff, _) = r.sub(scaled, NE);
        let (rel, _) = diff.abs().div(r.abs(), NE);
        assert!(
            rel.partial_cmp(parse("1e-5")).0 == Some(Ordering::Less),
            "atanPi({label}) = {r} is not x/π (relative gap {rel})"
        );
    }
}

/// The flag difference against the radian spelling.
#[test]
#[cfg(feature = "trig")]
fn infinity_rows_are_exact_where_atan_was_inexact() {
    for x in [Decimal32::INFINITY, Decimal32::NEG_INFINITY] {
        for rm in ALL {
            let (_, st_turns) = x.atan_pi(rm);
            let (_, st_radians) = x.atan(rm);
            assert_eq!(st_turns, Status::OK, "atanPi(±inf) [{rm:?}]");
            assert!(st_radians.inexact(), "atan(±inf) [{rm:?}]: {st_radians:?}");
        }
    }
}

/// The radian kernel, scaled, as the independent witness.
#[test]
#[cfg(feature = "trig")]
fn agrees_with_the_radian_kernel_scaled() {
    let pi = parse("3.141593");
    for label in ["1", "-1", "0.5", "2", "10", "0.001"] {
        let x = parse(label);
        let (turns, _) = x.atan_pi(NE);
        let (radians, _) = x.atan(NE);
        let (rescaled, _) = turns.mul(pi, NE);
        let (diff, _) = rescaled.sub(radians, NE);
        let (rel, _) = diff.abs().div(radians.abs(), NE);
        assert!(
            rel.partial_cmp(parse("1e-5")).0 == Some(Ordering::Less),
            "atanPi({label})·π = {rescaled} against atan({label}) = {radians}"
        );
    }
}
