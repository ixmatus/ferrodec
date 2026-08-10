//! Exact-result, special-value, anchor-band, and metamorphic gate for
//! `Decimal32::cos_pi` (IEEE 754-2019 §9.2 `cosPi`; ADR-0061 Track D
//! group D4).
//!
//! The classifier `ferrodec_transcend::exact_pi::cospi_exact` claims a
//! complete exact set: the integers (value `(−1)^n`) and the half
//! integers (value `+0` always, §9.2.1's rule that keeps the function
//! even). Niven closes it: the `±1/2` rows would need the abscissas
//! `k ± 1/3`, which no decimal format represents.
//!
//! ## The anchor band
//!
//! `cosPi` is the trio's one ADR-0051 residual channel.
//! `cos(πδ) = 1 − (πδ)²/2 + …` hugs `±1` from below, and near the
//! integer **zero** the hug is unbounded, because there `δ` is the
//! operand itself and reaches `10^-101`. The kernel gates the channel
//! at `adj(δ) ≤ −⌈(P + 4)/2⌉`, which is `−6` at this format, and
//! [`anchor_band_across_the_gate`] walks both sides of that boundary:
//! the gate is an implementation seam, so the delivered value, the
//! flags, and the side theorem must not change across it.
//!
//! Two regimes meet here and the tests separate them. The hug clears
//! the last nearest-mode boundary below 1 (`5·10^-8`) once
//! `|δ| < 1.0·10^-4`, so nearest delivers exactly `±1` from there
//! down, all the way to the subnormal floor; above it the value rounds
//! to a strictly smaller neighbour. In both regimes the side theorem
//! `|cos(πδ)| < 1` strictly off the exact set forces the toward-zero
//! directions off the grid point, and that is what makes the channel's
//! side a theorem rather than a measurement.

#![cfg(feature = "trig-pi")]

use core::cmp::Ordering;
use ferrodec_decimal32::{Decimal32, RoundingMode, Status};

const NE: RoundingMode = RoundingMode::NearestEven;
const NA: RoundingMode = RoundingMode::NearestAway;
const TZ: RoundingMode = RoundingMode::TowardZero;
const TP: RoundingMode = RoundingMode::TowardPositive;
const TN: RoundingMode = RoundingMode::TowardNegative;
const ALL: [RoundingMode; 5] = [NE, NA, TZ, TP, TN];

/// `cos(36°) = φ/2` at 7 digits.
const COS_36: &str = "0.8090170";
/// `cos(54°) = sin(36°)` at 7 digits.
const COS_54: &str = "0.5877853";
/// `cos(45°) = √2/2` at 7 digits.
const COS_45: &str = "0.7071068";
/// The largest `Decimal32` strictly below 1.
const JUST_BELOW_ONE: &str = "0.9999999";

fn parse(s: &str) -> Decimal32 {
    Decimal32::parse_str(s, NE)
        .unwrap_or_else(|_| panic!("test literal parses: {s:?}"))
        .0
}

fn eq(got: Decimal32, want: Decimal32) -> bool {
    got.partial_cmp(want).0 == Some(Ordering::Equal)
}

fn assert_exact(got: (Decimal32, Status), want: Decimal32, label: &str) {
    let (r, st) = got;
    assert!(eq(r, want), "{label}: got {r}, want {want}");
    assert_eq!(
        st,
        Status::OK,
        "{label}: exact result must be OK, got {st:?}"
    );
}

/// IEEE 754-2019 §9.2.1, every row, every rounding direction.
#[test]
fn specials_per_section_9_2_1() {
    let one = parse("1");
    for rm in ALL {
        // cosPi(±0) is 1.
        assert_exact(
            Decimal32::ZERO.cos_pi(rm),
            one,
            &format!("cosPi(+0) [{rm:?}]"),
        );
        assert_exact(
            Decimal32::NEG_ZERO.cos_pi(rm),
            one,
            &format!("cosPi(-0) [{rm:?}]"),
        );

        // cosPi(±∞) is a quiet NaN with INVALID.
        for (x, label) in [
            (Decimal32::INFINITY, "+inf"),
            (Decimal32::NEG_INFINITY, "-inf"),
        ] {
            let (r, st) = x.cos_pi(rm);
            assert!(r.is_nan(), "cosPi({label}) [{rm:?}] = {r}, want NaN");
            assert!(st.invalid(), "cosPi({label}) [{rm:?}] status {st:?}");
        }

        let (r, st) = Decimal32::NAN.cos_pi(rm);
        assert!(r.is_nan() && st.is_ok(), "cosPi(NaN) [{rm:?}] = {r} {st:?}");
        let (r, st) = Decimal32::SIGNALING_NAN.cos_pi(rm);
        assert!(
            r.is_nan() && st.invalid(),
            "cosPi(sNaN) [{rm:?}] = {r} {st:?}"
        );
    }
}

/// The integer row: `(−1)^n`, sign independent (the function is even),
/// including magnitudes whose quantum is at or above 1.
#[test]
fn exact_at_the_integers() {
    let one = parse("1");
    let neg_one = parse("-1");
    for (label, even) in [
        ("0", true),
        ("1", false),
        ("2", true),
        ("3", false),
        ("17", false),
        ("100", true),
        ("1000001", false),
        // Quantum ≥ 1: a trailing zero makes every one of these even.
        ("1E+7", true),
        ("1E+40", true),
        ("1E+90", true),
        ("9.999999E+96", true),
        // A full-width odd integer.
        ("9999999", false),
        // Cohorts of an even integer.
        ("2.0", true),
        ("2.000000", true),
        ("200E-2", true),
    ] {
        let want = if even { one } else { neg_one };
        for rm in ALL {
            assert_exact(
                parse(label).cos_pi(rm),
                want,
                &format!("cosPi({label}) [{rm:?}]"),
            );
            let neg = format!("-{label}");
            assert_exact(
                parse(&neg).cos_pi(rm),
                want,
                &format!("cosPi({neg}) [{rm:?}]"),
            );
        }
    }
}

/// The half-integer row: `+0` ALWAYS, both signs of operand and both
/// parities of `n`. This is the §9.2.1 rule that keeps `cosPi` even, so
/// a `−0` here would be a defect.
#[test]
fn exact_at_the_half_integers_is_always_positive_zero() {
    for label in [
        "0.5", "1.5", "2.5", "3.5", "100.5", "101.5", // Cohort variants.
        "2.50", "2.500000", "0.50", "250E-2", "999999.5",
    ] {
        for rm in ALL {
            for spelling in [label.to_string(), format!("-{label}")] {
                let (r, st) = parse(&spelling).cos_pi(rm);
                assert!(r.is_zero(), "cosPi({spelling}) [{rm:?}] = {r}, want +0");
                assert!(
                    !r.is_sign_negative(),
                    "cosPi({spelling}) [{rm:?}] = -0; §9.2.1 says +0"
                );
                assert_eq!(st, Status::OK, "cosPi({spelling}) [{rm:?}]: {st:?}");
            }
        }
    }
}

/// The fifth-turn and eighth-turn values against closed forms derived
/// independently of this kernel.
#[test]
fn known_values_against_closed_forms() {
    for (label, want) in [
        ("0.2", COS_36),
        ("0.3", COS_54),
        ("0.25", COS_45),
        // The second quadrant mirrors with a sign.
        ("0.8", "-0.8090170"),
        ("0.7", "-0.5877853"),
        ("0.75", "-0.7071068"),
    ] {
        let (r, st) = parse(label).cos_pi(NE);
        assert!(eq(r, parse(want)), "cosPi({label}) = {r}, want {want}");
        assert!(st.inexact(), "cosPi({label}) is irrational: {st:?}");
    }
}

/// The anchor band, walked across the gate boundary on both sides.
///
/// The kernel's channel opens at `adj(δ) ≤ −6`. Nothing observable may
/// change there, so this walks `k` from well above the gate (`10^-4`,
/// ladder territory) down through it (`10^-6` and below) to the
/// subnormal floor, and requires the same verdict throughout: nearest
/// delivers exactly 1, the toward-zero directions deliver the neighbour
/// below, and every mode is `INEXACT`.
#[test]
fn anchor_band_across_the_gate() {
    let one = parse("1");
    let below = parse(JUST_BELOW_ONE);
    // 10^-4 and 10^-5 sit ABOVE the gate and still inside the last
    // boundary below 1; 10^-6 is the first gated decade.
    for k in [4i32, 5, 6, 7, 8, 10, 20, 50, 95, 101] {
        let x = parse(&format!("1E-{k}"));
        for rm in [NE, NA, TP] {
            let (r, st) = x.cos_pi(rm);
            assert!(eq(r, one), "cosPi(1E-{k}) [{rm:?}] = {r}, want 1");
            assert!(st.inexact(), "cosPi(1E-{k}) [{rm:?}] must be INEXACT");
        }
        // The side theorem: cos(πδ) < 1 strictly, so the toward-zero
        // directions must step down to the neighbour and never to 1.
        for rm in [TZ, TN] {
            let (r, st) = x.cos_pi(rm);
            assert!(
                eq(r, below),
                "cosPi(1E-{k}) [{rm:?}] = {r}, want {JUST_BELOW_ONE}"
            );
            assert!(st.inexact(), "cosPi(1E-{k}) [{rm:?}] must be INEXACT");
        }
        // Evenness holds inside the band too.
        let negx = parse(&format!("-1E-{k}"));
        for rm in ALL {
            let (a, _) = x.cos_pi(rm);
            let (b, _) = negx.cos_pi(rm);
            assert_eq!(a.to_bits(), b.to_bits(), "cosPi(±1E-{k}) [{rm:?}] differ");
        }
    }
}

/// The same band around a nonzero integer, where the operand's own
/// quantum bounds `δ` below and the ladder does the work unaided. The
/// `−1` lobe exercises the channel's parity sign and the reflected
/// rounding modes.
#[test]
fn anchor_band_beside_the_integers() {
    let one = parse("1");
    let below = parse(JUST_BELOW_ONE);
    // (operand, result lobe positive)
    let cases: [(&str, bool); 4] = [
        ("1.000001", false),
        ("0.999999", false),
        ("2.000001", true),
        ("1.999999", true),
    ];
    for (label, positive) in cases {
        let x = parse(label);
        let sign = |v: Decimal32| if positive { v } else { v.neg() };
        for rm in [NE, NA] {
            let (r, st) = x.cos_pi(rm);
            assert!(
                eq(r, sign(one)),
                "cosPi({label}) [{rm:?}] = {r}, want {}",
                sign(one)
            );
            assert!(st.inexact(), "cosPi({label}) [{rm:?}] must be INEXACT");
        }
        // Toward zero steps off the anchor in both lobes.
        let (r, _) = x.cos_pi(TZ);
        assert!(
            eq(r, sign(below)),
            "cosPi({label}) [TowardZero] = {r}, want {}",
            sign(below)
        );
        // And the magnitude is strictly under 1 in every direction that
        // does not round away from zero.
        assert!(
            r.abs().partial_cmp(one).0 == Some(Ordering::Less),
            "cosPi({label}) [TowardZero] = {r} reached the anchor"
        );
    }
}

/// Above the gate the hug is wide enough that the value rounds to a
/// strictly smaller neighbour, which pins the other side of the
/// `1.0·10^-4` regime boundary and proves the band test above is not
/// vacuously asserting 1 everywhere.
#[test]
fn wide_offsets_do_not_reach_the_anchor() {
    let one = parse("1");
    for label in ["0.001", "0.002", "0.01", "0.02"] {
        let x = parse(label);
        for rm in ALL {
            let (r, st) = x.cos_pi(rm);
            assert!(st.inexact(), "cosPi({label}) [{rm:?}] must be INEXACT");
            assert!(
                r.abs().partial_cmp(one).0 == Some(Ordering::Less),
                "cosPi({label}) [{rm:?}] = {r}, want strictly under 1"
            );
        }
    }
    let (r, _) = parse("0.001").cos_pi(NE);
    assert!(eq(r, parse("0.9999951")), "cosPi(0.001) = {r}");
}

/// `10^-4` is the family's tightest nearest-mode call at any format,
/// and it lands here.
///
/// `cos(π·10^-4) = 1 − 4.93480220·10^-8`, while the midpoint below 1
/// sits at `5·10^-8`. The true value clears it by `6.5·10^-10`, a
/// margin of 1.3 percent of the half ulp: nearest must still deliver
/// exactly `1`, and toward zero must still deliver the neighbour. No
/// other format and offset in the trio comes this close to a boundary,
/// so this row is the one that would break first if the reduction or
/// the series lost accuracy.
#[test]
fn the_tightest_nearest_mode_call() {
    let x = parse("0.0001");
    for rm in [NE, NA, TP] {
        let (r, st) = x.cos_pi(rm);
        assert!(eq(r, parse("1")), "cosPi(1E-4) [{rm:?}] = {r}, want 1");
        assert!(st.inexact(), "cosPi(1E-4) [{rm:?}] must be INEXACT");
    }
    for rm in [TZ, TN] {
        let (r, _) = x.cos_pi(rm);
        assert!(
            eq(r, parse(JUST_BELOW_ONE)),
            "cosPi(1E-4) [{rm:?}] = {r}, want {JUST_BELOW_ONE}"
        );
    }
}

/// `cosPi` is even, bit for bit, on every class of operand.
#[test]
fn evenness_is_bitwise() {
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
        "999999.5",
    ] {
        let x = parse(label);
        let neg = parse(&format!("-{label}"));
        for rm in ALL {
            let (a, sa) = x.cos_pi(rm);
            let (b, sb) = neg.cos_pi(rm);
            assert_eq!(
                a.to_bits(),
                b.to_bits(),
                "cosPi(-{label}) [{rm:?}] is not cosPi({label}): {a} vs {b}"
            );
            assert_eq!(sa, sb, "cosPi(±{label}) [{rm:?}] flags differ");
        }
    }
}

/// `cosPi(x) = sinPi(x + 1/2)` exactly, the identity the shared
/// reduction is built to preserve, wherever both operands are
/// representable.
#[test]
fn quarter_turn_identity_matches_sin() {
    for label in ["0.3", "0.7", "1.2", "0.25", "0.1", "12.34", "0.0001"] {
        let x = parse(label);
        let (shifted, st_add) = x.add(parse("0.5"), NE);
        assert!(!st_add.inexact(), "{label} + 0.5 must be exact here");
        let (c, _) = x.cos_pi(NE);
        let (s, _) = shifted.sin_pi(NE);
        assert_eq!(
            c.to_bits(),
            s.to_bits(),
            "cosPi({label}) = {c} but sinPi({label} + 0.5) = {s}"
        );
    }
}
