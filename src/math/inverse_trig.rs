//! Re-export + delegating shim: the inverse-trig kernel moved to
//! ferrodec-transcend (P0a.2 c9). The public
//! `Decimal128::atan` / `asin` / `acos` / `atan2` wrappers and their
//! behaviour tests stay here as the byte-identical regression gate.

use crate::decimal::Decimal128;
use ferrodec_ieee::{RoundingMode, Status};

impl Decimal128 {
    /// Inverse tangent. Range `(-π/2, +π/2)`.
    #[must_use]
    pub fn atan(self, rm: RoundingMode) -> (Self, Status) {
        ferrodec_transcend::inverse_trig::atan_kernel::<Decimal128>(self, rm)
    }

    /// Inverse sine. Domain `[-1, +1]`; outside is NaN + INVALID.
    /// Range `[-π/2, +π/2]`.
    #[must_use]
    pub fn asin(self, rm: RoundingMode) -> (Self, Status) {
        ferrodec_transcend::inverse_trig::asin_kernel::<Decimal128>(self, rm)
    }

    /// Inverse cosine. Domain `[-1, +1]`; outside is NaN + INVALID.
    /// Range `[0, π]`.
    #[must_use]
    pub fn acos(self, rm: RoundingMode) -> (Self, Status) {
        ferrodec_transcend::inverse_trig::acos_kernel::<Decimal128>(self, rm)
    }

    /// Two-argument arctangent `atan2(y, x)`. Range `(-π, +π]`.
    /// Quadrant per IEEE 754-2019 §9.2.1.
    #[must_use]
    pub fn atan2(self, x: Self, rm: RoundingMode) -> (Self, Status) {
        ferrodec_transcend::inverse_trig::atan2_kernel::<Decimal128>(self, x, rm)
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
    fn atan_zero() {
        let (r, _) = Decimal128::ZERO.atan(RoundingMode::NearestEven);
        assert!(r.is_zero());
    }

    #[test]
    fn atan_one_is_pi_over_four() {
        let (r, _) = Decimal128::ONE.atan(RoundingMode::NearestEven);
        let want = parse("0.7853981633974483096156608458198757");
        assert!(within_ulps(r, want, 1));
    }

    #[test]
    fn atan_neg_one() {
        let (r, _) = Decimal128::NEG_ONE.atan(RoundingMode::NearestEven);
        let want = parse("-0.7853981633974483096156608458198757");
        assert!(within_ulps(r, want, 1));
    }

    #[test]
    fn atan_inf() {
        let (r, _) = Decimal128::INFINITY.atan(RoundingMode::NearestEven);
        let want = parse("1.570796326794896619231321691639751");
        assert!(within_ulps(r, want, 1));
        let (r, _) = Decimal128::NEG_INFINITY.atan(RoundingMode::NearestEven);
        assert!(within_ulps(r, want.neg(), 1));
    }

    #[test]
    fn atan_small() {
        let x = parse("0.1");
        let (r, _) = x.atan(RoundingMode::NearestEven);
        let want = parse("0.09966865249116202737844611987802059");
        assert!(within_ulps(r, want, 1));
    }

    #[test]
    fn asin_zero() {
        let (r, _) = Decimal128::ZERO.asin(RoundingMode::NearestEven);
        assert!(r.is_zero());
    }

    #[test]
    fn asin_one_is_half_pi() {
        let (r, _) = Decimal128::ONE.asin(RoundingMode::NearestEven);
        let want = parse("1.570796326794896619231321691639751");
        assert!(within_ulps(r, want, 1));
    }

    #[test]
    fn asin_half_is_pi_over_six() {
        let x = parse("0.5");
        let (r, _) = x.asin(RoundingMode::NearestEven);
        let want = parse("0.5235987755982988730771072305465838");
        assert!(within_ulps(r, want, 1));
    }

    #[test]
    fn asin_out_of_range_is_invalid_nan() {
        let x = parse("1.5");
        let (r, st) = x.asin(RoundingMode::NearestEven);
        assert!(r.is_nan());
        assert!(st.invalid());
    }

    #[test]
    fn acos_zero_is_half_pi() {
        let (r, _) = Decimal128::ZERO.acos(RoundingMode::NearestEven);
        let want = parse("1.570796326794896619231321691639751");
        assert!(within_ulps(r, want, 1));
    }

    #[test]
    fn acos_one_is_zero() {
        let (r, _) = Decimal128::ONE.acos(RoundingMode::NearestEven);
        assert!(r.is_zero());
    }

    #[test]
    fn acos_neg_one_is_pi() {
        let (r, _) = Decimal128::NEG_ONE.acos(RoundingMode::NearestEven);
        let want = parse("3.141592653589793238462643383279503");
        assert!(within_ulps(r, want, 1));
    }

    #[test]
    fn atan2_first_quadrant() {
        let (r, _) = parse("1").atan2(parse("1"), RoundingMode::NearestEven);
        let want = parse("0.7853981633974483096156608458198757"); // π/4
        assert!(within_ulps(r, want, 1));
    }

    #[test]
    fn atan2_second_quadrant() {
        let (r, _) = parse("1").atan2(parse("-1"), RoundingMode::NearestEven);
        let want = parse("2.356194490192344928846982537459627"); // 3π/4
        assert!(within_ulps(r, want, 1));
    }

    #[test]
    fn atan2_third_quadrant() {
        let (r, _) = parse("-1").atan2(parse("-1"), RoundingMode::NearestEven);
        let want = parse("-2.356194490192344928846982537459627"); // -3π/4
        assert!(within_ulps(r, want, 1));
    }

    #[test]
    fn atan2_fourth_quadrant() {
        let (r, _) = parse("-1").atan2(parse("1"), RoundingMode::NearestEven);
        let want = parse("-0.7853981633974483096156608458198757"); // -π/4
        assert!(within_ulps(r, want, 1));
    }

    #[test]
    fn atan2_axis_corners() {
        let (r, _) = parse("0").atan2(parse("1"), RoundingMode::NearestEven);
        assert!(r.is_zero() && !r.is_sign_negative());
        let (r, _) = parse("0").atan2(parse("-1"), RoundingMode::NearestEven);
        let want_pi = parse("3.141592653589793238462643383279503");
        assert!(within_ulps(r, want_pi, 1));
        let (r, _) = parse("1").atan2(parse("0"), RoundingMode::NearestEven);
        let want_half_pi = parse("1.570796326794896619231321691639751");
        assert!(within_ulps(r, want_half_pi, 1));
    }
}
