//! Special value, exact result, and no anchor gate for
//! `Decimal64::asin_pi` (IEEE 754-2019 §9.2 `asinPi`; ADR-0061
//! Track D D4). The `Decimal128` sibling
//! (`tests/transcend_asinpi.rs` in the parent crate) carries the full
//! rationale; this file is the same gate at `P = 16`.
//!
//! Re-deriving the format specific literals rather than scaling the
//! `Decimal128` ones is deliberate: ADR-0060's named failure mode is
//! a constant bookkeeping error, and a per format derivation is what
//! catches one. At `P = 16` the correctly rounded `1/6` is
//! `0.1666666666666667` (the tail past sixteen digits is `0.666…` of
//! a unit in the last place, above the midpoint, so the two nearest
//! modes and `TowardPositive` step up) and its truncation is
//! `0.1666666666666666`.

#![cfg(feature = "trig-pi")]

use core::cmp::Ordering;
use ferrodec_decimal64::{Decimal64, RoundingMode, Status};

const NE: RoundingMode = RoundingMode::NearestEven;
const NA: RoundingMode = RoundingMode::NearestAway;
const TZ: RoundingMode = RoundingMode::TowardZero;
const TP: RoundingMode = RoundingMode::TowardPositive;
const TN: RoundingMode = RoundingMode::TowardNegative;
const ALL: [RoundingMode; 5] = [NE, NA, TZ, TP, TN];

const P: usize = 16;
const SIXTH_UP: &str = "0.1666666666666667";
const SIXTH_DOWN: &str = "0.1666666666666666";

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

/// IEEE 754-2019 §9.2.1, every row, every rounding direction.
#[test]
fn specials_per_section_9_2_1() {
    for rm in ALL {
        let (r, st) = Decimal64::ZERO.asin_pi(rm);
        assert!(r.is_zero() && !r.is_sign_negative(), "asinPi(+0) [{rm:?}]");
        assert_eq!(st, Status::OK, "asinPi(+0) [{rm:?}] flags");
        let (r, st) = Decimal64::NEG_ZERO.asin_pi(rm);
        assert!(r.is_zero() && r.is_sign_negative(), "asinPi(-0) [{rm:?}]");
        assert_eq!(st, Status::OK, "asinPi(-0) [{rm:?}] flags");

        for label in ["1.000000000000001", "-1.5", "2", "-1e10"] {
            let (r, st) = parse(label).asin_pi(rm);
            assert!(r.is_nan(), "asinPi({label}) [{rm:?}] = {r}");
            assert!(st.invalid(), "asinPi({label}) [{rm:?}] flags {st:?}");
        }
        for x in [Decimal64::INFINITY, Decimal64::NEG_INFINITY] {
            let (r, st) = x.asin_pi(rm);
            assert!(
                r.is_nan() && st.invalid(),
                "asinPi(inf) [{rm:?}] {r} {st:?}"
            );
        }
        let (r, st) = Decimal64::NAN.asin_pi(rm);
        assert!(r.is_nan() && st.is_ok(), "asinPi(NaN) [{rm:?}] {r} {st:?}");
        let (r, st) = Decimal64::SIGNALING_NAN.asin_pi(rm);
        assert!(r.is_nan() && st.invalid(), "asinPi(sNaN) [{rm:?}]");
    }
}

/// `asinPi(±1) = ±1/2`, exact, flagless, cohort insensitive.
#[test]
fn unit_rows_are_exact_across_cohorts() {
    let mut wide = String::from("1.");
    wide.push_str(&"0".repeat(P - 1));
    for (label, want) in [
        ("1", "0.5"),
        ("1.0", "0.5"),
        (wide.as_str(), "0.5"),
        ("-1", "-0.5"),
        ("-1.000", "-0.5"),
    ] {
        for rm in ALL {
            let (r, st) = parse(label).asin_pi(rm);
            assert!(eq(r, parse(want)), "asinPi({label}) [{rm:?}] = {r}");
            assert_eq!(st, Status::OK, "asinPi({label}) [{rm:?}] flags {st:?}");
        }
    }
}

/// The non terminating rational row at `P = 16`, pinned per
/// direction and cross checked against the format's own parser
/// applied to a long expansion of `1/6`.
#[test]
fn one_sixth_rows() {
    for (rm, want) in [
        (NE, SIXTH_UP),
        (NA, SIXTH_UP),
        (TZ, SIXTH_DOWN),
        (TP, SIXTH_UP),
        (TN, SIXTH_DOWN),
    ] {
        let (r, st) = parse("0.5").asin_pi(rm);
        assert!(eq(r, parse(want)), "asinPi(0.5) [{rm:?}]: got {r}");
        assert!(st.inexact(), "asinPi(0.5) [{rm:?}] must be INEXACT");
    }
    let sixth_long = format!("0.1{}", "6".repeat(40));
    for rm in ALL {
        let (r, _) = parse("0.5").asin_pi(rm);
        assert!(
            eq(r, parse_rm(&sixth_long, rm)),
            "asinPi(0.5) [{rm:?}] disagrees with the parsed 1/6 expansion"
        );
    }
    // Six times the truncation is four units in the last place short
    // of 1, exactly and representably at this width.
    let (six_times, _) = parse(SIXTH_DOWN).mul(parse("6"), NE);
    assert!(
        eq(six_times, parse("0.9999999999999996")),
        "the truncated literal is not 1/6 truncated: {six_times}"
    );
    // The negative row is the odd reflection, with the two directed
    // modes swapping roles.
    for (rm, want) in [
        (NE, "-0.1666666666666667"),
        (TZ, "-0.1666666666666666"),
        (TP, "-0.1666666666666666"),
        (TN, "-0.1666666666666667"),
    ] {
        let (r, _) = parse("-0.5").asin_pi(rm);
        assert!(eq(r, parse(want)), "asinPi(-0.5) [{rm:?}]: got {r}");
    }
}

/// `asinPi` is odd, bit for bit.
#[test]
fn odd_symmetry_is_bitwise() {
    for label in ["0.5", "0.25", "1", "0.9", "1e-20"] {
        let (pos, st_p) = parse(label).asin_pi(NE);
        let (neg, st_n) = parse(&format!("-{label}")).asin_pi(NE);
        assert!(
            eq(neg, pos.neg()),
            "asinPi(-{label}) is not -asinPi({label})"
        );
        assert_eq!(st_p, st_n, "asinPi(±{label}) flag mismatch");
    }
}

/// The tiny end is generic (slope `1/π`), never stuck at the input.
#[test]
fn tiny_arguments_are_generic() {
    let inv_pi = parse("0.3183098861837907");
    for label in ["1e-20", "1e-100", "3e-380"] {
        let x = parse(label);
        for rm in ALL {
            let (r, st) = x.asin_pi(rm);
            assert!(st.inexact(), "asinPi({label}) [{rm:?}] flags {st:?}");
            assert!(!eq(r, x), "asinPi({label}) [{rm:?}] stuck at the input");
        }
        let (r, _) = x.asin_pi(NE);
        let (scaled, _) = x.mul(inv_pi, NE);
        let (diff, _) = r.sub(scaled, NE);
        let (rel, _) = diff.abs().div(r.abs(), NE);
        assert!(
            rel.partial_cmp(parse("1e-14")).0 == Some(Ordering::Less),
            "asinPi({label}) = {r} is not x/π (relative gap {rel})"
        );
    }
}

/// No anchor at `±1` either: at `P = 16` the closest representable
/// input below 1 puts the value `sqrt(2·10^-16)/π ≈ 4.5·10^-9` below
/// `1/2`, seven decades above the quantum `10^-16`. The square root
/// scale keeps halving the format's decades, which is why the claim
/// holds at every precision and not just the widest.
#[test]
fn inputs_next_to_one_stay_decades_from_the_half_turn() {
    let just_below = format!("0.{}", "9".repeat(P));
    let x = parse(&just_below);
    let (r, st) = x.asin_pi(NE);
    assert!(st.inexact(), "asinPi(1-ulp) flags {st:?}");
    assert!(!eq(r, parse("0.5")), "asinPi(1-ulp) collapsed onto 1/2");
    let (gap, _) = parse("0.5").sub(r, NE);
    assert!(
        gap.partial_cmp(parse("1e-10")).0 == Some(Ordering::Greater)
            && gap.partial_cmp(parse("1e-7")).0 == Some(Ordering::Less),
        "the gap {gap} is not at the square root scale"
    );
    for rm in ALL {
        let (r, _) = x.asin_pi(rm);
        assert!(
            r.partial_cmp(parse("0.5")).0 == Some(Ordering::Less),
            "asinPi(1-ulp) [{rm:?}] = {r} reached 1/2"
        );
    }
}

/// The metamorphic row: `asinPi(x) + acosPi(x) = 1/2`.
#[test]
fn asin_pi_plus_acos_pi_is_a_half_turn() {
    for label in ["0", "0.5", "-0.5", "1", "-1", "0.25", "-0.9", "1e-20"] {
        let x = parse(label);
        let (a, _) = x.asin_pi(NE);
        let (b, _) = x.acos_pi(NE);
        let (sum, _) = a.add(b, NE);
        let (diff, _) = sum.sub(parse("0.5"), NE);
        assert!(
            diff.abs().partial_cmp(parse("1e-14")).0 == Some(Ordering::Less),
            "asinPi({label}) + acosPi({label}) = {sum}, off by {diff}"
        );
    }
}

/// The radian kernel, scaled, as the independent witness.
#[test]
#[cfg(feature = "trig")]
fn agrees_with_the_radian_kernel_scaled() {
    let pi = parse("3.141592653589793");
    for label in ["0.5", "-0.5", "0.25", "0.9", "0.1"] {
        let x = parse(label);
        let (turns, _) = x.asin_pi(NE);
        let (radians, _) = x.asin(NE);
        let (rescaled, _) = turns.mul(pi, NE);
        let (diff, _) = rescaled.sub(radians, NE);
        let (rel, _) = diff.abs().div(radians.abs(), NE);
        assert!(
            rel.partial_cmp(parse("1e-14")).0 == Some(Ordering::Less),
            "asinPi({label})·π = {rescaled} against asin({label}) = {radians}"
        );
    }
}
