//! Re-export + delegating shim: the hyperbolic kernel moved to
//! ferrodec-transcend (P0a.2 c10). The public
//! `Decimal128::{sinh,cosh,tanh,asinh,acosh,atanh}` wrappers and
//! their behaviour tests stay here as the byte-identical regression
//! gate.

use crate::decimal::Decimal128;
use ferrodec_ieee::{RoundingMode, Status};

impl Decimal128 {
    /// Hyperbolic sine.
    #[must_use]
    pub fn sinh(self, rm: RoundingMode) -> (Self, Status) {
        ferrodec_transcend::hyperbolic::sinh_kernel::<Decimal128>(self, rm)
    }

    /// Hyperbolic cosine.
    #[must_use]
    pub fn cosh(self, rm: RoundingMode) -> (Self, Status) {
        ferrodec_transcend::hyperbolic::cosh_kernel::<Decimal128>(self, rm)
    }

    /// Hyperbolic tangent.
    #[must_use]
    pub fn tanh(self, rm: RoundingMode) -> (Self, Status) {
        ferrodec_transcend::hyperbolic::tanh_kernel::<Decimal128>(self, rm)
    }

    /// Inverse hyperbolic sine, defined for all real `self`.
    #[must_use]
    pub fn asinh(self, rm: RoundingMode) -> (Self, Status) {
        ferrodec_transcend::hyperbolic::asinh_kernel::<Decimal128>(self, rm)
    }

    /// Inverse hyperbolic cosine, defined for `self ≥ 1`.
    #[must_use]
    pub fn acosh(self, rm: RoundingMode) -> (Self, Status) {
        ferrodec_transcend::hyperbolic::acosh_kernel::<Decimal128>(self, rm)
    }

    /// Inverse hyperbolic tangent, defined for `|self| < 1`.
    #[must_use]
    pub fn atanh(self, rm: RoundingMode) -> (Self, Status) {
        ferrodec_transcend::hyperbolic::atanh_kernel::<Decimal128>(self, rm)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Decimal128 {
        Decimal128::parse_str(s, RoundingMode::NearestEven)
            .unwrap()
            .0
    }

    fn within_ulps(got: Decimal128, want: Decimal128, ulps: u32) -> bool {
        let (diff, _) = got.sub(want, RoundingMode::NearestEven);
        let diff = diff.abs();
        let abs_want = want.abs();
        if abs_want.is_zero() {
            let bound = parse(&alloc::format!("{ulps}e-30"));
            let (cmp, _) = diff.partial_cmp(bound);
            return matches!(
                cmp,
                Some(core::cmp::Ordering::Less | core::cmp::Ordering::Equal)
            );
        }
        let (rel, _) = diff.div(abs_want, RoundingMode::NearestEven);
        let bound = parse(&alloc::format!("{ulps}e-33"));
        let (cmp, _) = rel.partial_cmp(bound);
        matches!(
            cmp,
            Some(core::cmp::Ordering::Less | core::cmp::Ordering::Equal)
        )
    }

    extern crate alloc;

    #[test]
    fn sinh_zero() {
        let (r, _) = Decimal128::ZERO.sinh(RoundingMode::NearestEven);
        assert!(r.is_zero());
    }

    #[test]
    fn sinh_one() {
        let (r, _) = Decimal128::ONE.sinh(RoundingMode::NearestEven);
        let want = parse("1.175201193643801456882381850595601");
        assert!(within_ulps(r, want, 1));
    }

    #[test]
    fn cosh_zero() {
        let (r, _) = Decimal128::ZERO.cosh(RoundingMode::NearestEven);
        let (cmp, _) = r.partial_cmp(Decimal128::ONE);
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));
    }

    #[test]
    fn cosh_one() {
        let (r, _) = Decimal128::ONE.cosh(RoundingMode::NearestEven);
        let want = parse("1.543080634815243778477905620757061");
        assert!(within_ulps(r, want, 1));
    }

    #[test]
    fn tanh_zero() {
        let (r, _) = Decimal128::ZERO.tanh(RoundingMode::NearestEven);
        assert!(r.is_zero());
    }

    #[test]
    fn tanh_huge_saturates() {
        let x = parse("1000");
        let (r, _) = x.tanh(RoundingMode::NearestEven);
        let (cmp, _) = r.partial_cmp(Decimal128::ONE);
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));
    }

    #[test]
    fn tanh_neg_huge_saturates() {
        let x = parse("-1000");
        let (r, _) = x.tanh(RoundingMode::NearestEven);
        let (cmp, _) = r.partial_cmp(Decimal128::NEG_ONE);
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));
    }

    #[test]
    fn asinh_zero() {
        let (r, _) = Decimal128::ZERO.asinh(RoundingMode::NearestEven);
        assert!(r.is_zero());
    }

    #[test]
    fn asinh_one() {
        let (r, _) = Decimal128::ONE.asinh(RoundingMode::NearestEven);
        let want = parse("0.8813735870195430252326093249797923");
        assert!(within_ulps(r, want, 1));
    }

    #[test]
    fn acosh_one_is_zero() {
        let (r, _) = Decimal128::ONE.acosh(RoundingMode::NearestEven);
        assert!(r.is_zero());
    }

    #[test]
    fn acosh_two() {
        let (r, _) = parse("2").acosh(RoundingMode::NearestEven);
        let want = parse("1.316957896924816708625046347307969");
        assert!(within_ulps(r, want, 1));
    }

    #[test]
    fn acosh_below_one_is_invalid_nan() {
        let (r, st) = parse("0.5").acosh(RoundingMode::NearestEven);
        assert!(r.is_nan());
        assert!(st.invalid());
    }

    #[test]
    fn acosh_just_above_one_log1p_path() {
        // x = 1 + 10⁻³³, the smallest near-1 input the test suite can
        // build. acosh(1 + ε) ≈ √(2ε); for ε = 10⁻³³ that's √2 × 10⁻¹⁶·⁵
        // ≈ 4.472_135_954_999_579_392_818_347_337_462_552 × 10⁻¹⁷ at
        // 34 digits.
        //
        // The original `ln(x + sqrt(x² − 1))` formula loses ~33 digits
        // of precision at this input via the x²−1 cancellation; the
        // log1p path closes that gap.
        let x = parse("1.000000000000000000000000000000001");
        let (r, _) = x.acosh(RoundingMode::NearestEven);
        let want = parse("4.472135954999579392818347337462552E-17");
        assert!(
            within_ulps(r, want, 1),
            "acosh({x}) = {r:?}, want ≈ {want:?}"
        );
    }

    #[test]
    fn acosh_threshold_boundary_consistent() {
        // x = 1.01 sits right at the LOG1P_THRESHOLD. acosh(1.01) =
        // ln(1.01 + sqrt(1.01² − 1)) = ln(1 + 0.14177446878757825…) ≈
        // 0.141_303_769_485_648_577_351_151_646_974_354_6 at 34 digits.
        // The threshold's ≥ branch handles this input (y = 0.01 is
        // not strictly less than LOG1P_THRESHOLD = 0.01).
        let (r, _) = parse("1.01").acosh(RoundingMode::NearestEven);
        let want = parse("0.1413037694856485773511516469743546");
        assert!(within_ulps(r, want, 1));
    }

    #[test]
    fn atanh_zero() {
        let (r, _) = Decimal128::ZERO.atanh(RoundingMode::NearestEven);
        assert!(r.is_zero());
    }

    #[test]
    fn atanh_half() {
        let (r, _) = parse("0.5").atanh(RoundingMode::NearestEven);
        let want = parse("0.5493061443340548456976226184612628");
        assert!(within_ulps(r, want, 1));
    }

    #[test]
    fn atanh_one_is_inf() {
        let (r, st) = Decimal128::ONE.atanh(RoundingMode::NearestEven);
        assert!(r.is_infinite() && !r.is_sign_negative());
        assert!(st.div_by_zero());
    }

    #[test]
    fn atanh_above_one_is_invalid_nan() {
        let (r, st) = parse("1.5").atanh(RoundingMode::NearestEven);
        assert!(r.is_nan());
        assert!(st.invalid());
    }

    #[test]
    fn sinh_neg_x_is_neg_sinh() {
        let x = parse("0.7");
        let (s_pos, _) = x.sinh(RoundingMode::NearestEven);
        let (s_neg, _) = x.neg().sinh(RoundingMode::NearestEven);
        assert!(within_ulps(s_neg, s_pos.neg(), 1));
    }
}
