//! Exact-result, special-value, and metamorphic gate for
//! `Decimal64::sin_pi` (IEEE 754-2019 §9.2 `sinPi`; ADR-0061 Track D
//! group D4).
//!
//! The classifier `ferrodec_transcend::exact_pi::sinpi_exact` claims a
//! complete exact set: the integers (value `±0`, carrying the operand's
//! sign per §9.2.1's odd-function rule) and the half integers (value
//! `(−1)^n`). Niven's theorem closes it, because the only rational
//! values of `sin(πr)` are `{0, ±1/2, ±1}` and the `±1/2` rows need the
//! abscissas `k ± 1/6`, which no decimal format represents. This file
//! is the claim's witness at 16 digits.
//!
//! `sinPi` takes no ADR-0051 anchor arm even though it hugs `±1` near
//! the half integers, and [`half_integer_hug_needs_no_anchor`] is the
//! executable form of the reason: a half integer has magnitude at least
//! `1/2`, so the stored quantum forces the offset to `10^-16` or more
//! and the hug bottoms out around `4.9·10^-32`, decades outside every
//! rung's budget. Only a neighborhood containing zero escapes that
//! floor, which is why `cosPi`'s integers need a channel and these do
//! not.

#![cfg(feature = "trig-pi")]

use core::cmp::Ordering;
use ferrodec_decimal64::{Decimal64, RoundingMode, Status};

const NE: RoundingMode = RoundingMode::NearestEven;
const NA: RoundingMode = RoundingMode::NearestAway;
const TZ: RoundingMode = RoundingMode::TowardZero;
const TP: RoundingMode = RoundingMode::TowardPositive;
const TN: RoundingMode = RoundingMode::TowardNegative;
const ALL: [RoundingMode; 5] = [NE, NA, TZ, TP, TN];

/// `sin(54°) = cos(36°) = φ/2`, the golden ratio halved, at 16 digits.
const SIN_54: &str = "0.8090169943749474";
/// `sin(18°) = (√5 − 1)/4` at 16 digits.
const SIN_18: &str = "0.3090169943749474";
/// `sin(45°) = √2/2` at 16 digits (the same constant
/// `ferrodec_transcend::rsqrt`'s tests carry to 130 places).
const SIN_45: &str = "0.7071067811865475";

fn parse(s: &str) -> Decimal64 {
    Decimal64::parse_str(s, NE)
        .unwrap_or_else(|_| panic!("test literal parses: {s:?}"))
        .0
}

fn eq(got: Decimal64, want: Decimal64) -> bool {
    got.partial_cmp(want).0 == Some(Ordering::Equal)
}

/// An exact delivery: the value, and §7.5's ban on `INEXACT`.
fn assert_exact(got: (Decimal64, Status), want: Decimal64, label: &str) {
    let (r, st) = got;
    assert!(eq(r, want), "{label}: got {r}, want {want}");
    assert_eq!(
        st,
        Status::OK,
        "{label}: exact result must be OK, got {st:?}"
    );
}

/// A signed-zero delivery: `partial_cmp` cannot see the sign, so the
/// zero rows check it directly.
fn assert_zero(got: (Decimal64, Status), neg: bool, label: &str) {
    let (r, st) = got;
    assert!(r.is_zero(), "{label}: got {r}, want a zero");
    assert_eq!(r.is_sign_negative(), neg, "{label}: zero sign, got {r}");
    assert_eq!(
        st,
        Status::OK,
        "{label}: exact result must be OK, got {st:?}"
    );
}

/// IEEE 754-2019 §9.2.1, every row, every rounding direction.
#[test]
fn specials_per_section_9_2_1() {
    for rm in ALL {
        // sinPi(±0) is ±0.
        assert_zero(
            Decimal64::ZERO.sin_pi(rm),
            false,
            &format!("sinPi(+0) [{rm:?}]"),
        );
        assert_zero(
            Decimal64::NEG_ZERO.sin_pi(rm),
            true,
            &format!("sinPi(-0) [{rm:?}]"),
        );

        // sinPi(±∞) is a quiet NaN with INVALID: periodic, no limit.
        for (x, label) in [
            (Decimal64::INFINITY, "+inf"),
            (Decimal64::NEG_INFINITY, "-inf"),
        ] {
            let (r, st) = x.sin_pi(rm);
            assert!(r.is_nan(), "sinPi({label}) [{rm:?}] = {r}, want NaN");
            assert!(st.invalid(), "sinPi({label}) [{rm:?}] status {st:?}");
        }

        // NaN propagation; sNaN raises INVALID and quiets.
        let (r, st) = Decimal64::NAN.sin_pi(rm);
        assert!(r.is_nan() && st.is_ok(), "sinPi(NaN) [{rm:?}] = {r} {st:?}");
        let (r, st) = Decimal64::SIGNALING_NAN.sin_pi(rm);
        assert!(
            r.is_nan() && st.invalid(),
            "sinPi(sNaN) [{rm:?}] = {r} {st:?}"
        );
    }
}

/// The integer row: `±0` carrying the OPERAND's sign, both parities,
/// both signs, and out to magnitudes whose quantum is at or above 1
/// (where every representable value is an integer and the classifier
/// owns the whole decade).
#[test]
fn exact_at_the_integers() {
    for label in [
        "0",
        "1",
        "2",
        "3",
        "4",
        "17",
        "100",
        "1000000",
        // Quantum ≥ 1: the classifier owns these by exponent alone.
        "1E+16",
        "1E+40",
        "1E+380",
        "9.999999999999999E+384",
        // A full-width odd integer.
        "9999999999999999",
    ] {
        for rm in ALL {
            assert_zero(
                parse(label).sin_pi(rm),
                false,
                &format!("sinPi({label}) [{rm:?}]"),
            );
            let neg = format!("-{label}");
            assert_zero(
                parse(&neg).sin_pi(rm),
                true,
                &format!("sinPi({neg}) [{rm:?}]"),
            );
        }
    }
}

/// The half-integer row: `(−1)^n` for `|x| = n + 1/2`, reflected
/// through the odd function. Cohort variants are one operand.
#[test]
fn exact_at_the_half_integers() {
    let one = parse("1");
    let neg_one = parse("-1");
    // (operand, value) for the positive operand; the negative operand
    // takes the mirrored value.
    for (label, want_pos) in [
        ("0.5", &one),
        ("1.5", &neg_one),
        ("2.5", &one),
        ("3.5", &neg_one),
        ("100.5", &one),
        ("101.5", &neg_one),
        // Cohort variants of 2.5 and 0.5.
        ("2.50", &one),
        ("2.500000000000000", &one),
        ("0.50", &one),
        ("250E-2", &one),
        // The widest half integer this format can spell.
        ("999999999999999.5", &neg_one),
    ] {
        for rm in ALL {
            assert_exact(
                parse(label).sin_pi(rm),
                *want_pos,
                &format!("sinPi({label}) [{rm:?}]"),
            );
            let neg = format!("-{label}");
            assert_exact(
                parse(&neg).sin_pi(rm),
                if eq(*want_pos, one) { neg_one } else { one },
                &format!("sinPi({neg}) [{rm:?}]"),
            );
        }
    }
}

/// The fifth-turn and eighth-turn values, against closed forms derived
/// independently of this kernel: `sin(π/5) = (√5 − 1)/4`,
/// `sin(3π/10) = φ/2`, `sin(π/4) = √2/2`.
#[test]
fn known_values_against_closed_forms() {
    for (label, want) in [
        ("0.1", SIN_18),
        ("0.3", SIN_54),
        ("0.25", SIN_45),
        ("0.7", SIN_54),
        ("0.9", SIN_18),
        ("0.75", SIN_45),
    ] {
        let (r, st) = parse(label).sin_pi(NE);
        assert!(eq(r, parse(want)), "sinPi({label}) = {r}, want {want}");
        assert!(st.inexact(), "sinPi({label}) is irrational: {st:?}");
    }
    // The second half turn mirrors the first.
    for (label, want) in [("1.1", SIN_18), ("1.3", SIN_54), ("1.25", SIN_45)] {
        let (r, _) = parse(label).sin_pi(NE);
        let neg_want = parse(&format!("-{want}"));
        assert!(eq(r, neg_want), "sinPi({label}) = {r}, want -{want}");
    }
}

/// Small arguments track the slope `π` rather than sticking to the
/// operand, which is the executable form of "`sinPi` is not an anchor
/// family at zero": `sin(πx) ≈ πx`, so the result must differ from the
/// input and stay `INEXACT` in every direction.
#[test]
fn small_arguments_track_the_pi_slope() {
    let pi = parse("3.141592653589793");
    for label in [
        "1E-20", "1E-40", "1E-100", "1E-200", "1E-380", "0.000001", "1E-398",
    ] {
        let x = parse(label);
        for rm in ALL {
            let (r, st) = x.sin_pi(rm);
            assert!(st.inexact(), "sinPi({label}) [{rm:?}] must be INEXACT");
            assert!(
                !eq(r, x),
                "sinPi({label}) [{rm:?}] stuck to the operand: {r}"
            );
        }
    }
    // And the value really is `π·x` to within the format's own
    // multiplication. The series' first correction is `−(πx)³/6`, a
    // relative `(πx)²/6`, so this reads only the inputs small enough
    // for that term to sit under the tolerance, and stays clear of the
    // subnormal range where the product itself would round.
    for label in ["1E-20", "1E-40", "1E-100", "1E-200"] {
        let x = parse(label);
        let (r, _) = x.sin_pi(NE);
        let (want, _) = x.mul(pi, NE);
        let (diff, _) = r.sub(want, NE);
        let (scaled, _) = diff.div(want, NE);
        assert!(
            scaled.abs().partial_cmp(parse("1E-13")).0 == Some(Ordering::Less),
            "sinPi({label}) = {r} is not π·x = {want}"
        );
    }
}

/// `sinPi` is odd, bit for bit, including on the zero and exact rows.
#[test]
fn oddness_is_bitwise() {
    for label in [
        "0.3",
        "0.7",
        "1.2",
        "1.8",
        "3.7",
        "0.25",
        "0.5",
        "1",
        "2",
        "1E-20",
        "0.0001",
        "12345.6789",
        "1E+40",
        "999999999999999.5",
    ] {
        let x = parse(label);
        let neg = parse(&format!("-{label}"));
        for rm in ALL {
            let (a, sa) = x.sin_pi(rm);
            // Negating the operand negates the result, so the directed
            // modes must be read on the reflected mode.
            let (b, sb) = neg.sin_pi(rm.for_negation());
            assert_eq!(
                a.to_bits(),
                b.neg().to_bits(),
                "sinPi(-{label}) [{rm:?}] is not -sinPi({label}): {a} vs {b}"
            );
            assert_eq!(sa, sb, "sinPi(±{label}) [{rm:?}] flags differ");
        }
    }
}

/// The half-integer neighborhood: `sinPi` hugs `±1` there, and the
/// ladder resolves it without an anchor arm because the operand's own
/// quantum bounds the offset below.
///
/// Two claims, and the offset decides which applies. The hug is
/// `(πδ)²/2`, and the last nearest-mode boundary below 1 sits at
/// `5·10^-17`, so nearest delivers exactly `1` once `δ < 3.2·10^-9`
/// and delivers a strictly smaller neighbour above that. Both regimes
/// are reachable, and in both the side theorem `|sinPi| < 1` strictly
/// off the exact set forces the toward-zero directions off the grid
/// point. The tightest offset the format can spell beside `0.5` is
/// `10^-16`, where the hug is `≈ 4.9·10^-32`: bounded well away from
/// zero, which is exactly why no anchor channel is needed.
#[test]
fn half_integer_hug_needs_no_anchor() {
    let one = parse("1");
    let just_below_one = parse("0.9999999999999999");
    // (operand, positive lobe, nearest rounds all the way to 1)
    let cases: [(&str, bool, bool); 6] = [
        ("0.5000000000000001", true, true),
        ("0.4999999999999999", true, true),
        ("0.5000000001", true, true),
        // 16 digits wide, so the offset survives the parse; a 17th
        // would round the operand back onto the exact half integer.
        ("1.500000000000001", false, true),
        // δ = 10^-8 puts the hug at 4.9·10^-16, past the boundary.
        ("0.50000001", true, false),
        ("0.49999999", true, false),
    ];
    for (label, positive_lobe, snaps_to_one) in cases {
        let x = parse(label);
        let sign = |v: Decimal64| if positive_lobe { v } else { v.neg() };
        for rm in [NE, NA] {
            let (r, st) = x.sin_pi(rm);
            assert!(st.inexact(), "sinPi({label}) [{rm:?}] must be INEXACT");
            if snaps_to_one {
                assert!(
                    eq(r, sign(one)),
                    "sinPi({label}) [{rm:?}] = {r}, want {}",
                    sign(one)
                );
            }
        }
        // The side theorem, in every regime: the magnitude is strictly
        // below 1, so rounding toward zero steps off the grid point.
        let (r, _) = x.sin_pi(TZ);
        assert!(
            r.abs().partial_cmp(one).0 == Some(Ordering::Less),
            "sinPi({label}) [TowardZero] = {r} reached the anchor"
        );
        if snaps_to_one {
            assert!(
                eq(r, sign(just_below_one)),
                "sinPi({label}) [TowardZero] = {r}, want {}",
                sign(just_below_one)
            );
        }
    }
}

/// A deterministic sweep: away from the exact set every result is
/// `INEXACT` in every direction, bounded by 1, and consistent with the
/// Pythagorean identity through the format's own arithmetic.
#[test]
fn sweep_stays_bounded_and_inexact() {
    let one = parse("1");
    let tolerance = parse("1E-13");
    let mut checked = 0usize;
    for coef in ["0.3", "0.123456789", "1.7", "2.9", "12.34", "0.000001"] {
        for shift in (-40i32..=40).step_by(3) {
            let x = parse(&format!("{coef}E{shift}"));
            if x.is_zero() || !x.is_finite() {
                continue;
            }
            let (s, st) = x.sin_pi(NE);
            // A representable value with a fractional part is never in
            // the exact set, but a shift can turn one into an integer;
            // skip those, they are the exact rows above.
            if st.is_ok() {
                continue;
            }
            for rm in ALL {
                let (_, st) = x.sin_pi(rm);
                assert!(st.inexact(), "sinPi({coef}E{shift}) [{rm:?}]: {st:?}");
            }
            assert!(
                s.abs().partial_cmp(one).0 != Some(Ordering::Greater),
                "sinPi({coef}E{shift}) = {s} left [-1, 1]"
            );
            let (c, _) = x.cos_pi(NE);
            let (ss, _) = s.mul(s, NE);
            let (cc, _) = c.mul(c, NE);
            let (sum, _) = ss.add(cc, NE);
            let (diff, _) = sum.sub(one, NE);
            assert!(
                diff.abs().partial_cmp(tolerance).0 == Some(Ordering::Less),
                "sin²+cos² at {coef}E{shift} = {sum}"
            );
            checked += 1;
        }
    }
    assert!(checked > 50, "sweep covered only {checked} inputs");
}
