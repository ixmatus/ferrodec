#![cfg(feature = "pow")]
//! Property-style coverage of the IEEE 754-2019 §9.2.1 `pow` special-
//! value rule table for [`Decimal64`].
//!
//! Mirrors the Decimal128 `property_pow_specials` suite. The fd-r0l
//! P5 rewire moved the finite non-special path onto the shared
//! faithful kernel; the §9.2.1 special-value rules (NaN-and-zero
//! tie-breakers, the ±0 / ±∞ base and exponent cases, the
//! negative-base / non-integer-exponent INVALID, the ±1 to ±∞
//! identity) are deterministic and total over special inputs, so this
//! file walks the `(x, y, rm)` Cartesian product over a small set of
//! distinguished constants and the five IEEE rounding modes, asserting
//! the spec rule for every combination.
//!
//! "Property test" here means table-driven enumeration rather than
//! proptest fuzzing: the spec rules are total over special inputs, so
//! exhaustive enumeration is the right tool. The point is coverage,
//! not random sampling.

use ferrodec_decimal64::{Decimal64, RoundingMode, Status};

fn d(coef: i64, exp: i32) -> Decimal64 {
    Decimal64::try_new(coef, exp).unwrap()
}

fn distinguished_inputs() -> [(&'static str, Decimal64); 11] {
    [
        ("+0", Decimal64::ZERO),
        ("-0", Decimal64::NEG_ZERO),
        ("+1", Decimal64::ONE),
        ("-1", Decimal64::NEG_ONE),
        ("+2", d(2, 0)),
        ("-2", d(-2, 0)),
        ("+0.5", d(5, -1)),
        ("+inf", Decimal64::INFINITY),
        ("-inf", Decimal64::NEG_INFINITY),
        ("qNaN", Decimal64::NAN),
        ("sNaN", Decimal64::SIGNALING_NAN),
    ]
}

fn rounding_modes() -> [RoundingMode; 5] {
    [
        RoundingMode::NearestEven,
        RoundingMode::NearestAway,
        RoundingMode::TowardZero,
        RoundingMode::TowardPositive,
        RoundingMode::TowardNegative,
    ]
}

#[test]
fn pow_no_panics_over_distinguished_grid() {
    // Totality / no-panic guard over the full special grid: a panic
    // in any branch (kernel or short-circuit) surfaces as a test
    // thread panic before any assertion runs.
    for (xn, x) in distinguished_inputs() {
        for (yn, y) in distinguished_inputs() {
            for &rm in &rounding_modes() {
                let res = std::panic::catch_unwind(|| x.pow(y, rm));
                assert!(res.is_ok(), "pow({xn}, {yn}, {rm:?}) panicked");
            }
        }
    }
}

#[test]
fn pow_x_zero_is_one_for_any_x() {
    // Rule 1: pow(x, ±0) = 1, for every x including NaN. The
    // decimal64 `pow_special_cases` short-circuit resolves the
    // zero-exponent case first and unconditionally (it returns
    // `(ONE, OK)` before the sNaN-propagation branch), so unlike the
    // Decimal128 parent decimal64 does not raise INVALID for an sNaN
    // base here. This is the pre-fd-r0l byte-unchanged decimal64
    // contract: the P5 rewire keeps the short-circuit untouched, so
    // every x (including sNaN) yields exactly 1 with OK status.
    for (xn, x) in distinguished_inputs() {
        for y in [Decimal64::ZERO, Decimal64::NEG_ZERO] {
            for &rm in &rounding_modes() {
                let (r, s) = x.pow(y, rm);
                assert_eq!(
                    r.to_bits(),
                    Decimal64::ONE.to_bits(),
                    "pow({xn}, ±0, {rm:?}) must be exactly 1",
                );
                assert_eq!(s, Status::OK, "pow({xn}, ±0, {rm:?}): no flag should fire");
            }
        }
    }
}

#[test]
fn pow_one_y_is_one_for_any_y_including_qnan() {
    // Rule 2: pow(+1, y) = 1 for any y, even qNaN. sNaN raises
    // INVALID; the result is still 1.
    for (yn, y) in distinguished_inputs() {
        for &rm in &rounding_modes() {
            let (r, s) = Decimal64::ONE.pow(y, rm);
            assert_eq!(
                r.to_bits(),
                Decimal64::ONE.to_bits(),
                "pow(+1, {yn}, {rm:?}) must be exactly 1",
            );
            if y.is_signaling_nan() {
                assert!(s.invalid());
            }
        }
    }
}

#[test]
fn pow_non_canonical_one_cohort_short_circuits() {
    // §9.2 ties pow(1, y) = 1 to *value*, not cohort. Every
    // power-of-ten cohort of the value 1 must short-circuit, for any
    // y, in every rounding mode.
    for (coef, exp) in [
        (10i64, -1),
        (100, -2),
        (10_000_000, -7),
        (1_000_000_000_000_000, -15),
    ] {
        let one_cohort = Decimal64::try_new(coef, exp).unwrap();
        for &rm in &rounding_modes() {
            let (r, s) = one_cohort.pow(d(5, 0), rm);
            assert_eq!(
                r.to_bits(),
                Decimal64::ONE.to_bits(),
                "pow({coef}E{exp}, 5, {rm:?})"
            );
            assert!(s.is_ok());
            let (r, s) = one_cohort.pow(Decimal64::NAN, rm);
            assert_eq!(
                r.to_bits(),
                Decimal64::ONE.to_bits(),
                "pow({coef}E{exp}, NaN, {rm:?})"
            );
            assert!(s.is_ok());
            let (r, s) = one_cohort.pow(Decimal64::SIGNALING_NAN, rm);
            assert_eq!(
                r.to_bits(),
                Decimal64::ONE.to_bits(),
                "pow({coef}E{exp}, sNaN, {rm:?})"
            );
            assert!(s.invalid());
        }
    }
}

#[test]
fn pow_neg_one_to_infinity_is_one() {
    // pow(±1, ±∞) = 1 per IEEE 754-2019 §9.2.1, all signs, all modes.
    for &x in &[Decimal64::ONE, Decimal64::NEG_ONE] {
        for &y in &[Decimal64::INFINITY, Decimal64::NEG_INFINITY] {
            for &rm in &rounding_modes() {
                let (r, s) = x.pow(y, rm);
                assert_eq!(
                    r.to_bits(),
                    Decimal64::ONE.to_bits(),
                    "pow({x:?}, {y:?}, {rm:?}) must be 1 per §9.2.1",
                );
                assert_eq!(s, Status::OK, "no flag for pow(±1, ±∞)");
            }
        }
    }
}

#[test]
fn pow_neg_one_qnan_propagates() {
    // The rule-2 short-circuit must NOT fire for x = -1, so
    // pow(-1, qNaN) propagates NaN per the NaN-propagation rule.
    for &rm in &rounding_modes() {
        let (r, s) = Decimal64::NEG_ONE.pow(Decimal64::NAN, rm);
        assert!(r.is_quiet_nan(), "pow(-1, qNaN, {rm:?}) must be NaN");
        assert!(!s.invalid(), "pow(-1, qNaN, {rm:?}) must not raise INVALID");
    }
}

#[test]
fn pow_zero_neg_y_is_inf_div_by_zero() {
    // Rule 4: pow(±0, y < 0) = ±∞ + DIV_BY_ZERO; sign of result
    // depends on y's integer-ness when x is -0.
    let (r, s) = Decimal64::ZERO.pow(Decimal64::NEG_ONE, RoundingMode::NearestEven);
    assert!(r.is_infinite() && !r.is_sign_negative());
    assert!(s.div_by_zero());

    let (r, s) = Decimal64::NEG_ZERO.pow(Decimal64::NEG_ONE, RoundingMode::NearestEven);
    assert!(r.is_infinite() && r.is_sign_negative());
    assert!(s.div_by_zero());
}

#[test]
fn pow_zero_pos_y_is_zero() {
    // Rule 4: pow(±0, y > 0) = ±0; sign by odd-integer y when x = -0.
    let (r, _) = Decimal64::ZERO.pow(Decimal64::ONE, RoundingMode::NearestEven);
    assert!(r.is_zero() && !r.is_sign_negative());

    let (r, _) = Decimal64::NEG_ZERO.pow(d(3, 0), RoundingMode::NearestEven);
    assert!(r.is_zero() && r.is_sign_negative());

    let (r, _) = Decimal64::NEG_ZERO.pow(d(2, 0), RoundingMode::NearestEven);
    assert!(r.is_zero() && !r.is_sign_negative());
}

#[test]
fn pow_inf_base_rules() {
    // Rule 5: pow(±∞, y). |∞|^pos = ∞, |∞|^neg = 0; sign by
    // odd-integer y for a -∞ base.
    let (r, _) = Decimal64::INFINITY.pow(d(2, 0), RoundingMode::NearestEven);
    assert!(r.is_infinite() && !r.is_sign_negative());

    let (r, _) = Decimal64::INFINITY.pow(d(-2, 0), RoundingMode::NearestEven);
    assert!(r.is_zero());

    let (r, _) = Decimal64::NEG_INFINITY.pow(d(3, 0), RoundingMode::NearestEven);
    assert!(r.is_infinite() && r.is_sign_negative());
}

#[test]
fn pow_x_inf_exponent_rules() {
    // Rule 6: pow(x, ±∞). |x|>1 ^ +∞ = +∞, |x|>1 ^ -∞ = +0,
    // |x|<1 ^ +∞ = +0, |x|<1 ^ -∞ = +∞.
    let (r, _) = d(2, 0).pow(Decimal64::INFINITY, RoundingMode::NearestEven);
    assert!(r.is_infinite() && !r.is_sign_negative());

    let (r, _) = d(2, 0).pow(Decimal64::NEG_INFINITY, RoundingMode::NearestEven);
    assert!(r.is_zero());

    let (r, _) = d(5, -1).pow(Decimal64::INFINITY, RoundingMode::NearestEven);
    assert!(r.is_zero());

    let (r, _) = d(5, -1).pow(Decimal64::NEG_INFINITY, RoundingMode::NearestEven);
    assert!(r.is_infinite() && !r.is_sign_negative());
}

#[test]
fn pow_neg_finite_non_integer_is_invalid_nan() {
    // Rule 7: pow(x, y) signals INVALID for finite x < 0 and finite
    // non-integer y, in every rounding mode.
    let half = Decimal64::parse_str("0.5", RoundingMode::NearestEven)
        .unwrap()
        .0;
    for &rm in &rounding_modes() {
        let (r, s) = Decimal64::NEG_ONE.pow(half, rm);
        assert!(r.is_quiet_nan(), "pow(-1, 0.5, {rm:?}) must be NaN");
        assert!(s.invalid(), "pow(-1, 0.5, {rm:?}) must raise INVALID");
    }
}

#[test]
fn pow_neg_base_integer_exponent_sign() {
    // The negative-base / integer-exponent path applies (-1)^y: an
    // odd integer y keeps the negative sign, an even one clears it.
    let (r, _) = d(-2, 0).pow(d(3, 0), RoundingMode::NearestEven);
    assert!(r.is_sign_negative(), "(-2)^3 must be negative");
    let (cmp, _) = r.partial_cmp(d(-8, 0));
    assert_eq!(cmp, Some(core::cmp::Ordering::Equal), "(-2)^3 = -8");

    let (r, _) = d(-2, 0).pow(d(2, 0), RoundingMode::NearestEven);
    assert!(!r.is_sign_negative(), "(-2)^2 must be positive");
    let (cmp, _) = r.partial_cmp(d(4, 0));
    assert_eq!(cmp, Some(core::cmp::Ordering::Equal), "(-2)^2 = 4");
}
