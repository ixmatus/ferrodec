//! Special value, exact result, anchor band, and flag gate for
//! `Decimal128::acos_pi` (IEEE 754-2019 §9.2 `acosPi`; ADR-0061
//! Track D D4).
//!
//! Three claims are under test here.
//!
//! **The §9.2.1 table**, every row and every direction, transcribed
//! from the two free proxies ADR-0061 names (the C23 `acospi` Annex F
//! list and the MPFR 4.2 manual). `acosPi(±0) = 1/2` is the row worth
//! staring at: the radian `acos(±0) = π/2` had to round an
//! irrational and raise `INEXACT`, while `1/2` is a grid point at
//! every precision and is delivered exactly.
//!
//! **The non terminating rationals.** `acosPi(1/2) = 1/3` and
//! `acosPi(-1/2) = 2/3` are rational with a 3 in the lowest terms
//! denominator, so they terminate in no decimal format: correctly
//! rounded, `INEXACT` in every direction, and never a tie. The pinned
//! strings are computed from the rationals by hand (see
//! [`one_third_and_two_thirds_rows`]).
//!
//! **The ADR-0051 anchor band at `1/2`.** For `adj(x) ≤ -(P + 3)` the
//! true value hugs `1/2` at a distance no finite working precision
//! separates, so the residual channel decides it from the side
//! theorem instead of the ladder. The band tests cross the gate in
//! both directions and assert the side in the directed modes, which
//! is the only place the side is observable.

#![cfg(feature = "trig-pi")]

use core::cmp::Ordering;
use ferrodec::{Decimal128, RoundingMode, Status};

const NE: RoundingMode = RoundingMode::NearestEven;
const NA: RoundingMode = RoundingMode::NearestAway;
const TZ: RoundingMode = RoundingMode::TowardZero;
const TP: RoundingMode = RoundingMode::TowardPositive;
const TN: RoundingMode = RoundingMode::TowardNegative;
const ALL: [RoundingMode; 5] = [NE, NA, TZ, TP, TN];

/// `Decimal128` precision. The anchor gate is `adj(x) ≤ -(P + 3)`,
/// re-derived here rather than copied from the kernel: an off-by-one
/// in either place then fails a test instead of passing silently.
const P: i32 = 34;
const GATE: i32 = -(P + 3);

/// `1/2` and its two neighbours at 34 digits. Both neighbours sit one
/// quantum `10^-34` away, so the rounding boundaries either side are
/// `5·10^-35` from the anchor, which is the distance the module's
/// margin table clears by a factor of 150.
const HALF: &str = "0.5";
const HALF_DOWN: &str = "0.4999999999999999999999999999999999";
const HALF_UP: &str = "0.5000000000000000000000000000000001";

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
        // acosPi(±0) = 1/2 exactly, for both zero signs.
        for x in [Decimal128::ZERO, Decimal128::NEG_ZERO] {
            let (r, st) = x.acos_pi(rm);
            assert!(eq(r, parse(HALF)), "acosPi(±0) [{rm:?}] = {r}");
            assert_eq!(st, Status::OK, "acosPi(±0) [{rm:?}] flags {st:?}");
        }

        for label in ["1.000000000000000000000000000000001", "-1.5", "2", "-1e10"] {
            let (r, st) = parse(label).acos_pi(rm);
            assert!(r.is_nan(), "acosPi({label}) [{rm:?}] = {r}");
            assert!(st.invalid(), "acosPi({label}) [{rm:?}] flags {st:?}");
        }
        for x in [Decimal128::INFINITY, Decimal128::NEG_INFINITY] {
            let (r, st) = x.acos_pi(rm);
            assert!(r.is_nan(), "acosPi(inf) [{rm:?}] = {r}");
            assert!(st.invalid(), "acosPi(inf) [{rm:?}] flags {st:?}");
        }

        let (r, st) = Decimal128::NAN.acos_pi(rm);
        assert!(r.is_nan() && st.is_ok(), "acosPi(NaN) [{rm:?}] {r} {st:?}");
        let (r, st) = Decimal128::SIGNALING_NAN.acos_pi(rm);
        assert!(
            r.is_nan() && st.invalid(),
            "acosPi(sNaN) [{rm:?}] {r} {st:?}"
        );
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
                "acosPi({label}) [{rm:?}] = {r}, want +0"
            );
            assert_eq!(st, Status::OK, "acosPi({label}) [{rm:?}] flags {st:?}");
        }
    }
    for label in ["-1", "-1.0", "-1.00000000"] {
        for rm in ALL {
            assert_exact(
                parse(label).acos_pi(rm),
                "1",
                &format!("acosPi({label}) [{rm:?}]"),
            );
        }
    }
}

/// The non terminating rational rows, pinned per direction.
///
/// `acosPi(1/2) = (π/3)/π = 1/3 = 0.333…`: the tail past 34 digits is
/// `0.333…` of a unit in the last place, BELOW the midpoint, so every
/// mode except `TowardPositive` truncates and `TowardPositive` alone
/// steps up. `acosPi(-1/2) = 2/3 = 0.666…`: the tail is `0.666…`,
/// ABOVE the midpoint, so the two nearest modes and `TowardPositive`
/// step up while `TowardZero` and `TowardNegative` truncate. Both
/// columns come from the rationals, never from the kernel.
#[test]
fn one_third_and_two_thirds_rows() {
    const THIRD_DOWN: &str = "0.3333333333333333333333333333333333";
    const THIRD_UP: &str = "0.3333333333333333333333333333333334";
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
    const TWO_THIRDS_DOWN: &str = "0.6666666666666666666666666666666666";
    const TWO_THIRDS_UP: &str = "0.6666666666666666666666666666666667";
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
    // The independent oracle: the format's own parser applied to a
    // long expansion of the same rational, rounded under the same
    // mode through a path that shares no code with the kernel. The
    // expansions are the repeating digits of 1/3 and 2/3 carried well
    // past the precision, so their truncation cannot move the 34
    // digit rounding.
    let third_long = format!("0.{}", "3".repeat(45));
    let two_thirds_long = format!("0.{}", "6".repeat(45));
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
    // Three times the 1/3 literal lands one unit in the last place
    // short of 1, exactly and representably: the pin is a rounding of
    // 1/3 and not of some neighbour.
    let (three_thirds, _) = parse(THIRD_DOWN).mul(parse("3"), NE);
    assert!(
        eq(three_thirds, parse("0.9999999999999999999999999999999999")),
        "the 1/3 literal is wrong: {three_thirds}"
    );
    // And the pair sums to a half turn exactly, which is the §9.2
    // identity acosPi(x) + acosPi(-x) = 1 read at x = 1/2.
    let (sum, _) = parse(THIRD_DOWN).add(parse(TWO_THIRDS_UP), NE);
    assert!(eq(sum, parse("1")), "1/3 + 2/3 did not compose to 1: {sum}");
}

/// The anchor band, deep inside the gate: the value hugs `1/2` from
/// below for a positive operand and from above for a negative one,
/// and only the directed modes can see which. Every row is `INEXACT`.
#[test]
fn anchor_band_sides_are_the_operand_sign() {
    for k in [P + 3, P + 10, 100, 3000, 6100] {
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

/// The two treatments agree across the gate. An operand at
/// `adj(x) = GATE` takes the residual channel and one at
/// `adj(x) = GATE + 1` takes the ladder; both must deliver the same
/// bits and the same flags in every direction, which is what licenses
/// the channel short-circuiting the ladder at all.
#[test]
fn the_two_treatments_agree_across_the_gate() {
    let inside = parse(&format!("1e{GATE}"));
    let outside = parse(&format!("1e{}", GATE + 1));
    for rm in ALL {
        let (r_in, st_in) = inside.acos_pi(rm);
        let (r_out, st_out) = outside.acos_pi(rm);
        assert!(
            eq(r_in, r_out),
            "acosPi across the gate [{rm:?}]: {r_in} inside vs {r_out} outside"
        );
        assert_eq!(st_in, st_out, "acosPi across the gate [{rm:?}]: flags");
    }
    // And the same on the negative side.
    let inside = parse(&format!("-1e{GATE}"));
    let outside = parse(&format!("-1e{}", GATE + 1));
    for rm in ALL {
        let (r_in, _) = inside.acos_pi(rm);
        let (r_out, _) = outside.acos_pi(rm);
        assert!(
            eq(r_in, r_out),
            "acosPi across the gate, negative [{rm:?}]: {r_in} vs {r_out}"
        );
    }
}

/// The range is `[0, 1]` and `acosPi` is strictly decreasing: checked
/// on a deterministic sweep that crosses the domain, including both
/// exact endpoints.
#[test]
fn range_and_monotonicity() {
    let mut previous: Option<Decimal128> = None;
    for step in 0..=40i32 {
        let x = parse(&format!("{}", (step - 20) as f64 / 20.0));
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
    let pi = parse("3.141592653589793238462643383279503");
    for label in ["0.5", "-0.5", "0.25", "0.9", "-0.75", "1e-5"] {
        let x = parse(label);
        let (turns, _) = x.acos_pi(NE);
        let (radians, _) = x.acos(NE);
        let (rescaled, _) = turns.mul(pi, NE);
        let (diff, _) = rescaled.sub(radians, NE);
        let (rel, _) = diff.abs().div(radians.abs(), NE);
        assert!(
            rel.partial_cmp(parse("1e-32")).0 == Some(Ordering::Less),
            "acosPi({label})·π = {rescaled} against acos({label}) = {radians}"
        );
    }
}
