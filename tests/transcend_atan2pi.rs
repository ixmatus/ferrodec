//! Special value, exact result, anchor band, symmetry, and flag gate
//! for `Decimal128::atan2_pi` (IEEE 754-2019 §9.2 `atan2Pi`;
//! ADR-0061 Track D D4).
//!
//! ## The row that motivates the file
//!
//! `atan2Pi`'s §9.2.1 table is `atan2`'s with every result scaled by
//! `1/π`, and that scaling turns eight inexact rows into exact ones:
//! `±π`, `±π/2`, `±π/4` and `±3π/4` are irrational and must be
//! rounded with `INEXACT`, while `±1`, `±1/2`, `±1/4` and `±3/4` are
//! grid points at every format precision and §7.5 then FORBIDS a
//! flag on them. [`axis_rows_are_exact_where_atan2_was_inexact`]
//! asserts that difference against the radian kernel directly, since
//! a caller reading flags is reading a different function.
//!
//! ## The anchor bands
//!
//! Two ADR-0051 residual channels, both gated on the adjusted
//! exponent gap `adj(y) - adj(x)`:
//!
//! * `gap ≥ P + 2`: the value hugs `±1/2`, from INSIDE for `x > 0`
//!   and from OUTSIDE for `x < 0` (the quadrant shift crosses the
//!   axis, so `x`'s sign, not `y`'s, picks the side).
//! * `gap ≤ -(P + 3)` with `x < 0`: the value hugs `±1` from inside.
//!   The mirror case `x > 0` is deliberately NOT an anchor: there the
//!   value is `y/(πx)`, slope `1/π` against a shrinking ratio, which
//!   the plain ladder decides.
//!
//! [`tiny_ratio_with_a_positive_abscissa_is_not_an_anchor`] is the
//! executable form of that last sentence.

#![cfg(feature = "trig-pi")]

use core::cmp::Ordering;
use ferrodec::{Decimal128, RoundingMode, Status};

const NE: RoundingMode = RoundingMode::NearestEven;
const NA: RoundingMode = RoundingMode::NearestAway;
const TZ: RoundingMode = RoundingMode::TowardZero;
const TP: RoundingMode = RoundingMode::TowardPositive;
const TN: RoundingMode = RoundingMode::TowardNegative;
const ALL: [RoundingMode; 5] = [NE, NA, TZ, TP, TN];

/// `Decimal128` precision, and the two gates re-derived rather than
/// copied from the kernel.
const P: i32 = 34;
const HALF_GATE: i32 = P + 2;
const FULL_GATE: i32 = -(P + 3);

const HALF: &str = "0.5";
const HALF_DOWN: &str = "0.4999999999999999999999999999999999";
const HALF_UP: &str = "0.5000000000000000000000000000000001";
const ONE: &str = "1";
const ONE_DOWN: &str = "0.9999999999999999999999999999999999";

fn parse(s: &str) -> Decimal128 {
    Decimal128::parse_str(s, NE)
        .unwrap_or_else(|_| panic!("test literal parses: {s:?}"))
        .0
}

fn eq(got: Decimal128, want: Decimal128) -> bool {
    got.partial_cmp(want).0 == Some(Ordering::Equal)
}

/// Assert an exact row: the value, and the clean status §7.5 demands.
fn assert_exact(got: (Decimal128, Status), want: &str, label: &str) {
    let (r, st) = got;
    assert!(eq(r, parse(want)), "{label}: got {r}, want {want}");
    assert_eq!(st, Status::OK, "{label}: §7.5 forbids a flag here");
}

/// IEEE 754-2019 §9.2.1, every row, every rounding direction. The
/// table is `atan2`'s scaled by `1/π`, cross checked against the C23
/// `atan2pi` Annex F list, which agrees row for row.
#[test]
fn specials_per_section_9_2_1() {
    let inf = Decimal128::INFINITY;
    let ninf = Decimal128::NEG_INFINITY;
    let zero = Decimal128::ZERO;
    let nzero = Decimal128::NEG_ZERO;
    for rm in ALL {
        // Both infinite: the diagonals.
        assert_exact(
            inf.atan2_pi(inf, rm),
            "0.25",
            &format!("(+inf, +inf) {rm:?}"),
        );
        assert_exact(
            ninf.atan2_pi(inf, rm),
            "-0.25",
            &format!("(-inf, +inf) {rm:?}"),
        );
        assert_exact(
            inf.atan2_pi(ninf, rm),
            "0.75",
            &format!("(+inf, -inf) {rm:?}"),
        );
        assert_exact(
            ninf.atan2_pi(ninf, rm),
            "-0.75",
            &format!("(-inf, -inf) {rm:?}"),
        );

        // Infinite ordinate against a finite abscissa: the ±1/2 axis.
        for x in ["0", "-0", "1", "-1", "1e6000"] {
            assert_exact(
                inf.atan2_pi(parse(x), rm),
                HALF,
                &format!("(+inf, {x}) {rm:?}"),
            );
            assert_exact(
                ninf.atan2_pi(parse(x), rm),
                "-0.5",
                &format!("(-inf, {x}) {rm:?}"),
            );
        }

        // Finite ordinate against an infinite abscissa.
        for y in ["1", "1e-6000"] {
            assert_exact(
                parse(y).atan2_pi(ninf, rm),
                ONE,
                &format!("({y}, -inf) {rm:?}"),
            );
            assert_exact(
                parse(&format!("-{y}")).atan2_pi(ninf, rm),
                "-1",
                &format!("(-{y}, -inf) {rm:?}"),
            );
            let (r, st) = parse(y).atan2_pi(inf, rm);
            assert!(r.is_zero() && !r.is_sign_negative(), "({y}, +inf) {rm:?}");
            assert_eq!(st, Status::OK, "({y}, +inf) {rm:?} flags");
            let (r, st) = parse(&format!("-{y}")).atan2_pi(inf, rm);
            assert!(r.is_zero() && r.is_sign_negative(), "(-{y}, +inf) {rm:?}");
            assert_eq!(st, Status::OK, "(-{y}, +inf) {rm:?} flags");
        }

        // Both zero: the abscissa's sign alone decides.
        for (y, y_neg) in [(zero, false), (nzero, true)] {
            let (r, st) = y.atan2_pi(zero, rm);
            assert!(r.is_zero() && r.is_sign_negative() == y_neg, "(±0, +0)");
            assert_eq!(st, Status::OK, "(±0, +0) {rm:?} flags");
            assert_exact(
                y.atan2_pi(nzero, rm),
                if y_neg { "-1" } else { ONE },
                &format!("(±0, -0) {rm:?}"),
            );
        }

        // Zero ordinate against a finite nonzero abscissa.
        for (y, y_neg) in [(zero, false), (nzero, true)] {
            assert_exact(
                y.atan2_pi(parse("-3"), rm),
                if y_neg { "-1" } else { ONE },
                &format!("(±0, x < 0) {rm:?}"),
            );
            let (r, st) = y.atan2_pi(parse("3"), rm);
            assert!(r.is_zero() && r.is_sign_negative() == y_neg, "(±0, x > 0)");
            assert_eq!(st, Status::OK, "(±0, x > 0) {rm:?} flags");
        }

        // Nonzero ordinate against a zero abscissa: the ±1/2 axis.
        for x in [zero, nzero] {
            assert_exact(
                parse("7").atan2_pi(x, rm),
                HALF,
                &format!("(y > 0, ±0) {rm:?}"),
            );
            assert_exact(
                parse("-7").atan2_pi(x, rm),
                "-0.5",
                &format!("(y < 0, ±0) {rm:?}"),
            );
        }

        // NaN propagation, in the fixed operand order [self, x].
        let (r, st) = Decimal128::NAN.atan2_pi(parse("1"), rm);
        assert!(r.is_nan() && st.is_ok(), "(NaN, 1) {rm:?} {r} {st:?}");
        let (r, st) = parse("1").atan2_pi(Decimal128::NAN, rm);
        assert!(r.is_nan() && st.is_ok(), "(1, NaN) {rm:?} {r} {st:?}");
        let (r, st) = Decimal128::SIGNALING_NAN.atan2_pi(parse("1"), rm);
        assert!(r.is_nan() && st.invalid(), "(sNaN, 1) {rm:?} {r} {st:?}");
        let (r, st) = parse("1").atan2_pi(Decimal128::SIGNALING_NAN, rm);
        assert!(r.is_nan() && st.invalid(), "(1, sNaN) {rm:?} {r} {st:?}");
        // An infinite operand does NOT outrank a NaN here (unlike
        // `hypot`, whose §9.2.1 row makes that exception explicit).
        let (r, _) = Decimal128::NAN.atan2_pi(inf, rm);
        assert!(r.is_nan(), "(NaN, +inf) {rm:?} = {r}");
    }
}

/// The behavioural difference the scaling introduces, asserted
/// against the radian kernel on the same operands: eight rows that
/// were `INEXACT` there are exact here.
#[test]
#[cfg(feature = "trig")]
fn axis_rows_are_exact_where_atan2_was_inexact() {
    let inf = Decimal128::INFINITY;
    let ninf = Decimal128::NEG_INFINITY;
    let rows: [(Decimal128, Decimal128); 6] = [
        (inf, inf),
        (inf, ninf),
        (inf, parse("1")),
        (parse("1"), ninf),
        (Decimal128::ZERO, parse("-1")),
        (parse("1"), Decimal128::ZERO),
    ];
    for (y, x) in rows {
        for rm in ALL {
            let (_, st_turns) = y.atan2_pi(x, rm);
            let (_, st_radians) = y.atan2(x, rm);
            assert_eq!(
                st_turns,
                Status::OK,
                "atan2Pi({y}, {x}) [{rm:?}] must be exact"
            );
            assert!(
                st_radians.inexact(),
                "atan2({y}, {x}) [{rm:?}] must be INEXACT: {st_radians:?}"
            );
        }
    }
}

/// The finite diagonals `|y| = |x|`, exact and cohort insensitive:
/// `±1/4` for a positive abscissa, `±3/4` for a negative one, signed
/// by the ordinate.
#[test]
fn diagonals_are_exact() {
    for (y, x, want) in [
        ("1", "1", "0.25"),
        ("3", "3.0", "0.25"),
        ("3.000", "3", "0.25"),
        ("1e100", "1e100", "0.25"),
        ("1e-6100", "1e-6100", "0.25"),
        ("-1", "1", "-0.25"),
        ("1", "-1", "0.75"),
        ("-1", "-1", "-0.75"),
        ("-2.5", "-2.5", "-0.75"),
        ("0.0001", "-0.0001", "0.75"),
    ] {
        for rm in ALL {
            assert_exact(
                parse(y).atan2_pi(parse(x), rm),
                want,
                &format!("atan2Pi({y}, {x}) [{rm:?}]"),
            );
        }
    }
    // One ulp off the diagonal is not exact.
    for (y, x) in [
        ("1.000000000000000000000000000000001", "1"),
        ("1", "-1.000000000000000000000000000000001"),
    ] {
        for rm in ALL {
            let (_, st) = parse(y).atan2_pi(parse(x), rm);
            assert!(st.inexact(), "atan2Pi({y}, {x}) [{rm:?}] flags {st:?}");
        }
    }
}

/// The `±1/2` band: the side is the ABSCISSA's sign, because the
/// quadrant shift crosses the axis for `x < 0`. The ordinate's sign
/// picks which half turn.
#[test]
fn half_turn_band_takes_its_side_from_the_abscissa() {
    for gap in [HALF_GATE, HALF_GATE + 10, 200, 6000] {
        let big = parse(&format!("1e{gap}"));
        let neg_big = parse(&format!("-1e{gap}"));
        // x > 0: the value approaches ±1/2 from inside.
        for (rm, want) in [
            (NE, HALF),
            (NA, HALF),
            (TZ, HALF_DOWN),
            (TN, HALF_DOWN),
            (TP, HALF),
        ] {
            let (r, st) = big.atan2_pi(parse("1"), rm);
            assert!(eq(r, parse(want)), "atan2Pi(1e{gap}, 1) [{rm:?}]: {r}");
            assert!(st.inexact(), "atan2Pi(1e{gap}, 1) [{rm:?}] flags {st:?}");
        }
        for (rm, want) in [
            (NE, "-0.5"),
            (NA, "-0.5"),
            (TZ, "-0.4999999999999999999999999999999999"),
            (TP, "-0.4999999999999999999999999999999999"),
            (TN, "-0.5"),
        ] {
            let (r, _) = neg_big.atan2_pi(parse("1"), rm);
            assert!(eq(r, parse(want)), "atan2Pi(-1e{gap}, 1) [{rm:?}]: {r}");
        }
        // x < 0: the value approaches ±1/2 from outside.
        for (rm, want) in [
            (NE, HALF),
            (NA, HALF),
            (TZ, HALF),
            (TN, HALF),
            (TP, HALF_UP),
        ] {
            let (r, st) = big.atan2_pi(parse("-1"), rm);
            assert!(eq(r, parse(want)), "atan2Pi(1e{gap}, -1) [{rm:?}]: {r}");
            assert!(st.inexact(), "atan2Pi(1e{gap}, -1) [{rm:?}] flags {st:?}");
        }
        for (rm, want) in [
            (NE, "-0.5"),
            (NA, "-0.5"),
            (TZ, "-0.5"),
            (TP, "-0.5"),
            (TN, "-0.5000000000000000000000000000000001"),
        ] {
            let (r, _) = neg_big.atan2_pi(parse("-1"), rm);
            assert!(eq(r, parse(want)), "atan2Pi(-1e{gap}, -1) [{rm:?}]: {r}");
        }
    }
}

/// The `±1` band: a vanishing ratio against a negative abscissa hugs
/// the full turn from inside, in both ordinate signs.
#[test]
fn full_turn_band_hugs_from_inside() {
    for gap in [FULL_GATE, FULL_GATE - 10, -200, -6000] {
        let tiny = parse(&format!("1e{gap}"));
        let neg_tiny = parse(&format!("-1e{gap}"));
        for (rm, want) in [
            (NE, ONE),
            (NA, ONE),
            (TZ, ONE_DOWN),
            (TN, ONE_DOWN),
            (TP, ONE),
        ] {
            let (r, st) = tiny.atan2_pi(parse("-1"), rm);
            assert!(eq(r, parse(want)), "atan2Pi(1e{gap}, -1) [{rm:?}]: {r}");
            assert!(st.inexact(), "atan2Pi(1e{gap}, -1) [{rm:?}] flags {st:?}");
        }
        for (rm, want) in [
            (NE, "-1"),
            (NA, "-1"),
            (TZ, "-0.9999999999999999999999999999999999"),
            (TP, "-0.9999999999999999999999999999999999"),
            (TN, "-1"),
        ] {
            let (r, _) = neg_tiny.atan2_pi(parse("-1"), rm);
            assert!(eq(r, parse(want)), "atan2Pi(-1e{gap}, -1) [{rm:?}]: {r}");
        }
    }
}

/// The absent arm, made observable: the same vanishing ratio against
/// a POSITIVE abscissa is a generic small value `y/(πx)`, never a
/// hug, and the plain ladder delivers it.
#[test]
fn tiny_ratio_with_a_positive_abscissa_is_not_an_anchor() {
    let inv_pi = parse("0.3183098861837906715377675267450287");
    for gap in [FULL_GATE, -200, -3000] {
        let y = parse(&format!("1e{gap}"));
        let (r, st) = y.atan2_pi(parse("1"), NE);
        assert!(st.inexact(), "atan2Pi(1e{gap}, 1) flags {st:?}");
        assert!(!r.is_zero(), "atan2Pi(1e{gap}, 1) collapsed to zero");
        assert!(!eq(r, y), "atan2Pi(1e{gap}, 1) stuck at the ordinate");
        let (scaled, _) = y.mul(inv_pi, NE);
        let (diff, _) = r.sub(scaled, NE);
        let (rel, _) = diff.abs().div(r.abs(), NE);
        assert!(
            rel.partial_cmp(parse("1e-30")).0 == Some(Ordering::Less),
            "atan2Pi(1e{gap}, 1) = {r} is not y/(πx) (relative gap {rel})"
        );
    }
}

/// Both bands agree with the ladder across their gates: one step
/// outside each gate the plain path must deliver the same bits and
/// flags the residual channel delivers one step inside.
#[test]
fn the_two_treatments_agree_across_both_gates() {
    for (inside, outside) in [
        (
            (format!("1e{HALF_GATE}"), "1".to_string()),
            (format!("1e{}", HALF_GATE - 1), "1".to_string()),
        ),
        (
            (format!("1e{HALF_GATE}"), "-1".to_string()),
            (format!("1e{}", HALF_GATE - 1), "-1".to_string()),
        ),
        (
            (format!("1e{FULL_GATE}"), "-1".to_string()),
            (format!("1e{}", FULL_GATE + 1), "-1".to_string()),
        ),
    ] {
        for rm in ALL {
            let (r_in, st_in) = parse(&inside.0).atan2_pi(parse(&inside.1), rm);
            let (r_out, st_out) = parse(&outside.0).atan2_pi(parse(&outside.1), rm);
            assert!(
                eq(r_in, r_out),
                "atan2Pi({}, {}) vs ({}, {}) [{rm:?}]: {r_in} vs {r_out}",
                inside.0,
                inside.1,
                outside.0,
                outside.1
            );
            assert_eq!(st_in, st_out, "flags across the gate [{rm:?}]");
        }
    }
}

/// Quadrant symmetries. Negating the ordinate negates the result bit
/// for bit (the function is odd in `y`), and reflecting the abscissa
/// takes a first quadrant value to its complement in a half turn.
#[test]
fn quadrant_symmetries() {
    for (y, x) in [
        ("1", "2"),
        ("3", "1"),
        ("0.5", "0.25"),
        ("1e10", "3"),
        ("7", "1e10"),
    ] {
        let (pos, st_p) = parse(y).atan2_pi(parse(x), NE);
        let (neg, st_n) = parse(&format!("-{y}")).atan2_pi(parse(x), NE);
        assert!(
            eq(neg, pos.neg()),
            "atan2Pi(-{y}, {x}) is not -atan2Pi({y}, {x}): {neg} vs {pos}"
        );
        assert_eq!(st_p, st_n, "atan2Pi(±{y}, {x}) flag mismatch");

        // Reflecting the abscissa: atan2Pi(y, -x) = 1 - atan2Pi(y, x)
        // for a positive ordinate, up to the two roundings.
        let (reflected, _) = parse(y).atan2_pi(parse(&format!("-{x}")), NE);
        let (sum, _) = reflected.add(pos, NE);
        let (diff, _) = sum.sub(parse(ONE), NE);
        assert!(
            diff.abs().partial_cmp(parse("1e-33")).0 == Some(Ordering::Less),
            "atan2Pi({y}, -{x}) + atan2Pi({y}, {x}) = {sum}, off by {diff}"
        );
    }
}

/// The one argument kernel is the witness on the open first quadrant:
/// `atan2Pi(y, x) = atanPi(y/x)` for `x > 0` whenever the quotient is
/// itself representable, so the two paths must agree to within the
/// quotient's own rounding.
#[test]
fn first_quadrant_matches_the_one_argument_kernel() {
    for (y, x, quotient) in [
        ("1", "2", "0.5"),
        ("3", "4", "0.75"),
        ("1", "8", "0.125"),
        ("5", "1", "5"),
        ("1", "1e10", "1e-10"),
    ] {
        for rm in ALL {
            let (binary, st_b) = parse(y).atan2_pi(parse(x), rm);
            let (unary, st_u) = parse(quotient).atan_pi(rm);
            assert!(
                eq(binary, unary),
                "atan2Pi({y}, {x}) [{rm:?}] = {binary} against atanPi({quotient}) = {unary}"
            );
            assert_eq!(st_b, st_u, "atan2Pi({y}, {x}) [{rm:?}] flags");
        }
    }
}

/// The radian kernel, scaled, as the independent witness.
#[test]
#[cfg(feature = "trig")]
fn agrees_with_the_radian_kernel_scaled() {
    let pi = parse("3.141592653589793238462643383279503");
    for (y, x) in [
        ("1", "2"),
        ("-3", "4"),
        ("1", "-1.5"),
        ("-2", "-7"),
        ("1e6", "3"),
    ] {
        let (turns, _) = parse(y).atan2_pi(parse(x), NE);
        let (radians, _) = parse(y).atan2(parse(x), NE);
        let (rescaled, _) = turns.mul(pi, NE);
        let (diff, _) = rescaled.sub(radians, NE);
        let (rel, _) = diff.abs().div(radians.abs(), NE);
        assert!(
            rel.partial_cmp(parse("1e-32")).0 == Some(Ordering::Less),
            "atan2Pi({y}, {x})·π = {rescaled} against atan2 = {radians}"
        );
    }
}
