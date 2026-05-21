//! General Decimal Arithmetic `reduce` — strip non-significant
//! trailing zeros from a finite coefficient.
//!
//! `Decimal32` counterpart to the parent `Decimal128::reduce`; same
//! semantics, sized to the u32 coefficient and `precision = 7`. The
//! operation is always exact (never raises `INEXACT`). See ADR-0031
//! for the lens relitigation that admits this and the seven other
//! GDA extension operations into the 1.x line.

use crate::bid::{
    classify_bits, pack_finite, pack_quiet_nan, BiasedExp, Class, Coefficient, BIASED_EXP_MAX,
};
use crate::decimal::Decimal32;
use ferrodec_ieee::Status;

impl Decimal32 {
    /// General Decimal Arithmetic `reduce(x)`.
    ///
    /// Returns a value numerically equal to `self` with all
    /// non-significant trailing zeros stripped from its coefficient,
    /// the exponent adjusted upward to compensate. Sign is preserved
    /// on every input.
    ///
    /// Special cases:
    /// * `reduce(±0)` → `±0` at exponent 0 (zero of any cohort
    ///   normalises to the canonical zero quantum).
    /// * `reduce(±∞)` → `±∞` unchanged.
    /// * `reduce(qNaN)` → the NaN unchanged.
    /// * `reduce(sNaN)` → quiet NaN with the same payload, plus
    ///   `INVALID`.
    ///
    /// The operation is exact and never raises `INEXACT`. When the
    /// preferred exponent for the trailing-zero-stripped form would
    /// exceed the format's clamp limit (`BIASED_EXP_MAX = 191` for
    /// Decimal32, corresponding to unbiased exponent
    /// `Emax - precision + 1 = 90`), stripping stops at the limit
    /// and the result keeps one or more trailing zeros, matching the
    /// GDA clamp behaviour.
    ///
    /// Decimal32 has no upstream `dsReduce.decTest` vector file (the
    /// historical decSingle distribution shipped no GDA-extension
    /// vectors), so the verification of this method rests on the
    /// hand-derived unit tests below plus the cross-format property
    /// test against the conformance-proven Decimal64 implementation.
    ///
    /// # Examples
    ///
    /// ```
    /// use ferrodec_decimal32::Decimal32;
    ///
    /// // 1.00 reduces to 1.
    /// let x = Decimal32::try_new(100, -2).unwrap();
    /// let (r, st) = x.reduce();
    /// assert!(st.is_ok());
    /// assert_eq!(r.to_bits(), Decimal32::ONE.to_bits());
    /// ```
    #[must_use]
    pub fn reduce(self) -> (Self, Status) {
        let (sign, mut bexp, mut coef) = match classify_bits(self.0) {
            Class::SignalingNaN { sign, payload } => {
                return (
                    Decimal32::from_bits(pack_quiet_nan(sign, payload)),
                    Status::INVALID,
                );
            }
            Class::QuietNaN { .. } | Class::Infinity { .. } => return (self, Status::OK),
            Class::Zero { sign, .. } => {
                return (
                    Decimal32::from_bits(pack_finite(
                        sign,
                        BiasedExp::ZERO_QUANTUM,
                        Coefficient::ZERO,
                    )),
                    Status::OK,
                );
            }
            Class::Finite {
                sign,
                biased_exp,
                coefficient,
            } => (sign, biased_exp, coefficient),
        };
        while coef % 10 == 0 && bexp < BIASED_EXP_MAX {
            coef /= 10;
            bexp += 1;
        }
        let bexp_typed = BiasedExp::try_from_biased(bexp).expect("bexp in range");
        let coef_typed = Coefficient::try_new(coef).expect("coef from classify_bits");
        (
            Decimal32::from_bits(pack_finite(sign, bexp_typed, coef_typed)),
            Status::OK,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bid::BIAS;

    #[test]
    fn one_dot_zero_zero_reduces_to_one() {
        let x = Decimal32::try_new(100, -2).unwrap();
        let (r, st) = x.reduce();
        assert!(st.is_ok());
        assert_eq!(r.to_bits(), Decimal32::ONE.to_bits());
    }

    #[test]
    fn negative_one_dot_zero_zero_reduces_to_negative_one() {
        let x = Decimal32::try_new(-100, -2).unwrap();
        let (r, st) = x.reduce();
        assert!(st.is_ok());
        let expected = Decimal32::try_new(-1, 0).unwrap();
        assert_eq!(r.to_bits(), expected.to_bits());
    }

    #[test]
    fn zero_normalises_to_exponent_zero() {
        // Decimal32 unbiased exponent range is [-101, 90].
        for exp in [-5i32, -2, 0, 1, 5, 50, 90] {
            let z = Decimal32::try_new(0, exp).unwrap();
            let (r, st) = z.reduce();
            assert!(st.is_ok(), "0 at exp {exp}");
            assert_eq!(r.to_bits(), Decimal32::ZERO.to_bits(), "0 at exp {exp}");
        }
    }

    #[test]
    fn negative_zero_normalises_with_sign_preserved() {
        let neg_zero = Decimal32::from_bits(pack_finite(
            true,
            BiasedExp::try_from_biased(BIAS + 5).unwrap(),
            Coefficient::ZERO,
        ));
        let (r, _) = neg_zero.reduce();
        assert!(r.is_zero());
        assert!(r.is_sign_negative());
    }

    #[test]
    fn no_trailing_zero_leaves_coefficient_alone() {
        let x = Decimal32::try_new(1, -1).unwrap();
        let (r, st) = x.reduce();
        assert!(st.is_ok());
        assert_eq!(r.to_bits(), x.to_bits());
    }

    #[test]
    fn infinity_passes_through() {
        for inf in [Decimal32::INFINITY, Decimal32::NEG_INFINITY] {
            let (r, st) = inf.reduce();
            assert!(st.is_ok());
            assert_eq!(r.to_bits(), inf.to_bits());
        }
    }

    #[test]
    fn quiet_nan_passes_through() {
        let qnan = Decimal32::NAN;
        let (r, st) = qnan.reduce();
        assert!(st.is_ok());
        assert!(r.is_nan());
        assert!(!r.is_signaling_nan());
    }

    #[test]
    fn signaling_nan_raises_invalid_and_quiets() {
        let snan = Decimal32::SIGNALING_NAN;
        let (r, st) = snan.reduce();
        assert_eq!(st, Status::INVALID);
        assert!(r.is_nan());
        assert!(!r.is_signaling_nan());
    }

    #[test]
    fn one_million_reduces_to_one_with_exponent_six() {
        // Decimal32 max precision = 7 digits, so 10^6 (= 1_000_000)
        // is the largest "all trailing zeros" exemplar that fits.
        let x = Decimal32::try_new(1_000_000i32, 0).unwrap();
        let (r, st) = x.reduce();
        assert!(st.is_ok());
        let expected = Decimal32::try_new(1, 6).unwrap();
        assert_eq!(r.to_bits(), expected.to_bits());
    }
}
