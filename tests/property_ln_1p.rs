//! `Decimal128::ln_1p` (IEEE 754-2019 §9.2 `logp1`): special values,
//! flag honesty, the `ln` cross check, the ADR-0051 anchor band, and a
//! property sweep over the whole finite domain `x > −1`.
//!
//! The strongest cheap oracle here is `ln` itself. Wherever `1 + x` is
//! exactly representable, `logp1(x)` and `ln(1 + x)` are correctly
//! rounded values of the same real number, so the two must agree bit
//! for bit in every rounding direction; the kernels reach that
//! agreement by different routes (the `log1p` series on `u = x` versus
//! the `ln` core on `t`), which is what makes the identity a test
//! rather than a tautology. The anchor band is the part `ln` cannot
//! reach: `u = x` runs down to the smallest subnormal, the series
//! collapses onto `x` itself, and only the ADR-0051 seam and the
//! strict inequality `ln(1 + x) < x` decide the directed modes there.

#![cfg(feature = "exp-log")]

use core::cmp::Ordering;
use ferrodec::{Decimal128, RoundingMode, Status};
use proptest::prelude::*;

const NE: RoundingMode = RoundingMode::NearestEven;
const NA: RoundingMode = RoundingMode::NearestAway;
const TZ: RoundingMode = RoundingMode::TowardZero;
const TP: RoundingMode = RoundingMode::TowardPositive;
const TN: RoundingMode = RoundingMode::TowardNegative;
const ALL: [RoundingMode; 5] = [NE, NA, TZ, TP, TN];

fn parse(s: &str) -> Decimal128 {
    Decimal128::parse_str(s, NE)
        .unwrap_or_else(|_| panic!("test literal parses: {s:?}"))
        .0
}

/// Value equality, cohort insensitive (the IEEE `compare` the corpus
/// gate uses).
fn eq(a: Decimal128, b: Decimal128) -> bool {
    a.partial_cmp(b).0 == Some(Ordering::Equal)
}

// Special values, IEEE 754-2019 §9.2.1 -------------------------------------

#[test]
fn zeros_return_themselves_sign_preserved_and_exception_free() {
    for rm in ALL {
        let (r, s) = Decimal128::ZERO.ln_1p(rm);
        assert!(
            r.is_zero() && !r.is_sign_negative(),
            "logp1(+0) = +0 [{rm:?}]"
        );
        assert_eq!(s, Status::OK, "logp1(+0) raises nothing [{rm:?}]");

        let (r, s) = Decimal128::NEG_ZERO.ln_1p(rm);
        assert!(
            r.is_zero() && r.is_sign_negative(),
            "logp1(-0) = -0 [{rm:?}]"
        );
        assert_eq!(s, Status::OK, "logp1(-0) raises nothing [{rm:?}]");
    }
}

#[test]
fn minus_one_is_negative_infinity_with_div_by_zero() {
    for rm in ALL {
        let (r, s) = Decimal128::NEG_ONE.ln_1p(rm);
        assert!(
            r.is_infinite() && r.is_sign_negative(),
            "logp1(-1) [{rm:?}]"
        );
        assert!(s.div_by_zero(), "logp1(-1) raises DIV_BY_ZERO [{rm:?}]");
    }
}

#[test]
fn below_minus_one_is_invalid_nan() {
    // The tightest domain error: one representable step below −1. It
    // is computed, not spelled, so the literal cannot silently round
    // back onto −1 and turn the case into the DIV_BY_ZERO one.
    let just_below = Decimal128::NEG_ONE.next_down().0;
    for rm in ALL {
        for x in [parse("-1.5"), parse("-2"), parse("-1e100"), just_below] {
            let (r, s) = x.ln_1p(rm);
            assert!(r.is_nan(), "logp1({x}) is NaN [{rm:?}]");
            assert!(s.invalid(), "logp1({x}) raises INVALID [{rm:?}]");
        }
        let (r, s) = Decimal128::NEG_INFINITY.ln_1p(rm);
        assert!(r.is_nan(), "logp1(-inf) is NaN [{rm:?}]");
        assert!(s.invalid(), "logp1(-inf) raises INVALID [{rm:?}]");
    }
}

#[test]
fn just_above_minus_one_stays_in_domain() {
    // The mirror of the case above: one step up from −1 is the
    // smallest in-domain argument, and it must produce a finite,
    // inexact `ln(1 + x)` rather than a domain error.
    let x = Decimal128::NEG_ONE.next_up().0;
    for rm in ALL {
        let (r, s) = x.ln_1p(rm);
        assert!(r.is_finite() && r.is_sign_negative(), "logp1({x}) [{rm:?}]");
        assert!(s.inexact(), "logp1({x}) raises INEXACT [{rm:?}]");
        assert!(!s.invalid(), "logp1({x}) is in domain [{rm:?}]");
    }
}

#[test]
fn positive_infinity_passes_through() {
    for rm in ALL {
        let (r, s) = Decimal128::INFINITY.ln_1p(rm);
        assert!(
            r.is_infinite() && !r.is_sign_negative(),
            "logp1(+inf) [{rm:?}]"
        );
        assert_eq!(s, Status::OK, "logp1(+inf) raises nothing [{rm:?}]");
    }
}

#[test]
fn nan_propagates_and_signaling_nan_is_invalid() {
    for rm in ALL {
        let (r, s) = Decimal128::NAN.ln_1p(rm);
        assert!(r.is_quiet_nan(), "logp1(NaN) is a quiet NaN [{rm:?}]");
        assert_eq!(s, Status::OK, "logp1(NaN) raises nothing [{rm:?}]");

        let (r, s) = Decimal128::SIGNALING_NAN.ln_1p(rm);
        assert!(r.is_quiet_nan(), "logp1(sNaN) quiets [{rm:?}]");
        assert!(s.invalid(), "logp1(sNaN) raises INVALID [{rm:?}]");
    }
}

// Flag honesty, IEEE 754-2019 §7.5 -----------------------------------------

#[test]
fn generic_finite_inputs_are_inexact_in_every_mode() {
    for lit in [
        "1",
        "-0.5",
        "0.25",
        "-0.75",
        "2.5",
        "1e10",
        "-0.999999",
        "1e-20",
        "-1e-20",
        "123.456",
    ] {
        for rm in ALL {
            let (r, s) = parse(lit).ln_1p(rm);
            assert!(r.is_finite(), "logp1({lit}) is finite [{rm:?}]");
            assert!(s.inexact(), "logp1({lit}) raises INEXACT [{rm:?}]");
        }
    }
}

#[test]
fn the_only_exact_deliveries_raise_no_inexact() {
    // `logp1(x) = r` rational forces `1 + x = e^r`, transcendental for
    // rational `r != 0`; so `x = 0` is the whole exact set and the
    // classification leg has nothing else to deliver. §7.5 forbids
    // INEXACT on those two, in every direction.
    for rm in ALL {
        for x in [Decimal128::ZERO, Decimal128::NEG_ZERO] {
            let (_, s) = x.ln_1p(rm);
            assert!(
                !s.inexact(),
                "exact delivery must not raise INEXACT [{rm:?}]"
            );
            assert_eq!(s, Status::OK, "exact delivery raises nothing [{rm:?}]");
        }
    }
}

// The `ln` cross check ------------------------------------------------------

/// `t` values whose `t − 1` is exactly representable at 34 digits, so
/// `logp1(t − 1)` and `ln(t)` correctly round the same real number.
/// Both bands are covered: `|t − 1| < 0.5` routes `logp1` through the
/// `log1p` series while `ln` routes through its own near 1 path, and
/// `|t − 1| ≥ 0.5` routes `logp1` through `1 ⊕ x` into the `ln` core.
const LN_CROSS_CHECK_T: &[&str] = &[
    // Wide band, positive side.
    "2",
    "3",
    "7",
    "10",
    "1.5",
    "10000000000",
    "1000000000000000000000000000000",
    "1000000000000000000000000000000000",
    // Wide band, negative side (t in (0, 1), so x in (-1, -0.5]).
    "0.5",
    "0.1",
    "0.0000000001",
    "1e-33",
    // Direct band, both sides of 1.
    "1.25",
    "0.75",
    "1.000000000000000000000000000000001",
    "0.9999999999999999999999999999999999",
    "1.0000000001",
    "0.9999999999",
    "123.4567890123456789012345678901234",
];

#[test]
fn agrees_with_ln_wherever_one_plus_x_is_representable() {
    for lit in LN_CROSS_CHECK_T {
        let t = parse(lit);
        let (x, st) = t.sub(Decimal128::ONE, NE);
        assert!(
            !st.inexact(),
            "test premise: {lit} - 1 must be exact, got status {st:?}"
        );
        let (back, _) = Decimal128::ONE.add(x, NE);
        assert!(
            eq(back, t),
            "test premise: 1 + ({lit} - 1) must recover {lit}, got {back}"
        );
        for rm in ALL {
            let (from_logp1, s1) = x.ln_1p(rm);
            let (from_ln, s2) = t.ln(rm);
            assert_eq!(
                from_logp1.to_bits(),
                from_ln.to_bits(),
                "logp1({x}) and ln({lit}) must agree bit for bit [{rm:?}]: \
                 {from_logp1} vs {from_ln}"
            );
            assert_eq!(
                s1.inexact(),
                s2.inexact(),
                "logp1({x}) and ln({lit}) INEXACT parity [{rm:?}]"
            );
        }
    }
}

// The ADR-0051 anchor band --------------------------------------------------

/// One tiny argument, both signs, checked against the side theorem
/// `ln(1 + x) < x`: the true value sits strictly between `next_down(x)`
/// and `x` on the value line, whatever the sign of `x`. So the modes
/// that reach up (`NearestEven`, `NearestAway`, `TowardPositive`)
/// deliver `x` itself, `TowardNegative` steps one below, and
/// `TowardZero` splits on the sign: below `x` for positive `x` (that
/// direction is toward zero), `x` itself for negative `x` (stepping
/// below would grow the magnitude).
fn check_anchor(lit: &str) {
    for s in [lit.to_string(), format!("-{lit}")] {
        let negative = s.starts_with('-');
        let x = parse(&s);
        let below = x.next_down().0;

        for rm in ALL {
            let (r, st) = x.ln_1p(rm);
            let want_below = rm == TN || (rm == TZ && !negative);
            let want = if want_below { below } else { x };
            assert!(
                eq(r, want),
                "logp1({s}) [{rm:?}]: got {r}, want {want} (side theorem \
                 ln(1+x) < x puts the true value in (next_down(x), x))"
            );
            assert!(st.inexact(), "logp1({s}) raises INEXACT [{rm:?}]");
            assert_eq!(
                st.underflow(),
                r.is_subnormal() || r.is_zero(),
                "logp1({s}) [{rm:?}]: UNDERFLOW iff the inexact result is \
                 tiny (subnormal, or flushed onto zero from a nonzero \
                 argument), got status {st:?} for {r}"
            );
        }
    }
}

#[test]
fn anchor_band_both_signs_follows_the_side_theorem() {
    // Above the ~1e-47 snap threshold the guarded ladder delivery
    // decides these; below it the ADR-0051 seam does. Both regimes
    // must answer identically, which is the seam's whole contract.
    check_anchor("1e-40");
    check_anchor("2.718281828459045235360287471352662e-40");
    // Snap band, normal results.
    check_anchor("1e-50");
    check_anchor("1e-100");
    check_anchor("1e-6100");
    // Snap band, subnormal results (Decimal128 turns subnormal below
    // 1e-6143): the UNDERFLOW leg of the assertion above.
    check_anchor("1e-6150");
    check_anchor("1e-6170");
    // The floor: the smallest subnormal. `next_down` of it is zero on
    // the positive side, so the two modes that step below the argument
    // deliver +0 while the other three still deliver the argument. The
    // negative side has a neighbour to step onto and behaves like the
    // cases above, which is exactly the asymmetry the side theorem
    // predicts and the reason this row is not folded into them.
    check_anchor("1e-6176");
}

// Property sweep ------------------------------------------------------------

/// A finite `x > −1`: either any positive magnitude, or a negative one
/// pinned inside `(−1, 0)` by shifting the coefficient below the
/// decimal point (the interesting negative half; `x ≤ −1` is the domain
/// error the special case tests already cover).
fn in_domain() -> impl Strategy<Value = Decimal128> {
    prop_oneof![
        (1u128..10u128.pow(34), -60i32..=60i32)
            .prop_map(|(coef, exp)| parse(&format!("{coef}e{exp}"))),
        (1u128..10u128.pow(34), 0i32..=60i32).prop_map(|(coef, shift)| {
            let digits = coef.to_string().len() as i32;
            parse(&format!("-{coef}e{}", -digits - shift))
        }),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// Every in-domain input yields a finite, inexact result, and the
    /// nearest mode answer is one of the two directed neighbours it
    /// sits between.
    #[test]
    fn finite_inexact_and_bracketed_by_the_directed_modes(x in in_domain()) {
        prop_assume!(!x.is_zero());
        let (tp, _) = x.ln_1p(TP);
        let (tn, _) = x.ln_1p(TN);
        for rm in ALL {
            let (r, s) = x.ln_1p(rm);
            prop_assert!(r.is_finite(), "logp1({x}) [{rm:?}] is finite, got {r}");
            prop_assert!(s.inexact(), "logp1({x}) [{rm:?}] raises INEXACT");
        }
        let (ne, _) = x.ln_1p(NE);
        prop_assert!(
            eq(ne, tp) || eq(ne, tn),
            "logp1({x}): NearestEven {ne} must be TowardPositive {tp} or \
             TowardNegative {tn}"
        );
        prop_assert!(
            eq(tn, tp) || eq(tn, tp.next_down().0),
            "logp1({x}): the directed pair must be adjacent, got {tn} and {tp}"
        );
    }

    /// Correct rounding is monotone, so the kernel must be too.
    #[test]
    fn monotone_under_a_fixed_mode(a in in_domain(), b in in_domain()) {
        let (lo, hi) = match a.partial_cmp(b).0 {
            Some(Ordering::Greater) => (b, a),
            Some(_) => (a, b),
            None => return Ok(()),
        };
        let (rlo, _) = lo.ln_1p(NE);
        let (rhi, _) = hi.ln_1p(NE);
        prop_assert!(
            matches!(rlo.partial_cmp(rhi).0, Some(Ordering::Less | Ordering::Equal)),
            "logp1 is monotone: logp1({lo}) = {rlo} must not exceed \
             logp1({hi}) = {rhi}"
        );
    }
}
