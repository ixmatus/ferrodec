//! General Decimal Arithmetic `reduce` — strip non-significant
//! trailing zeros from a finite coefficient.
//!
//! `reduce(x)` returns a value numerically equal to `x` whose stored
//! representation has no trailing zeros in its coefficient (within
//! the cohort permitted by the format's clamp limit). The operation
//! is always exact: it never raises `INEXACT`.
//!
//! See ADR-0031 for the lens relitigation that admits this and the
//! seven other GDA extension operations into the 1.x line.

use crate::bid::{classify_bits, pack_finite, Class, BIAS, BIASED_EXP_MAX};
use crate::decimal::Decimal128;
use crate::ops::nan_from;
use crate::status::Status;

impl Decimal128 {
    /// General Decimal Arithmetic `reduce(x)`.
    ///
    /// Returns a value numerically equal to `self` with all
    /// non-significant trailing zeros stripped from its coefficient,
    /// the exponent adjusted upward to compensate. The operation
    /// preserves sign on every input.
    ///
    /// Special cases:
    /// * `reduce(±0)` → `±0` at exponent 0 (any zero cohort
    ///   normalises to the canonical zero quantum).
    /// * `reduce(±∞)` → `±∞` unchanged.
    /// * `reduce(qNaN)` → the NaN unchanged.
    /// * `reduce(sNaN)` → quiet NaN + `INVALID`.
    ///
    /// The operation is exact and never raises `INEXACT`. When the
    /// preferred exponent for the trailing-zero-stripped form would
    /// exceed the format's clamp limit (`BIASED_EXP_MAX`,
    /// corresponding to unbiased exponent `Emax - precision + 1`),
    /// stripping stops at the limit and the result keeps one or
    /// more trailing zeros, matching the GDA clamp behaviour.
    ///
    /// See `ddReduce.decTest` and `dqReduce.decTest` for the
    /// conformance vectors that pin every observable shape.
    ///
    /// # Examples
    ///
    /// ```
    /// use ferrodec::Decimal128;
    ///
    /// // 1.00 reduces to 1.
    /// let x = Decimal128::try_new(100, -2).unwrap();
    /// let (r, st) = x.reduce();
    /// assert!(st.is_ok());
    /// assert_eq!(r.to_bits(), Decimal128::ONE.to_bits());
    ///
    /// // 0E+5 reduces to 0; sign and exponent normalise.
    /// let z = Decimal128::try_new(0, 5).unwrap();
    /// let (r, _) = z.reduce();
    /// assert_eq!(r.to_bits(), Decimal128::ZERO.to_bits());
    /// ```
    #[must_use]
    pub fn reduce(self) -> (Self, Status) {
        if self.is_signaling_nan() {
            return (nan_from(self), Status::INVALID);
        }
        if self.is_nan() {
            return (self, Status::OK);
        }
        if self.is_infinite() {
            return (self, Status::OK);
        }
        let (sign, mut bexp, mut coef) = match classify_bits(self.0) {
            Class::Zero { sign, .. } => {
                // Zero of any cohort normalises to exponent 0 with
                // sign preserved.
                return (Self::from_bits(pack_finite(sign, BIAS, 0)), Status::OK);
            }
            Class::Finite {
                sign,
                biased_exp,
                coefficient,
            } => (sign, biased_exp, coefficient),
            _ => unreachable!(),
        };
        // Non-zero coefficient: strip trailing zeros while there's
        // exponent room. The clamp at BIASED_EXP_MAX leaves residual
        // trailing zeros if the value's cohort sits at the format's
        // upper exponent boundary, per GDA spec with `clamp: 1`.
        while coef % 10 == 0 && bexp < BIASED_EXP_MAX {
            coef /= 10;
            bexp += 1;
        }
        (Self::from_bits(pack_finite(sign, bexp, coef)), Status::OK)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_dot_zero_zero_reduces_to_one() {
        let x = Decimal128::try_new(100, -2).unwrap();
        let (r, st) = x.reduce();
        assert!(st.is_ok());
        assert_eq!(r.to_bits(), Decimal128::ONE.to_bits());
    }

    #[test]
    fn negative_one_dot_zero_zero_reduces_to_negative_one() {
        let x = Decimal128::try_new(-100, -2).unwrap();
        let (r, st) = x.reduce();
        assert!(st.is_ok());
        let expected = Decimal128::try_new(-1, 0).unwrap();
        assert_eq!(r.to_bits(), expected.to_bits());
    }

    #[test]
    fn zero_normalises_to_exponent_zero() {
        for exp in [-5i32, -2, 0, 1, 5, 100, 6111] {
            let z = Decimal128::try_new(0, exp).unwrap();
            let (r, st) = z.reduce();
            assert!(st.is_ok(), "0 at exp {exp}");
            assert_eq!(r.to_bits(), Decimal128::ZERO.to_bits(), "0 at exp {exp}");
        }
    }

    #[test]
    fn negative_zero_normalises_with_sign_preserved() {
        // `try_new` can't express -0 via i128 (which has only one zero);
        // construct directly from the BID payload.
        let neg_zero = Decimal128::from_bits(pack_finite(true, BIAS + 5, 0));
        let (r, _) = neg_zero.reduce();
        assert!(r.is_zero());
        assert!(r.is_sign_negative());
    }

    #[test]
    fn no_trailing_zero_leaves_coefficient_alone() {
        // 0.1 has coefficient 1, exponent -1; no trailing zero to
        // strip, so the representation is unchanged.
        let x = Decimal128::try_new(1, -1).unwrap();
        let (r, st) = x.reduce();
        assert!(st.is_ok());
        assert_eq!(r.to_bits(), x.to_bits());
    }

    #[test]
    fn infinity_passes_through() {
        for inf in [Decimal128::INFINITY, Decimal128::NEG_INFINITY] {
            let (r, st) = inf.reduce();
            assert!(st.is_ok());
            assert_eq!(r.to_bits(), inf.to_bits());
        }
    }

    #[test]
    fn quiet_nan_passes_through() {
        let qnan = Decimal128::NAN;
        let (r, st) = qnan.reduce();
        assert!(st.is_ok());
        assert!(r.is_nan());
        assert!(!r.is_signaling_nan());
    }

    #[test]
    fn signaling_nan_raises_invalid_and_quiets() {
        let snan = Decimal128::SIGNALING_NAN;
        let (r, st) = snan.reduce();
        assert_eq!(st, Status::INVALID);
        assert!(r.is_nan());
        assert!(!r.is_signaling_nan());
    }

    #[test]
    fn ten_million_reduces_to_one_with_exponent_seven() {
        // 10_000_000 with exponent 0 reduces to 1 with exponent 7.
        let x = Decimal128::try_new(10_000_000i128, 0).unwrap();
        let (r, st) = x.reduce();
        assert!(st.is_ok());
        let expected = Decimal128::try_new(1, 7).unwrap();
        assert_eq!(r.to_bits(), expected.to_bits());
    }
}
