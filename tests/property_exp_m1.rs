//! `Decimal128::exp_m1` (IEEE 754-2019 §9.2 `expm1`): special values,
//! flag honesty, two independent cross checks, the ADR-0051 anchor
//! band at the argument, and a property sweep.
//!
//! Neither cheap oracle here is a tautology. `exp_m1(x) ⊕ 1` is *not*
//! a valid reference for `exp(x)` (that is a double rounding), so the
//! `exp` cross check runs the other way and only where the closing
//! subtraction is provably exact and grid preserving: for those `x`,
//! `exp(x) ⊖ 1` and `exp_m1(x)` correctly round the same real number
//! by different routes (the `exp` pipeline plus an exact translation
//! versus the direct `expm1` series or the pipeline's own
//! subtraction), so they must agree bit for bit in every direction.
//! The second check inverts through the D1 family: `ln_1p` undoes
//! `exp_m1`, and the composition must land in the argument's own
//! neighbourhood, which is a statement about relative accuracy that
//! survives however tiny the argument is.
//!
//! The anchor band is the part no cross check reaches. Below roughly
//! `10^-47` the series collapses onto `x` itself, a format grid point
//! no rung separates from the true value, and only the ADR-0051 seam
//! and the strict inequality `e^x − 1 > x` decide the directed modes
//! there. It is the exact mirror of `logp1`'s seam with the sides
//! swapped: `logp1` hugs its argument from below, `expm1` from above.

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

/// One ulp of `v`, measured on the upward side of its magnitude.
/// Counting representable *steps* would be the wrong instrument at a
/// decade boundary: just below a power of ten the grid is ten times
/// finer, so a value one ulp under `1e-10` is ten steps away while a
/// value one ulp over it is one.
fn ulp(v: Decimal128) -> Decimal128 {
    let magnitude = v.abs();
    magnitude.next_up().0.sub(magnitude, NE).0
}

fn at_most(a: Decimal128, b: Decimal128) -> bool {
    matches!(a.partial_cmp(b).0, Some(Ordering::Less | Ordering::Equal))
}

// Special values, IEEE 754-2019 §9.2.1 -------------------------------------

#[test]
fn zeros_return_themselves_sign_preserved_and_exception_free() {
    for rm in ALL {
        let (r, s) = Decimal128::ZERO.exp_m1(rm);
        assert!(
            r.is_zero() && !r.is_sign_negative(),
            "expm1(+0) = +0 [{rm:?}]"
        );
        assert_eq!(s, Status::OK, "expm1(+0) raises nothing [{rm:?}]");

        let (r, s) = Decimal128::NEG_ZERO.exp_m1(rm);
        assert!(
            r.is_zero() && r.is_sign_negative(),
            "expm1(-0) = -0 [{rm:?}]"
        );
        assert_eq!(s, Status::OK, "expm1(-0) raises nothing [{rm:?}]");
    }
}

#[test]
fn infinities_are_the_two_asymptotes() {
    for rm in ALL {
        let (r, s) = Decimal128::INFINITY.exp_m1(rm);
        assert!(
            r.is_infinite() && !r.is_sign_negative(),
            "expm1(+inf) [{rm:?}]"
        );
        assert_eq!(s, Status::OK, "expm1(+inf) raises nothing [{rm:?}]");

        let (r, s) = Decimal128::NEG_INFINITY.exp_m1(rm);
        assert!(
            eq(r, Decimal128::NEG_ONE),
            "expm1(-inf) = -1 exactly [{rm:?}], got {r}"
        );
        assert_eq!(s, Status::OK, "expm1(-inf) raises nothing [{rm:?}]");
    }
}

#[test]
fn nan_propagates_and_signaling_nan_is_invalid() {
    for rm in ALL {
        let (r, s) = Decimal128::NAN.exp_m1(rm);
        assert!(r.is_quiet_nan(), "expm1(NaN) is a quiet NaN [{rm:?}]");
        assert_eq!(s, Status::OK, "expm1(NaN) raises nothing [{rm:?}]");

        let (r, s) = Decimal128::SIGNALING_NAN.exp_m1(rm);
        assert!(r.is_quiet_nan(), "expm1(sNaN) quiets [{rm:?}]");
        assert!(s.invalid(), "expm1(sNaN) raises INVALID [{rm:?}]");
    }
}

// Flag honesty, IEEE 754-2019 §7.5 -----------------------------------------

#[test]
fn generic_finite_inputs_are_inexact_in_every_mode() {
    for lit in [
        "1", "-1", "0.5", "-0.5", "2.5", "-2.5", "700", "-1e10", "1e-20", "-1e-20", "123.456",
        "-99.75",
    ] {
        for rm in ALL {
            let (r, s) = parse(lit).exp_m1(rm);
            assert!(r.is_finite(), "expm1({lit}) is finite [{rm:?}]");
            assert!(s.inexact(), "expm1({lit}) raises INEXACT [{rm:?}]");
        }
    }
}

#[test]
fn the_only_exact_deliveries_raise_no_inexact() {
    // `e^x - 1 = r` rational forces `e^x = 1 + r` rational, which
    // Lindemann forbids for rational `x != 0`; so `±0` is the whole
    // exact set. `-inf` joins them as an exact special value. §7.5
    // forbids INEXACT on those, in every direction.
    for rm in ALL {
        for x in [
            Decimal128::ZERO,
            Decimal128::NEG_ZERO,
            Decimal128::NEG_INFINITY,
        ] {
            let (_, s) = x.exp_m1(rm);
            assert!(
                !s.inexact(),
                "exact delivery must not raise INEXACT [{rm:?}]"
            );
            assert_eq!(s, Status::OK, "exact delivery raises nothing [{rm:?}]");
        }
    }
}

// The `exp` cross check -----------------------------------------------------

/// Arguments whose `e^x` lands well inside a decade between 2 and
/// `10^15`, so the closing `⊖ 1` is exact *and* grid preserving. The
/// test asserts both halves of that premise rather than assuming it.
/// Both bands are covered: `|x| ≤ 1.1513` routes `exp_m1` through the
/// direct series while `exp` routes through its reduction, and larger
/// `|x|` routes both through the reduction with the subtraction after
/// it.
const EXP_CROSS_CHECK_X: &[&str] = &[
    "1", "2", "3", "5", "7", "10", "20", "30", "34", "0.8", "1.1", "1.5", "2.5", "4.25", "12.0625",
    "25.5",
];

/// Where `exp(x) ⊖ 1` is exact and stays on the same format grid as
/// `exp(x)`, translating by `1` commutes with rounding (1 is an exact
/// multiple of the shared quantum, and its coefficient ends in enough
/// zeros to preserve the last digit's parity, so even the
/// nearest-even tie rule survives). `exp_m1(x)` and `exp(x) ⊖ 1` then
/// correctly round the same real number and must agree bit for bit.
#[test]
fn agrees_with_exp_minus_one_where_the_subtraction_is_exact() {
    for lit in EXP_CROSS_CHECK_X {
        let x = parse(lit);
        for rm in ALL {
            let (y, _) = x.exp(rm);
            let (d, sub_status) = y.sub(Decimal128::ONE, rm);
            // Premise 1: the subtraction itself rounds nothing away.
            assert!(
                !sub_status.inexact(),
                "test premise: exp({lit}) - 1 must be exact [{rm:?}], \
                 status {sub_status:?}"
            );
            // Premise 2: the difference stays on `exp(x)`'s own grid,
            // at least one ulp above the decade floor (so the true
            // `e^x - 1` cannot slip into the finer decade below and
            // round on a different grid).
            assert!(
                y.same_quantum(d),
                "test premise: exp({lit}) and exp({lit}) - 1 must share a \
                 quantum [{rm:?}]"
            );
            assert!(
                d.same_quantum(d.next_down().0),
                "test premise: exp({lit}) - 1 must sit at least one ulp \
                 above its decade floor [{rm:?}]"
            );

            let (m1, m1_status) = x.exp_m1(rm);
            assert_eq!(
                m1.to_bits(),
                d.to_bits(),
                "expm1({lit}) and exp({lit}) - 1 must agree bit for bit \
                 [{rm:?}]: {m1} vs {d}"
            );
            assert!(m1_status.inexact(), "expm1({lit}) raises INEXACT [{rm:?}]");
        }
    }
}

// The `ln_1p` inverse cross check -------------------------------------------

/// `logp1` is `expm1`'s inverse (ADR-0059 Track D group D1), so the
/// composition must return the argument's own neighbourhood. The
/// bound is derived at each point rather than assumed: `expm1`
/// contributes at most half an ulp of its own result, which `logp1`
/// maps back through `dx/d(m1) = 1/(1 + m1)`, and `logp1` adds at
/// most half an ulp of what it returns. Charging a full ulp for each
/// leaves a factor of two of slack, so a kernel off by an ulp on
/// either side of the composition fails this.
///
/// The grid stops at `−2` on the negative side. Deeper negative
/// arguments test `logp1`'s pole rather than either kernel: `1 + m1`
/// is `e^x`, so the amplification grows as `e^{|x|}`, and below about
/// `−77` at `Decimal128` the `expm1` result rounds onto `−1` exactly
/// and the inverse is `−∞`.
#[test]
fn ln_1p_inverts_exp_m1_on_a_moderate_grid() {
    for lit in [
        "1e-30", "-1e-30", "1e-10", "-1e-10", "0.001", "-0.001", "0.25", "-0.25", "0.75", "-0.75",
        "1", "-1", "1.1513", "-1.1513", "2", "-2", "5", "20", "100", "700",
    ] {
        let x = parse(lit);
        let (m1, s1) = x.exp_m1(NE);
        let (back, s2) = m1.ln_1p(NE);
        assert!(s1.inexact(), "expm1({lit}) raises INEXACT");
        assert!(s2.inexact(), "logp1(expm1({lit})) raises INEXACT");

        let (one_plus_m1, _) = m1.add(Decimal128::ONE, NE);
        let (mapped, _) = ulp(m1).div(one_plus_m1.abs(), NE);
        let (bound, _) = mapped.add(ulp(back), NE);
        let (diff, _) = back.sub(x, NE);
        assert!(
            at_most(diff.abs(), bound),
            "logp1(expm1({lit})) = {back} misses {x} by {diff}, past the \
             composition's own bound {bound}"
        );
    }
}

// The ADR-0051 anchor band --------------------------------------------------

/// One tiny argument, both signs, checked against the side theorem
/// `e^x − 1 > x`: the true value sits strictly between `x` and
/// `next_up(x)` on the value line, whatever the sign of `x`. So the
/// modes that reach down (`NearestEven`, `NearestAway`,
/// `TowardNegative`) deliver `x` itself, `TowardPositive` steps one
/// above, and `TowardZero` splits on the sign: `x` itself for
/// positive `x` (stepping up would grow the magnitude), one above for
/// negative `x` (that direction is toward zero). This is `logp1`'s
/// `check_anchor` with the sides swapped, `logp1` hugging its argument
/// from below where `expm1` hugs it from above.
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
                eq(r, want),
                "expm1({s}) [{rm:?}]: got {r}, want {want} (side theorem \
                 e^x - 1 > x puts the true value in (x, next_up(x)))"
            );
            assert!(st.inexact(), "expm1({s}) raises INEXACT [{rm:?}]");
            assert_eq!(
                st.underflow(),
                r.is_subnormal() || r.is_zero(),
                "expm1({s}) [{rm:?}]: UNDERFLOW iff the inexact result is \
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
    // The floor: the smallest subnormal. `next_up` of it exists on the
    // positive side, so the positive row behaves like the ones above;
    // on the negative side `next_up(-1e-6176)` is zero, so the two
    // modes that step above the argument deliver -0 (with UNDERFLOW)
    // while the other three still deliver the argument. That is the
    // mirror of `logp1`'s floor asymmetry with the sides swapped, and
    // the reason this row is not folded into them.
    check_anchor("1e-6176");
}

// Property sweep ------------------------------------------------------------

/// A finite argument spanning the tiny band, the direct band, and the
/// reduction band on both signs, staying inside the format's finite
/// result range (`|x| ≤ ~10^4`, comfortably under the `14150`
/// overflow gate and well past the `-120` collapse gate).
fn in_range() -> impl Strategy<Value = Decimal128> {
    (1u128..10u128.pow(34), -60i32..=-30i32, any::<bool>()).prop_map(|(coef, exp, neg)| {
        let sign = if neg { "-" } else { "" };
        parse(&format!("{sign}{coef}e{exp}"))
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// Every in-range input yields a finite, inexact result, and the
    /// nearest mode answer is one of the two directed neighbours it
    /// sits between.
    #[test]
    fn finite_inexact_and_bracketed_by_the_directed_modes(x in in_range()) {
        prop_assume!(!x.is_zero());
        let (tp, _) = x.exp_m1(TP);
        let (tn, _) = x.exp_m1(TN);
        for rm in ALL {
            let (r, s) = x.exp_m1(rm);
            prop_assert!(r.is_finite(), "expm1({x}) [{rm:?}] is finite, got {r}");
            prop_assert!(s.inexact(), "expm1({x}) [{rm:?}] raises INEXACT");
        }
        let (ne, _) = x.exp_m1(NE);
        prop_assert!(
            eq(ne, tp) || eq(ne, tn),
            "expm1({x}): NearestEven {ne} must be TowardPositive {tp} or \
             TowardNegative {tn}"
        );
        prop_assert!(
            eq(tn, tp) || eq(tn, tp.next_down().0),
            "expm1({x}): the directed pair must be adjacent, got {tn} and {tp}"
        );
    }

    /// The result keeps the argument's sign: `e^x − 1` is positive
    /// exactly when `x` is, and the `−1` collapse band stays negative.
    #[test]
    fn sign_follows_the_argument(x in in_range()) {
        prop_assume!(!x.is_zero());
        for rm in ALL {
            let (r, _) = x.exp_m1(rm);
            prop_assert_eq!(
                r.is_sign_negative(),
                x.is_sign_negative(),
                "expm1({}) [{:?}] must keep the argument's sign, got {}",
                x, rm, r
            );
        }
    }

    /// Correct rounding is monotone, so the kernel must be too.
    #[test]
    fn monotone_under_a_fixed_mode(a in in_range(), b in in_range()) {
        let (lo, hi) = match a.partial_cmp(b).0 {
            Some(Ordering::Greater) => (b, a),
            Some(_) => (a, b),
            None => return Ok(()),
        };
        let (rlo, _) = lo.exp_m1(NE);
        let (rhi, _) = hi.exp_m1(NE);
        prop_assert!(
            matches!(rlo.partial_cmp(rhi).0, Some(Ordering::Less | Ordering::Equal)),
            "expm1 is monotone: expm1({lo}) = {rlo} must not exceed \
             expm1({hi}) = {rhi}"
        );
    }
}
