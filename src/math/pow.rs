//! Re-export + delegating shim: the pow kernel moved to
//! ferrodec-transcend (P0a.2 c11). The public `Decimal128::pow`
//! wrapper, the `#[cfg(kani)]` `pow_special_only_for_kani` ADR-0016
//! shim, and the behaviour tests stay here as the byte-identical /
//! Kani regression gate.

use crate::decimal::Decimal128;
use ferrodec_ieee::{RoundingMode, Status};

impl Decimal128 {
    /// `self` raised to the power `exp`.
    #[must_use]
    pub fn pow(self, exp: Self, rm: RoundingMode) -> (Self, Status) {
        ferrodec_transcend::pow::pow_kernel::<Decimal128>(self, exp, rm)
    }

    /// IEEE 754-2019 §9.2 `pown(self, n)`: `self` raised to the
    /// integer power `n`.
    ///
    /// A negative base is legal for every `n` (the exponent is an
    /// integer by type, so `pow`'s negative-base `INVALID` has no
    /// analog); the result is negative exactly when `self` is negative
    /// and `n` is odd. Pure delegation onto the shared kernel, which
    /// resolves every §9.2.1 special value internally and runs the
    /// ADR-0059 escalation ladder from this operation's first release.
    /// The special-value table, the two-arm kernel (working-precision
    /// powering for `|n| ≤ 6`, `exp(n·ln|self|)` beyond), the
    /// exactness and tie classification, and the ADR-0060 operand
    /// ranges over which the correct-rounding claim is unconditional
    /// all live on `ferrodec_transcend::pow::powi_kernel`.
    ///
    /// Preferred exponent (§9.2.2): `Q(pown(x, n))` is
    /// `floor(n × Q(x))` where the result is exact.
    #[must_use]
    #[doc(alias = "pown")]
    pub fn powi(self, n: i32, rm: RoundingMode) -> (Self, Status) {
        ferrodec_transcend::pow::powi_kernel::<Decimal128>(self, n, rm)
    }

    /// Kani-only entry point that returns the IEEE 754-2019 §9.2.1
    /// special-case branch only (rules 1–7), without invoking the
    /// `Extended`-precision `exp(y · ln(x))` pipeline.
    ///
    /// This exists so symbolic proofs of the pow rule table don't drag
    /// the heavyweight transcendental path through CBMC's path
    /// explosion. Production code uses [`Decimal128::pow`]. Returns
    /// `None` for the general-path inputs (rule 8: positive-base
    /// non-special exponent). `rm` is accepted for convention parity
    /// with the other `*_special_only_for_kani` shims but ignored —
    /// rules 1–7 don't depend on rounding direction.
    #[cfg(kani)]
    #[doc(hidden)]
    #[must_use]
    pub fn pow_special_only_for_kani(self, exp: Self, _rm: RoundingMode) -> Option<(Self, Status)> {
        ferrodec_transcend::pow::pow_special_cases::<Decimal128>(self, exp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate alloc;
    use alloc::format;

    fn parse(s: &str) -> Decimal128 {
        Decimal128::parse_str(s, RoundingMode::NearestEven)
            .unwrap()
            .0
    }

    fn approx_equal_ulps(a: Decimal128, b: Decimal128, ulps: u32) -> bool {
        let (diff, _) = a.sub(b, RoundingMode::NearestEven);
        let diff = diff.abs();
        let abs_b = b.abs();
        if abs_b.is_zero() {
            let bound = parse(&format!("{ulps}e-30"));
            let (cmp, _) = diff.partial_cmp(bound);
            return matches!(
                cmp,
                Some(core::cmp::Ordering::Less | core::cmp::Ordering::Equal)
            );
        }
        let (rel, _) = diff.div(abs_b, RoundingMode::NearestEven);
        let bound = parse(&format!("{ulps}e-33"));
        let (cmp, _) = rel.partial_cmp(bound);
        matches!(
            cmp,
            Some(core::cmp::Ordering::Less | core::cmp::Ordering::Equal)
        )
    }

    #[test]
    fn pow_x_zero_is_one() {
        for x in &[
            Decimal128::ZERO,
            Decimal128::ONE,
            Decimal128::NEG_ONE,
            Decimal128::INFINITY,
            Decimal128::NEG_INFINITY,
            Decimal128::NAN,
        ] {
            let (r, _) = x.pow(Decimal128::ZERO, RoundingMode::NearestEven);
            assert_eq!(r.to_bits(), Decimal128::ONE.to_bits(), "pow({x:?}, 0)");
        }
    }

    #[test]
    fn pow_one_y_is_one() {
        for y in &[
            Decimal128::ZERO,
            Decimal128::ONE,
            parse("0.5"),
            parse("-3.14"),
            Decimal128::INFINITY,
            Decimal128::NEG_INFINITY,
            Decimal128::NAN,
        ] {
            let (r, _) = Decimal128::ONE.pow(*y, RoundingMode::NearestEven);
            assert_eq!(r.to_bits(), Decimal128::ONE.to_bits(), "pow(1, {y:?})");
        }
    }

    #[test]
    fn pow_zero_neg_is_inf_div_by_zero() {
        let (r, s) = Decimal128::ZERO.pow(Decimal128::NEG_ONE, RoundingMode::NearestEven);
        assert!(r.is_infinite());
        assert!(s.div_by_zero());

        let (r, s) = Decimal128::NEG_ZERO.pow(Decimal128::NEG_ONE, RoundingMode::NearestEven);
        assert!(r.is_infinite());
        assert!(r.is_sign_negative());
        assert!(s.div_by_zero());
    }

    #[test]
    fn pow_zero_pos_is_zero() {
        let (r, _) = Decimal128::ZERO.pow(Decimal128::ONE, RoundingMode::NearestEven);
        assert!(r.is_zero());

        let (r, _) = Decimal128::NEG_ZERO.pow(Decimal128::from_i32(3), RoundingMode::NearestEven);
        assert!(r.is_zero());
        assert!(r.is_sign_negative());

        let (r, _) = Decimal128::NEG_ZERO.pow(Decimal128::from_i32(2), RoundingMode::NearestEven);
        assert!(r.is_zero());
        assert!(!r.is_sign_negative());
    }

    #[test]
    fn pow_neg_non_integer_is_invalid_nan() {
        let (r, s) = Decimal128::NEG_ONE.pow(parse("0.5"), RoundingMode::NearestEven);
        assert!(r.is_nan());
        assert!(s.invalid());
    }

    #[test]
    fn pow_integer_basics() {
        // 2^10 = 1024
        let two = Decimal128::from_i32(2);
        let (r, _) = two.pow(Decimal128::from_i32(10), RoundingMode::NearestEven);
        let target = Decimal128::from_i32(1024);
        let (cmp, _) = r.partial_cmp(target);
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal), "2^10 = {r:?}");

        // 3^3 = 27
        let (r, _) =
            Decimal128::from_i32(3).pow(Decimal128::from_i32(3), RoundingMode::NearestEven);
        let (cmp, _) = r.partial_cmp(Decimal128::from_i32(27));
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));

        // (-2)^3 = -8
        let (r, _) =
            Decimal128::from_i32(-2).pow(Decimal128::from_i32(3), RoundingMode::NearestEven);
        let (cmp, _) = r.partial_cmp(Decimal128::from_i32(-8));
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));

        // (-2)^2 = 4
        let (r, _) =
            Decimal128::from_i32(-2).pow(Decimal128::from_i32(2), RoundingMode::NearestEven);
        let (cmp, _) = r.partial_cmp(Decimal128::from_i32(4));
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));
    }

    #[test]
    fn pow_negative_integer_inverts() {
        // 2^-3 = 0.125
        let (r, _) =
            Decimal128::from_i32(2).pow(Decimal128::from_i32(-3), RoundingMode::NearestEven);
        let target = parse("0.125");
        assert!(approx_equal_ulps(r, target, 5));
    }

    #[test]
    fn pow_inf_inf_rules() {
        // 2^Inf = Inf
        let (r, _) = Decimal128::from_i32(2).pow(Decimal128::INFINITY, RoundingMode::NearestEven);
        assert!(r.is_infinite());

        // 0.5^Inf = 0
        let (r, _) = parse("0.5").pow(Decimal128::INFINITY, RoundingMode::NearestEven);
        assert!(r.is_zero());

        // 2^-Inf = 0
        let (r, _) =
            Decimal128::from_i32(2).pow(Decimal128::NEG_INFINITY, RoundingMode::NearestEven);
        assert!(r.is_zero());

        // 0.5^-Inf = Inf
        let (r, _) = parse("0.5").pow(Decimal128::NEG_INFINITY, RoundingMode::NearestEven);
        assert!(r.is_infinite());
    }

    #[test]
    fn pow_neg_one_to_infinity_is_one() {
        // Per IEEE 754-2019 §9.2.1, pow(±1, ±∞) = 1. The previous
        // implementation panicked at unreachable!() because rule 2's
        // short-circuit only matched x = +1 (deliberately, so that
        // pow(-1, qNaN) can still propagate NaN), and rule 6 then
        // saw |x| = 1 and had no Equal arm.
        for &y in &[Decimal128::INFINITY, Decimal128::NEG_INFINITY] {
            let (r, s) = Decimal128::NEG_ONE.pow(y, RoundingMode::NearestEven);
            assert_eq!(
                r.to_bits(),
                Decimal128::ONE.to_bits(),
                "pow(-1, {y:?}) must be 1"
            );
            assert_eq!(s, Status::OK, "pow(-1, {y:?}) must not raise any flag");
        }
        // Also confirm pow(+1, ±∞) = 1 (this path used to be handled
        // by rule 2 alone; the new rule 6 Equal arm covers it too).
        for &y in &[Decimal128::INFINITY, Decimal128::NEG_INFINITY] {
            let (r, _) = Decimal128::ONE.pow(y, RoundingMode::NearestEven);
            assert_eq!(r.to_bits(), Decimal128::ONE.to_bits());
        }
    }

    #[test]
    fn pow_neg_one_qnan_propagates() {
        // Regression for the rule-2 / rule-3 interplay: extending rule
        // 2 to |x|=1 would be incorrect because pow(-1, qNaN) must
        // propagate NaN per IEEE 754-2019 §9.2.1 (rule 2 explicitly
        // covers only x = +1).
        let (r, s) = Decimal128::NEG_ONE.pow(Decimal128::NAN, RoundingMode::NearestEven);
        assert!(r.is_nan(), "pow(-1, NaN) must be NaN, got {r:?}");
        assert!(!s.invalid(), "pow(-1, qNaN) must not raise INVALID");
        // Sanity: pow(+1, qNaN) still returns 1.
        let (r, _) = Decimal128::ONE.pow(Decimal128::NAN, RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), Decimal128::ONE.to_bits());
    }

    #[test]
    fn pow_general_path_basics() {
        // 2^0.5 ≈ sqrt(2) ≈ 1.41421356...
        let (r, _) = Decimal128::from_i32(2).pow(parse("0.5"), RoundingMode::NearestEven);
        let target = parse("1.41421356237309504880168872420969808");
        assert!(
            approx_equal_ulps(r, target, 100),
            "2^0.5 = {r:?}, want ≈ {target:?}"
        );
    }

    #[test]
    fn pow_exact_results_are_not_inexact() {
        // IEEE 754-2019 §7.5: an exactly representable power must not raise
        // INEXACT (fd-92w.8). Covers rational exponents (square roots),
        // a negative rational exponent, the integer fast path, and an
        // integer exponent past the |y| ≤ 256 fast path (power of ten).
        let cases = [
            ("4", "0.5", "2"),
            ("9", "0.5", "3"),
            ("100", "0.5", "10"),
            ("4", "-0.5", "0.5"),
            ("2", "3", "8"),
            ("10", "300", "1E+300"),
        ];
        for (base, exp, want) in cases {
            let (r, s) = parse(base).pow(parse(exp), RoundingMode::NearestEven);
            let (cmp, _) = r.partial_cmp(parse(want));
            assert_eq!(
                cmp,
                Some(core::cmp::Ordering::Equal),
                "pow({base}, {exp}) = {r:?}, want {want}"
            );
            assert!(!s.inexact(), "pow({base}, {exp}) must not raise INEXACT");
        }
    }

    #[test]
    fn pow_irrational_results_are_inexact() {
        // Irrational powers keep INEXACT (guard against over-suppression).
        for (base, exp) in [("2", "0.5"), ("3", "0.5"), ("2", "0.1"), ("7", "2.5")] {
            let (_, s) = parse(base).pow(parse(exp), RoundingMode::NearestEven);
            assert!(s.inexact(), "pow({base}, {exp}) must raise INEXACT");
        }
    }
}
