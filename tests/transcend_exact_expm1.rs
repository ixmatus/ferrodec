//! Exact-result, special-value, gate, and flag gate for
//! `Decimal128`'s `exp_m1` (IEEE 754-2019 §9.2 `expm1`; ADR-0059
//! Track D).
//!
//! `expm1` has the smallest exact family in the §9.2 surface: if
//! `e^x − 1 = r` is rational then `e^x = 1 + r` is rational, which
//! Lindemann forbids for rational `x ≠ 0`. So `±0` is the whole exact
//! set and every other input is inexact in every rounding direction.
//! That makes this file's job the *other* three legs: the two
//! saturating gates (`+∞` past the overflow threshold, `−1` past
//! `x = −120`), the ADR-0051 collapse onto `−1` just above that gate,
//! and the §7.5 flag honesty that a spurious `INEXACT` on `±0` would
//! break.
//!
//! The `−1` band is where a defect would hide: the true value sits
//! inside `(−1, −1 + 10^−52)`, closer to `−1` than the format's first
//! boundary toward zero by more than thirty orders of magnitude, so
//! the three modes that reach away from zero deliver `−1` and the two
//! that reach toward it deliver the 34 nines neighbour. A kernel that
//! returned `−1` in all five directions would pass a nearest-only
//! test and violate §7.4's directed dispositions.

#![cfg(feature = "exp-log")]

use ferrodec::{Decimal128, RoundingMode, Status};

const NE: RoundingMode = RoundingMode::NearestEven;
const NA: RoundingMode = RoundingMode::NearestAway;
const TZ: RoundingMode = RoundingMode::TowardZero;
const TP: RoundingMode = RoundingMode::TowardPositive;
const TN: RoundingMode = RoundingMode::TowardNegative;
const ALL: [RoundingMode; 5] = [NE, NA, TZ, TP, TN];

/// `Decimal128` significand width.
const PRECISION: usize = 34;

/// The `exp` overflow gate threshold for `Decimal128`
/// (`transcend_impl`'s `exp_overflow_limit`).
const OVERFLOW_LIMIT: &str = "14150";

fn parse(s: &str) -> Decimal128 {
    Decimal128::parse_str(s, NE)
        .unwrap_or_else(|_| panic!("literal parses: {s:?}"))
        .0
}

fn equal(a: Decimal128, b: Decimal128) -> bool {
    a.partial_cmp(b).0 == Some(core::cmp::Ordering::Equal)
}

/// `−0.999…9` with `PRECISION` nines: the representable neighbour of
/// `−1` toward zero, and the answer the two toward-zero directions owe
/// on the whole `−1` band.
fn nines_neighbour() -> Decimal128 {
    parse(&format!("-0.{}", "9".repeat(PRECISION)))
}

// ---------------------------------------------------------------------------
// The exact family: the zeros, and nothing else.

/// `expm1(±0) = ±0`, sign preserved, with no exception raised in any
/// direction (§9.2.1 for the value, §7.5 for the flags). This is the
/// entire exact family, so it is walked exhaustively rather than
/// sampled.
#[test]
fn zeros_are_the_whole_exact_family() {
    for rm in ALL {
        let (r, st) = Decimal128::ZERO.exp_m1(rm);
        assert!(
            r.is_zero() && !r.is_sign_negative(),
            "expm1(+0) at {rm:?}: got {r}"
        );
        assert_eq!(st, Status::OK, "expm1(+0) at {rm:?}: flags");

        let (r, st) = Decimal128::NEG_ZERO.exp_m1(rm);
        assert!(
            r.is_zero() && r.is_sign_negative(),
            "expm1(-0) at {rm:?}: got {r}"
        );
        assert_eq!(st, Status::OK, "expm1(-0) at {rm:?}: flags");
    }
}

/// The zero cohort, not just the canonical member: a zero carrying a
/// non-zero quantum exponent is still an exact `±0` case.
#[test]
fn zero_cohort_members_stay_exact() {
    for literal in ["0.000", "-0.000", "0e-6100", "-0e100"] {
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

/// §7.5 flag honesty: every input outside the exact family is
/// irrational and raises `INEXACT` in every direction, including the
/// integers, the powers of ten, and the arguments whose `exp` is an
/// exactly representable-looking round number.
#[test]
fn inexact_flag_is_honest_in_every_mode() {
    for literal in [
        "1", "-1", "2", "-0.5", "0.25", "10", "-10", "1e-20", "-1e-20", "123.456", "1e-6176",
        "-1e-6176", "1000", "-1000",
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

/// Every input at or below `−120` (and, above it, every input whose
/// working `e^x − 1` collapses onto `−1`) delivers the anchor: `−1`
/// toward negative and at both nearest modes, the nines neighbour
/// toward zero and toward positive. `INEXACT` always, and nothing
/// else: the result is normal, so neither `UNDERFLOW` nor `OVERFLOW`
/// belongs on it.
fn check_minus_one_band(literal: &str) {
    let x = parse(literal);
    let neighbour = nines_neighbour();
    for rm in ALL {
        let (r, st) = x.exp_m1(rm);
        let want = if rm == TZ || rm == TP {
            neighbour
        } else {
            Decimal128::NEG_ONE
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
    // Past the gate (`|x| > 120`), spanning the format's whole
    // negative range down to the largest representable magnitude.
    for literal in ["-120.5", "-200", "-1000", "-1000000", "-3.5e6144"] {
        check_minus_one_band(literal);
    }
    // Above the gate, where the ADR-0051 seam catches the collapse
    // instead: `e^-117` is ~1.5e-51, so the working subtraction rounds
    // to exactly -1 at 50 digits, and `e^-109` is ~1.4e-48, inside the
    // seam's ~1e-47 relative snap band without collapsing.
    for literal in ["-117", "-113.25", "-109"] {
        check_minus_one_band(literal);
    }
}

/// The gate edge, from both sides: `−121` is delivered by the gate and
/// `−119` by the kernel's own collapse onto `−1`. The two routes must
/// return identical verdicts in every direction, which is what makes
/// the gate a short circuit rather than a second policy.
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
    // And exactly at the gate's own threshold, which the comparison
    // leaves on the kernel side (`|x| > 120`, strictly).
    check_minus_one_band("-120");
}

// ---------------------------------------------------------------------------
// Overflow, per §7.4.

/// Past the overflow gate every direction gets its §7.4 disposition:
/// `+∞` at both nearest modes and toward positive, the largest finite
/// magnitude toward zero and toward negative, with `OVERFLOW` and
/// `INEXACT` on all five. Subtracting 1 from a value at the `10^6145`
/// scale cannot pull it back inside the format, which is why the gate
/// may saturate without consulting the series.
fn check_overflow(literal: &str) {
    let x = parse(literal);
    for rm in ALL {
        let (r, st) = x.exp_m1(rm);
        if rm == TZ || rm == TN {
            assert!(
                equal(r, Decimal128::MAX),
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
    // Past the gate.
    check_overflow("14150.5");
    check_overflow("20000");
    check_overflow("1e6000");
    // Exactly at the gate threshold, which the comparison leaves on
    // the kernel side (`x > limit`, strictly): the series runs and the
    // format rounder raises the overflow instead. The two routes must
    // agree, or the gate would be observable.
    check_overflow(OVERFLOW_LIMIT);
}

/// The gate's other side: the threshold is coarse, so the largest
/// in-range arguments must still deliver a finite result rather than
/// being swept into the saturation. `e^14149 ≈ 6.8e6144` sits just
/// under `MAX`.
#[test]
fn just_under_the_overflow_gate_stays_finite() {
    let x = parse("14149");
    for rm in ALL {
        let (r, st) = x.exp_m1(rm);
        assert!(r.is_finite(), "expm1(14149) at {rm:?}: got {r}");
        assert!(!st.overflow(), "expm1(14149) at {rm:?}: no OVERFLOW");
        assert!(st.inexact(), "expm1(14149) at {rm:?}: expected INEXACT");
    }
}

// ---------------------------------------------------------------------------
// Special values, IEEE 754-2019 §9.2.1.

#[test]
fn special_values_every_mode() {
    for rm in ALL {
        let (r, st) = Decimal128::INFINITY.exp_m1(rm);
        assert!(
            r.is_infinite() && !r.is_sign_negative(),
            "expm1(+inf) at {rm:?}: got {r}"
        );
        assert_eq!(st, Status::OK, "expm1(+inf) at {rm:?}: flags");

        // The one special value that is a finite exact number: -1,
        // with no exception. A kernel that reached the series here
        // would raise INEXACT and be wrong on both counts.
        let (r, st) = Decimal128::NEG_INFINITY.exp_m1(rm);
        assert!(
            equal(r, Decimal128::NEG_ONE),
            "expm1(-inf) at {rm:?}: got {r}, want -1"
        );
        assert_eq!(st, Status::OK, "expm1(-inf) at {rm:?}: flags");

        let (r, st) = Decimal128::NAN.exp_m1(rm);
        assert!(r.is_nan(), "expm1(NaN) at {rm:?}: got {r}");
        assert_eq!(st, Status::OK, "expm1(NaN) at {rm:?}: flags");

        let (r, st) = Decimal128::SIGNALING_NAN.exp_m1(rm);
        assert!(
            r.is_nan() && !r.is_signaling_nan(),
            "expm1(sNaN) at {rm:?}: got {r}"
        );
        assert!(st.invalid(), "expm1(sNaN) at {rm:?}: expected INVALID");
    }
}
