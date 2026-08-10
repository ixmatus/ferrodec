//! Special value, exact result, anchor band, and flag gate for
//! `Decimal128::atan_pi` (IEEE 754-2019 §9.2 `atanPi`; ADR-0061
//! Track D D4).
//!
//! `atanPi` is the inverse family's cleanest member: its whole exact
//! set, `{±0 → ±0, ±1 → ±1/4, ±∞ → ±1/2}`, is representable at every
//! format precision. That is the quarter turn family the decimal
//! formats keep, where `asinPi` and `acosPi` lost theirs to the non
//! terminating `1/6` and `1/3`, and it is the sharpest flag
//! difference against the radian spelling: `atan(±∞) = ±π/2` must
//! round an irrational and raise `INEXACT`, `atanPi(±∞) = ±1/2` must
//! not.
//!
//! The ADR-0051 anchor band at `±1/2` covers `adj(x) ≥ P + 2`, where
//! the true value hugs the grid point from inside at a distance no
//! finite working precision separates. The side theorem
//! `|atanPi(x)| < 1/2` for finite `x` decides the directed modes, and
//! the band tests below cross the gate in both directions and both
//! signs.

#![cfg(feature = "trig-pi")]

use core::cmp::Ordering;
use ferrodec::{Decimal128, RoundingMode, Status};

const NE: RoundingMode = RoundingMode::NearestEven;
const NA: RoundingMode = RoundingMode::NearestAway;
const TZ: RoundingMode = RoundingMode::TowardZero;
const TP: RoundingMode = RoundingMode::TowardPositive;
const TN: RoundingMode = RoundingMode::TowardNegative;
const ALL: [RoundingMode; 5] = [NE, NA, TZ, TP, TN];

/// `Decimal128` precision, and the anchor gate `adj(x) ≥ P + 2`
/// re-derived rather than copied from the kernel.
const P: i32 = 34;
const GATE: i32 = P + 2;

const HALF: &str = "0.5";
const HALF_DOWN: &str = "0.4999999999999999999999999999999999";
const NEG_HALF: &str = "-0.5";
const NEG_HALF_DOWN: &str = "-0.4999999999999999999999999999999999";

fn parse(s: &str) -> Decimal128 {
    Decimal128::parse_str(s, NE)
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

/// IEEE 754-2019 §9.2.1, every row, every rounding direction. The
/// infinities are the rows that changed: exact half turns with clean
/// flags.
#[test]
fn specials_per_section_9_2_1() {
    for rm in ALL {
        let (r, st) = Decimal128::ZERO.atan_pi(rm);
        assert!(r.is_zero() && !r.is_sign_negative(), "atanPi(+0) [{rm:?}]");
        assert_eq!(st, Status::OK, "atanPi(+0) [{rm:?}] flags");
        let (r, st) = Decimal128::NEG_ZERO.atan_pi(rm);
        assert!(r.is_zero() && r.is_sign_negative(), "atanPi(-0) [{rm:?}]");
        assert_eq!(st, Status::OK, "atanPi(-0) [{rm:?}] flags");

        assert_exact(
            Decimal128::INFINITY.atan_pi(rm),
            HALF,
            &format!("atanPi(+inf) [{rm:?}]"),
        );
        assert_exact(
            Decimal128::NEG_INFINITY.atan_pi(rm),
            NEG_HALF,
            &format!("atanPi(-inf) [{rm:?}]"),
        );

        let (r, st) = Decimal128::NAN.atan_pi(rm);
        assert!(r.is_nan() && st.is_ok(), "atanPi(NaN) [{rm:?}] {r} {st:?}");
        let (r, st) = Decimal128::SIGNALING_NAN.atan_pi(rm);
        assert!(
            r.is_nan() && st.invalid(),
            "atanPi(sNaN) [{rm:?}] {r} {st:?}"
        );
    }
}

/// `atanPi(±1) = ±1/4`, exact and cohort insensitive.
#[test]
fn unit_rows_are_exact_across_cohorts() {
    for (label, want) in [
        ("1", "0.25"),
        ("1.0", "0.25"),
        ("1.000000000000000000000000000000000", "0.25"),
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
    // One ulp off the unit input is not exact: the classifier must
    // decline it and the kernel must flag.
    for label in [
        "1.000000000000000000000000000000001",
        "0.9999999999999999999999999999999999",
    ] {
        for rm in ALL {
            let (_, st) = parse(label).atan_pi(rm);
            assert!(st.inexact(), "atanPi({label}) [{rm:?}] flags {st:?}");
        }
    }
}

/// The anchor band, deep inside the gate. `|atanPi| < 1/2` strictly,
/// so the two modes that shrink a magnitude step off the anchor and
/// the rest stay on it, in both signs. Every row is `INEXACT`.
#[test]
fn anchor_band_hugs_the_half_turn_from_inside() {
    for k in [GATE, GATE + 4, 100, 3000, 6100] {
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
            assert!(
                r.partial_cmp(parse(HALF)).0 != Some(Ordering::Greater),
                "atanPi(1e{k}) [{rm:?}] = {r} passed the half turn"
            );
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

/// The two treatments agree across the gate: `adj(x) = GATE` takes
/// the residual channel, `adj(x) = GATE - 1` takes the ladder, and
/// both must deliver identical bits and flags in every direction.
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
            assert_eq!(st_in, st_out, "atanPi across the gate [{sign}, {rm:?}]");
        }
    }
}

/// `atanPi` is odd, bit for bit.
#[test]
fn odd_symmetry_is_bitwise() {
    for label in ["1", "0.5", "3", "1e20", "1e40", "1e-20", "0.123456789"] {
        for rm in [NE, NA] {
            let (pos, st_p) = parse(label).atan_pi(rm);
            let (neg, st_n) = parse(&format!("-{label}")).atan_pi(rm);
            assert!(
                eq(neg, pos.neg()),
                "atanPi(-{label}) [{rm:?}] is not -atanPi({label})"
            );
            assert_eq!(st_p, st_n, "atanPi(±{label}) [{rm:?}] flag mismatch");
        }
    }
}

/// The tiny end is generic (slope `1/π`): `INEXACT` everywhere, never
/// stuck at the input, and equal to `x/π` to the precision one format
/// multiply can confirm. This is the executable form of ADR-0061's
/// "plain ladder, no anchor" ruling for small arguments.
#[test]
fn tiny_arguments_are_generic() {
    let inv_pi = parse("0.3183098861837906715377675267450287");
    for label in ["1e-20", "1e-100", "7e-2000"] {
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
            rel.partial_cmp(parse("1e-30")).0 == Some(Ordering::Less),
            "atanPi({label}) = {r} is not x/π (relative gap {rel})"
        );
    }
}

/// The flag difference against the radian spelling, stated directly:
/// the infinities are exact here and inexact there.
#[test]
#[cfg(feature = "trig")]
fn infinity_rows_are_exact_where_atan_was_inexact() {
    for x in [Decimal128::INFINITY, Decimal128::NEG_INFINITY] {
        for rm in ALL {
            let (_, st_turns) = x.atan_pi(rm);
            let (_, st_radians) = x.atan(rm);
            assert_eq!(st_turns, Status::OK, "atanPi(±inf) [{rm:?}] must be OK");
            assert!(
                st_radians.inexact(),
                "atan(±inf) [{rm:?}] must be INEXACT: {st_radians:?}"
            );
        }
    }
}

/// The radian kernel, scaled, as the independent witness.
#[test]
#[cfg(feature = "trig")]
fn agrees_with_the_radian_kernel_scaled() {
    let pi = parse("3.141592653589793238462643383279503");
    for label in ["1", "-1", "0.5", "2", "10", "0.001", "-7.25"] {
        let x = parse(label);
        let (turns, _) = x.atan_pi(NE);
        let (radians, _) = x.atan(NE);
        let (rescaled, _) = turns.mul(pi, NE);
        let (diff, _) = rescaled.sub(radians, NE);
        let (rel, _) = diff.abs().div(radians.abs(), NE);
        assert!(
            rel.partial_cmp(parse("1e-32")).0 == Some(Ordering::Less),
            "atanPi({label})·π = {rescaled} against atan({label}) = {radians}"
        );
    }
}
