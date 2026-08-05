//! Exact-result, special-value, gate, anchor, and flag gate for
//! `Decimal32`'s `exp_m1` (IEEE 754-2019 §9.2 `expm1`; ADR-0059
//! Track D). The sibling mirror of the root crate's
//! `tests/transcend_exact_expm1.rs`, with `Decimal32`'s own
//! thresholds.
//!
//! The exact family is `±0` and nothing else: `e^x − 1 = r` rational
//! forces `e^x = 1 + r` rational, which Lindemann forbids for
//! rational `x ≠ 0`. So this file's work is the other legs, and the
//! ones that move between formats are the two saturating gates. The
//! overflow threshold is `Decimal32`'s `224` rather than
//! `Decimal128`'s `14150`, while the `−120` collapse gate does not
//! move at all: it is a statement about the 50 digit working width,
//! not about the destination format, which is exactly why the same
//! band assertions must hold here.

#![cfg(feature = "exp-log")]

use ferrodec_decimal32::{Decimal32, RoundingMode, Status};

const NE: RoundingMode = RoundingMode::NearestEven;
const NA: RoundingMode = RoundingMode::NearestAway;
const TZ: RoundingMode = RoundingMode::TowardZero;
const TP: RoundingMode = RoundingMode::TowardPositive;
const TN: RoundingMode = RoundingMode::TowardNegative;
const ALL: [RoundingMode; 5] = [NE, NA, TZ, TP, TN];

/// `Decimal32` significand width.
const PRECISION: usize = 7;

/// `Decimal32`'s `exp` overflow gate threshold (`transcend_impl`'s
/// `exp_overflow_limit`); `e^224 ≈ 10^97.3` is past `MAX ≈ 10^97`.
const OVERFLOW_LIMIT: &str = "224";

fn parse(s: &str) -> Decimal32 {
    Decimal32::parse_str(s, NE)
        .unwrap_or_else(|_| panic!("literal parses: {s:?}"))
        .0
}

fn equal(a: Decimal32, b: Decimal32) -> bool {
    a.partial_cmp(b).0 == Some(core::cmp::Ordering::Equal)
}

/// `−0.999…9` with `PRECISION` nines: the representable neighbour of
/// `−1` toward zero.
fn nines_neighbour() -> Decimal32 {
    parse(&format!("-0.{}", "9".repeat(PRECISION)))
}

// ---------------------------------------------------------------------------
// The exact family: the zeros, and nothing else.

#[test]
fn zeros_are_the_whole_exact_family() {
    for rm in ALL {
        let (r, st) = Decimal32::ZERO.exp_m1(rm);
        assert!(
            r.is_zero() && !r.is_sign_negative(),
            "expm1(+0) at {rm:?}: got {r}"
        );
        assert_eq!(st, Status::OK, "expm1(+0) at {rm:?}: flags");

        let (r, st) = Decimal32::NEG_ZERO.exp_m1(rm);
        assert!(
            r.is_zero() && r.is_sign_negative(),
            "expm1(-0) at {rm:?}: got {r}"
        );
        assert_eq!(st, Status::OK, "expm1(-0) at {rm:?}: flags");
    }
}

#[test]
fn zero_cohort_members_stay_exact() {
    for literal in ["0.000", "-0.000", "0e-90", "-0e50"] {
        let x = parse(literal);
        for rm in ALL {
            let (r, st) = x.exp_m1(rm);
            assert!(r.is_zero(), "expm1({literal}) at {rm:?}: got {r}");
            assert_eq!(
                r.is_sign_negative(),
                x.is_sign_negative(),
                "expm1({literal}) at {rm:?}: sign must be preserved"
            );
            assert_eq!(st, Status::OK, "expm1({literal}) at {rm:?}: flags");
        }
    }
}

#[test]
fn inexact_flag_is_honest_in_every_mode() {
    for literal in [
        "1", "-1", "2", "-0.5", "0.25", "10", "-10", "1e-20", "-1e-20", "123.456", "1e-101",
        "-1e-101", "-1000",
    ] {
        let x = parse(literal);
        for rm in ALL {
            let (r, st) = x.exp_m1(rm);
            assert!(r.is_finite(), "expm1({literal}) at {rm:?}: got {r}");
            assert!(st.inexact(), "expm1({literal}) at {rm:?}: expected INEXACT");
        }
    }
}

// ---------------------------------------------------------------------------
// The −1 band: the gate and the ADR-0051 collapse above it.

fn check_minus_one_band(literal: &str) {
    let x = parse(literal);
    let neighbour = nines_neighbour();
    for rm in ALL {
        let (r, st) = x.exp_m1(rm);
        let want = if rm == TZ || rm == TP {
            neighbour
        } else {
            Decimal32::NEG_ONE
        };
        assert!(
            equal(r, want),
            "expm1({literal}) at {rm:?}: got {r}, want {want} (the true \
             value lies inside (-1, -1 + 1e-52), so only the two \
             toward-zero directions leave the anchor)"
        );
        assert_eq!(
            st,
            Status::INEXACT,
            "expm1({literal}) at {rm:?}: INEXACT and nothing else"
        );
    }
}

#[test]
fn minus_one_band_delivers_the_anchor_in_every_mode() {
    // Past the gate (`|x| > 120`), down to the largest representable
    // magnitude.
    for literal in ["-120.5", "-200", "-1000", "-1000000", "-3.5e96"] {
        check_minus_one_band(literal);
    }
    // Above the gate, where the ADR-0051 seam catches the collapse.
    for literal in ["-117", "-113.25", "-109"] {
        check_minus_one_band(literal);
    }
}

#[test]
fn gate_edge_routes_agree() {
    let gated = parse("-121");
    let kernelled = parse("-119");
    for rm in ALL {
        let (rg, sg) = gated.exp_m1(rm);
        let (rk, sk) = kernelled.exp_m1(rm);
        assert!(
            equal(rg, rk),
            "expm1(-121) = {rg} and expm1(-119) = {rk} must agree at {rm:?}"
        );
        assert_eq!(
            sg, sk,
            "gate and kernel routes must agree on flags at {rm:?}"
        );
        assert_eq!(sg, Status::INEXACT, "expm1(-121) at {rm:?}: flags");
    }
    check_minus_one_band("-120");
}

// ---------------------------------------------------------------------------
// Overflow, per §7.4.

fn check_overflow(literal: &str) {
    let x = parse(literal);
    for rm in ALL {
        let (r, st) = x.exp_m1(rm);
        if rm == TZ || rm == TN {
            assert!(
                equal(r, Decimal32::MAX),
                "expm1({literal}) at {rm:?}: got {r}, want MAX"
            );
        } else {
            assert!(
                r.is_infinite() && !r.is_sign_negative(),
                "expm1({literal}) at {rm:?}: got {r}, want +inf"
            );
        }
        assert!(
            st.overflow(),
            "expm1({literal}) at {rm:?}: expected OVERFLOW"
        );
        assert!(st.inexact(), "expm1({literal}) at {rm:?}: expected INEXACT");
    }
}

#[test]
fn overflow_follows_section_7_4_on_both_routes() {
    check_overflow("224.5");
    check_overflow("500");
    check_overflow("1e50");
    // Exactly at the threshold, which the gate leaves on the kernel
    // side (`x > limit`, strictly): the format rounder raises the
    // overflow instead, and the two routes must agree.
    check_overflow(OVERFLOW_LIMIT);
}

/// The gate threshold is coarse, so the largest in-range arguments
/// must still deliver a finite result. `e^223 ≈ 7.1e96` sits under
/// `MAX ≈ 9.999999e96`.
#[test]
fn just_under_the_overflow_gate_stays_finite() {
    let x = parse("223");
    for rm in ALL {
        let (r, st) = x.exp_m1(rm);
        assert!(r.is_finite(), "expm1(223) at {rm:?}: got {r}");
        assert!(!st.overflow(), "expm1(223) at {rm:?}: no OVERFLOW");
        assert!(st.inexact(), "expm1(223) at {rm:?}: expected INEXACT");
    }
}

// ---------------------------------------------------------------------------
// The ADR-0051 anchor band at the argument.

/// The side theorem `e^x − 1 > x` puts the true value strictly
/// between `x` and `next_up(x)` for both signs, so the modes reaching
/// down deliver `x` and `TowardPositive` (plus `TowardZero` on the
/// negative side, where up is toward zero) deliver the step above.
fn check_anchor(lit: &str) {
    for s in [lit.to_string(), format!("-{lit}")] {
        let negative = s.starts_with('-');
        let x = parse(&s);
        let above = x.next_up().0;
        for rm in ALL {
            let (r, st) = x.exp_m1(rm);
            let want_above = rm == TP || (rm == TZ && negative);
            let want = if want_above { above } else { x };
            assert!(
                equal(r, want),
                "expm1({s}) at {rm:?}: got {r}, want {want} (side theorem \
                 e^x - 1 > x puts the true value in (x, next_up(x)))"
            );
            assert!(st.inexact(), "expm1({s}) at {rm:?}: expected INEXACT");
            assert_eq!(
                st.underflow(),
                r.is_subnormal() || r.is_zero(),
                "expm1({s}) at {rm:?}: UNDERFLOW iff the inexact result is \
                 tiny, got {st:?} for {r}"
            );
        }
    }
}

#[test]
fn anchor_band_both_signs_follows_the_side_theorem() {
    // Above the ~1e-47 snap threshold the guarded ladder decides;
    // below it the ADR-0051 seam does. Both must answer identically.
    check_anchor("1e-40");
    check_anchor("1e-50");
    // Normal down to 1e-95, subnormal below it, floor at 1e-101.
    check_anchor("1e-90");
    check_anchor("1e-98");
    check_anchor("1e-101");
}

// ---------------------------------------------------------------------------
// Special values, IEEE 754-2019 §9.2.1.

#[test]
fn special_values_every_mode() {
    for rm in ALL {
        let (r, st) = Decimal32::INFINITY.exp_m1(rm);
        assert!(
            r.is_infinite() && !r.is_sign_negative(),
            "expm1(+inf) at {rm:?}: got {r}"
        );
        assert_eq!(st, Status::OK, "expm1(+inf) at {rm:?}: flags");

        let (r, st) = Decimal32::NEG_INFINITY.exp_m1(rm);
        assert!(
            equal(r, Decimal32::NEG_ONE),
            "expm1(-inf) at {rm:?}: got {r}, want -1"
        );
        assert_eq!(st, Status::OK, "expm1(-inf) at {rm:?}: flags");

        let (r, st) = Decimal32::NAN.exp_m1(rm);
        assert!(r.is_nan(), "expm1(NaN) at {rm:?}: got {r}");
        assert_eq!(st, Status::OK, "expm1(NaN) at {rm:?}: flags");

        let (r, st) = Decimal32::SIGNALING_NAN.exp_m1(rm);
        assert!(
            r.is_nan() && !r.is_signaling_nan(),
            "expm1(sNaN) at {rm:?}: got {r}"
        );
        assert!(st.invalid(), "expm1(sNaN) at {rm:?}: expected INVALID");
    }
}
