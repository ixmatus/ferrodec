//! Delegating shim: the cbrt kernel moved to ferrodec-transcend
//! (P0a.2 c6). The public `Decimal128::cbrt` wrapper and its
//! behaviour tests stay here as the byte-identical regression gate.

use crate::decimal::Decimal128;
use ferrodec_ieee::{RoundingMode, Status};

impl Decimal128 {
    /// Cube root. Defined for all real `self`:
    /// `cbrt(0) = 0`, `cbrt(-x) = -cbrt(x)`.
    #[must_use]
    pub fn cbrt(self, rm: RoundingMode) -> (Self, Status) {
        ferrodec_transcend::cbrt::cbrt_kernel::<Decimal128>(self, rm)
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
    fn cbrt_zero() {
        let (r, _) = Decimal128::ZERO.cbrt(RoundingMode::NearestEven);
        assert!(r.is_zero());
        assert!(!r.is_sign_negative());
        let (r, _) = Decimal128::NEG_ZERO.cbrt(RoundingMode::NearestEven);
        assert!(r.is_zero());
        assert!(r.is_sign_negative());
    }

    #[test]
    fn cbrt_one() {
        let (r, _) = Decimal128::ONE.cbrt(RoundingMode::NearestEven);
        assert!(within_ulps(r, Decimal128::ONE, 1));
    }

    #[test]
    fn cbrt_eight() {
        let (r, _) = parse("8").cbrt(RoundingMode::NearestEven);
        assert!(within_ulps(r, parse("2"), 1));
    }

    #[test]
    fn cbrt_negative() {
        let (r, _) = parse("-27").cbrt(RoundingMode::NearestEven);
        assert!(within_ulps(r, parse("-3"), 1));
    }

    #[test]
    fn cbrt_fractional() {
        let (r, _) = parse("0.001").cbrt(RoundingMode::NearestEven);
        assert!(within_ulps(r, parse("0.1"), 1));
    }

    #[test]
    fn cbrt_inf() {
        let (r, _) = Decimal128::INFINITY.cbrt(RoundingMode::NearestEven);
        assert!(r.is_infinite() && !r.is_sign_negative());
        let (r, _) = Decimal128::NEG_INFINITY.cbrt(RoundingMode::NearestEven);
        assert!(r.is_infinite() && r.is_sign_negative());
    }

    #[test]
    fn cbrt_nan_propagates() {
        let (r, _) = Decimal128::NAN.cbrt(RoundingMode::NearestEven);
        assert!(r.is_nan());
        let (r, st) = Decimal128::SIGNALING_NAN.cbrt(RoundingMode::NearestEven);
        assert!(r.is_nan());
        assert!(st.invalid());
    }
}
