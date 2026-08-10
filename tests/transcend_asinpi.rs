//! Special value, exact result, flag, and no anchor gate for
//! `Decimal128::asin_pi` (IEEE 754-2019 §9.2 `asinPi`; ADR-0061
//! Track D D4).
//!
//! Three claims are under test here.
//!
//! **The §9.2.1 table**, every row and every rounding direction. The
//! rows come from the standard as the two free proxies transcribe it
//! (the C23 `asinpi` Annex F list and the MPFR 4.2 manual), which
//! agree with each other and with the mathematical necessity that the
//! domain is `[-1, 1]`.
//!
//! **The exact set is exactly `{±0 → ±0, ±1 → ±1/2}`**, and in
//! particular `asinPi(±1/2) = ±1/6` is NOT in it. `1/6` is rational
//! but its lowest terms denominator carries a 3, so it terminates in
//! no decimal format: the result is the correctly rounded neighbour
//! with `INEXACT` in every direction. The pinned strings below are
//! computed from the rational `1/6` by hand (see the comment on
//! [`one_sixth_rows`]), never from the kernel.
//!
//! **No anchor arm, at either end.** ADR-0061's closed list gives
//! `asinPi` no ADR-0051 residual channel, and the two tests
//! [`tiny_arguments_are_generic`] and
//! [`inputs_next_to_one_stay_decades_from_the_half_turn`] are the
//! executable form of that claim: slope `1/π` at 0, square root scale
//! at `±1`, and so no grid point close enough to hug.

#![cfg(feature = "trig-pi")]

use core::cmp::Ordering;
use ferrodec::{Decimal128, RoundingMode, Status};

const NE: RoundingMode = RoundingMode::NearestEven;
const NA: RoundingMode = RoundingMode::NearestAway;
const TZ: RoundingMode = RoundingMode::TowardZero;
const TP: RoundingMode = RoundingMode::TowardPositive;
const TN: RoundingMode = RoundingMode::TowardNegative;
const ALL: [RoundingMode; 5] = [NE, NA, TZ, TP, TN];

/// `Decimal128` precision, the width every pinned literal below is
/// spelled at.
const P: usize = 34;

fn parse(s: &str) -> Decimal128 {
    Decimal128::parse_str(s, NE)
        .unwrap_or_else(|_| panic!("test literal parses: {s:?}"))
        .0
}

/// The format's own decimal parser under an explicit rounding mode:
/// the independent oracle for a value spelled out exactly, sharing no
/// code with the transcendental kernel.
fn parse_rm(s: &str, rm: RoundingMode) -> Decimal128 {
    Decimal128::parse_str(s, rm)
        .unwrap_or_else(|_| panic!("test literal parses: {s:?}"))
        .0
}

fn eq(got: Decimal128, want: Decimal128) -> bool {
    got.partial_cmp(want).0 == Some(Ordering::Equal)
}

fn assert_exact(got: (Decimal128, Status), want: &str, label: &str) {
    let (r, st) = got;
    assert!(eq(r, parse(want)), "{label}: got {r}, want {want}");
    assert_eq!(st, Status::OK, "{label}: §7.5 forbids a flag here");
}

/// IEEE 754-2019 §9.2.1, every row, every rounding direction.
#[test]
fn specials_per_section_9_2_1() {
    for rm in ALL {
        // asinPi(±0) = ±0, exact, sign preserved.
        let (r, st) = Decimal128::ZERO.asin_pi(rm);
        assert!(r.is_zero() && !r.is_sign_negative(), "asinPi(+0) [{rm:?}]");
        assert_eq!(st, Status::OK, "asinPi(+0) [{rm:?}] flags");
        let (r, st) = Decimal128::NEG_ZERO.asin_pi(rm);
        assert!(r.is_zero() && r.is_sign_negative(), "asinPi(-0) [{rm:?}]");
        assert_eq!(st, Status::OK, "asinPi(-0) [{rm:?}] flags");

        // Outside the domain, and the infinities with it.
        for label in ["1.000000000000000000000000000000001", "-1.5", "2", "-1e10"] {
            let (r, st) = parse(label).asin_pi(rm);
            assert!(r.is_nan(), "asinPi({label}) [{rm:?}] = {r}");
            assert!(st.invalid(), "asinPi({label}) [{rm:?}] flags {st:?}");
        }
        for x in [Decimal128::INFINITY, Decimal128::NEG_INFINITY] {
            let (r, st) = x.asin_pi(rm);
            assert!(r.is_nan(), "asinPi(inf) [{rm:?}] = {r}");
            assert!(st.invalid(), "asinPi(inf) [{rm:?}] flags {st:?}");
        }

        // NaN propagation; a signaling NaN quiets and raises INVALID.
        let (r, st) = Decimal128::NAN.asin_pi(rm);
        assert!(r.is_nan() && st.is_ok(), "asinPi(NaN) [{rm:?}] {r} {st:?}");
        let (r, st) = Decimal128::SIGNALING_NAN.asin_pi(rm);
        assert!(
            r.is_nan() && st.invalid(),
            "asinPi(sNaN) [{rm:?}] {r} {st:?}"
        );
    }
}

/// `asinPi(±1) = ±1/2`, exact and flagless in every direction, and
/// cohort insensitive: the classifier decides on stripped parts, so
/// `1`, `1.0` and `1.000…` are one input.
#[test]
fn unit_rows_are_exact_across_cohorts() {
    let mut wide = String::from("1.");
    wide.push_str(&"0".repeat(P - 1));
    for (label, want) in [
        ("1", "0.5"),
        ("1.0", "0.5"),
        ("1.00000000", "0.5"),
        (wide.as_str(), "0.5"),
        ("-1", "-0.5"),
        ("-1.0", "-0.5"),
        ("-1.00000000", "-0.5"),
    ] {
        for rm in ALL {
            assert_exact(
                parse(label).asin_pi(rm),
                want,
                &format!("asinPi({label}) [{rm:?}]"),
            );
        }
    }
}

/// The non terminating rational rows, pinned per direction.
///
/// `asinPi(1/2) = (π/6)/π = 1/6` exactly as a rational, so the
/// correctly rounded 34 digit results are the roundings of
/// `1/6 = 0.1666…` with the digit sequence `1` then thirty-two `6`s
/// then a truncated `6…` tail: the tail is `0.666…` of a unit in the
/// last place, above the midpoint, so every mode that rounds up (both
/// nearest modes and `TowardPositive` on a positive value) lands on
/// `…667` and the two that round down land on `…666`. The negative
/// row is the odd reflection, with `TowardPositive` and
/// `TowardNegative` swapping roles. None of these numbers comes from
/// the kernel; they come from the rational `1/6` and the definition
/// of the rounding directions.
#[test]
fn one_sixth_rows() {
    const UP: &str = "0.1666666666666666666666666666666667";
    const DOWN: &str = "0.1666666666666666666666666666666666";
    for (rm, want) in [(NE, UP), (NA, UP), (TZ, DOWN), (TP, UP), (TN, DOWN)] {
        let (r, st) = parse("0.5").asin_pi(rm);
        assert!(eq(r, parse(want)), "asinPi(0.5) [{rm:?}]: got {r}");
        assert!(st.inexact(), "asinPi(0.5) [{rm:?}] must be INEXACT");
    }
    for (rm, want) in [
        (NE, "-0.1666666666666666666666666666666667"),
        (NA, "-0.1666666666666666666666666666666667"),
        (TZ, "-0.1666666666666666666666666666666666"),
        (TP, "-0.1666666666666666666666666666666666"),
        (TN, "-0.1666666666666666666666666666666667"),
    ] {
        let (r, st) = parse("-0.5").asin_pi(rm);
        assert!(eq(r, parse(want)), "asinPi(-0.5) [{rm:?}]: got {r}");
        assert!(st.inexact(), "asinPi(-0.5) [{rm:?}] must be INEXACT");
    }
    // The literal really is the rounding of one sixth: six times the
    // truncated value is four units in the last place short of 1,
    // exactly and representably, which is what "the tail is 0.666…
    // ulp" means read back through the format itself.
    let (six_times, _) = parse(DOWN).mul(parse("6"), NE);
    assert!(
        eq(six_times, parse("0.9999999999999999999999999999999996")),
        "the truncated literal is not 1/6 truncated: {six_times}"
    );
    // And the independent oracle: the format's own parser applied to
    // a long expansion of 1/6 (the digit 1 then a repeating 6),
    // rounded under the same mode through a path that shares no code
    // with the kernel.
    let sixth_long = format!("0.1{}", "6".repeat(44));
    for rm in ALL {
        let (r, _) = parse("0.5").asin_pi(rm);
        assert!(
            eq(r, parse_rm(&sixth_long, rm)),
            "asinPi(0.5) [{rm:?}] disagrees with the parsed 1/6 expansion"
        );
    }
}

/// `asinPi` is odd, bit for bit.
#[test]
fn odd_symmetry_is_bitwise() {
    for label in ["0.5", "0.25", "1", "0.9", "1e-20", "0.7071067811865475"] {
        for rm in [NE, NA] {
            let (pos, st_p) = parse(label).asin_pi(rm);
            let (neg, st_n) = parse(&format!("-{label}")).asin_pi(rm);
            assert!(
                eq(neg, pos.neg()),
                "asinPi(-{label}) [{rm:?}] is not -asinPi({label}): {neg} vs {pos}"
            );
            assert_eq!(st_p, st_n, "asinPi(±{label}) [{rm:?}] flag mismatch");
        }
    }
}

/// The tiny end is generic, not anchored: the value is `x/π`, which
/// is neither `x` (the parent `asin`'s `sticks_to` anchor) nor a grid
/// point, and it is `INEXACT` in every direction.
#[test]
fn tiny_arguments_are_generic() {
    let inv_pi = parse("0.3183098861837906715377675267450287");
    for label in ["1e-20", "1e-100", "3e-2000", "1e-6100"] {
        let x = parse(label);
        for rm in ALL {
            let (r, st) = x.asin_pi(rm);
            assert!(st.inexact(), "asinPi({label}) [{rm:?}] flags {st:?}");
            assert!(!eq(r, x), "asinPi({label}) [{rm:?}] stuck at the input");
        }
        // And it is the expected `x/π`, to the precision a single
        // format multiply can confirm.
        let (r, _) = x.asin_pi(NE);
        let (scaled, _) = x.mul(inv_pi, NE);
        let (diff, _) = r.sub(scaled, NE);
        let (rel, _) = diff.abs().div(r.abs(), NE);
        assert!(
            rel.partial_cmp(parse("1e-30")).0 == Some(Ordering::Less),
            "asinPi({label}) = {r} is not x/π (relative gap {rel})"
        );
    }
}

/// The `±1` end has no anchor either, and this is why: the closest
/// representable input below 1 already puts the value `sqrt(2·10^-34)/π`
/// below `1/2`, some `10^16` ulps away, because the square root scale
/// halves the decades. A residual channel here would be answering a
/// question nobody asked.
#[test]
fn inputs_next_to_one_stay_decades_from_the_half_turn() {
    let mut just_below = String::from("0.");
    just_below.push_str(&"9".repeat(P));
    let x = parse(&just_below);
    let (r, st) = x.asin_pi(NE);
    assert!(st.inexact(), "asinPi(1-ulp) flags {st:?}");
    assert!(!eq(r, parse("0.5")), "asinPi(1-ulp) collapsed onto 1/2");
    let (gap, _) = parse("0.5").sub(r, NE);
    // sqrt(2e-34)/π ≈ 4.5e-18: bracket it two decades either side.
    assert!(
        gap.partial_cmp(parse("1e-19")).0 == Some(Ordering::Greater),
        "the gap {gap} is nearer 1/2 than the square root scale allows"
    );
    assert!(
        gap.partial_cmp(parse("1e-16")).0 == Some(Ordering::Less),
        "the gap {gap} is further from 1/2 than the square root scale allows"
    );
    // Every direction agrees the value is below 1/2, since asinPi is
    // strictly increasing and asinPi(1) = 1/2 exactly.
    for rm in ALL {
        let (r, _) = x.asin_pi(rm);
        assert!(
            r.partial_cmp(parse("0.5")).0 == Some(Ordering::Less),
            "asinPi(1-ulp) [{rm:?}] = {r} reached 1/2"
        );
    }
}

/// The metamorphic row ADR-0061 names: `asinPi(x) + acosPi(x) = 1/2`
/// for every `x` in the domain. A smoke property at loose tolerance,
/// since both sides are separately rounded, except at `x = ±1/2`
/// where the two roundings compose exactly (`1/6 + 1/3 = 1/2` with
/// the up and down roundings cancelling).
#[test]
fn asin_pi_plus_acos_pi_is_a_half_turn() {
    for label in [
        "0", "0.5", "-0.5", "1", "-1", "0.25", "-0.9", "1e-20", "0.999999",
    ] {
        let x = parse(label);
        let (a, _) = x.asin_pi(NE);
        let (b, _) = x.acos_pi(NE);
        let (sum, _) = a.add(b, NE);
        let (diff, _) = sum.sub(parse("0.5"), NE);
        assert!(
            diff.abs().partial_cmp(parse("1e-32")).0 == Some(Ordering::Less),
            "asinPi({label}) + acosPi({label}) = {sum}, off by {diff}"
        );
    }
    let (a, _) = parse("0.5").asin_pi(NE);
    let (b, _) = parse("0.5").acos_pi(NE);
    let (sum, _) = a.add(b, NE);
    assert!(
        eq(sum, parse("0.5")),
        "1/6 + 1/3 did not compose exactly: {sum}"
    );
}

/// The radian kernel, scaled, is an independent witness: it shares
/// this crate's `asin` core but none of its §9.2.1 table, classifier,
/// or budget, so a disagreement past a few ulps means one of those
/// three is wrong.
#[test]
#[cfg(feature = "trig")]
fn agrees_with_the_radian_kernel_scaled() {
    let pi = parse("3.141592653589793238462643383279503");
    for label in ["0.5", "-0.5", "0.25", "0.9", "0.1", "1e-5", "-0.75"] {
        let x = parse(label);
        let (turns, _) = x.asin_pi(NE);
        let (radians, _) = x.asin(NE);
        let (rescaled, _) = turns.mul(pi, NE);
        let (diff, _) = rescaled.sub(radians, NE);
        let (rel, _) = diff.abs().div(radians.abs(), NE);
        assert!(
            rel.partial_cmp(parse("1e-32")).0 == Some(Ordering::Less),
            "asinPi({label})·π = {rescaled} against asin({label}) = {radians}"
        );
    }
}
