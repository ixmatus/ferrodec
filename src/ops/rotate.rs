//! General Decimal Arithmetic `rotate` — modular digit rotation
//! within the format's precision-wide digit window.
//!
//! Same precondition shape as `shift` (rhs must be a true integer
//! with `|n| <= PRECISION`); the only behavioural difference is that
//! shifted-out digits wrap to the other end of the digit window
//! rather than being dropped. Reuses the shared digit-stream kernel
//! `crate::ops::shift::digit_shift` with `wrap = true`. The op is
//! exact and never raises `INEXACT`. See ADR-0031.

use crate::decimal::Decimal128;
use crate::status::Status;

impl Decimal128 {
    /// General Decimal Arithmetic `rotate(x, n)`.
    ///
    /// Rotates the lhs coefficient by `n` digit positions inside the
    /// format's precision-wide digit window. Positive `n` rotates
    /// left; negative `n` rotates right; shifted-out digits wrap to
    /// the other end. The result's sign and exponent equal the lhs's.
    ///
    /// Preconditions and special-case behaviour match
    /// [`Self::shift`]; see that method's doc comment.
    ///
    /// # Examples
    ///
    /// ```
    /// use ferrodec::Decimal128;
    ///
    /// // rotate 1 -1 — right-rotate by 1 puts the LSD at the top of
    /// // the 34-digit window.
    /// let one = Decimal128::ONE;
    /// let neg_one = Decimal128::try_new(-1, 0).unwrap();
    /// let (r, st) = one.rotate(neg_one);
    /// assert!(st.is_ok());
    /// // 10^33, the top-of-window position at PRECISION = 34.
    /// let top = Decimal128::try_new(10i128.pow(33), 0).unwrap();
    /// assert_eq!(r.to_bits(), top.to_bits());
    /// ```
    #[must_use]
    pub fn rotate(self, rhs: Self) -> (Self, Status) {
        crate::ops::shift::digit_shift(self, rhs, /*wrap=*/ true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(c: i128, e: i32) -> Decimal128 {
        Decimal128::try_new(c, e).unwrap()
    }

    #[test]
    fn rotate_full_precision_is_identity() {
        // At PRECISION = 34, rotate by 34 returns the input.
        let x = d(1234567890, 0);
        let (r, st) = x.rotate(d(34, 0));
        assert!(st.is_ok());
        assert_eq!(r.to_bits(), x.to_bits());
    }

    #[test]
    fn rotate_left_one_multiplies_low_window() {
        // rotate 1 1: LSD `1` moves to position 1, top zeros stay; in
        // a 34-digit window the result is `10`.
        let (r, st) = d(1, 0).rotate(d(1, 0));
        assert!(st.is_ok());
        assert_eq!(r.to_bits(), d(10, 0).to_bits());
    }

    #[test]
    fn rotate_right_one_wraps_lsd_to_top() {
        // rotate 1 -1: LSD `1` wraps to position PRECISION - 1 = 33;
        // coefficient = 10^33.
        let (r, st) = d(1, 0).rotate(d(-1, 0));
        assert!(st.is_ok());
        assert_eq!(r.to_bits(), d(10i128.pow(33), 0).to_bits());
    }

    #[test]
    fn rotate_zero_yields_zero() {
        let (r, st) = d(0, 0).rotate(d(5, 0));
        assert!(st.is_ok());
        assert!(r.is_zero());
    }

    #[test]
    fn rotate_negative_lhs_preserves_sign() {
        let (r, st) = d(-1, 0).rotate(d(1, 0));
        assert!(st.is_ok());
        assert_eq!(r.to_bits(), d(-10, 0).to_bits());
    }

    #[test]
    fn rhs_non_integer_is_invalid() {
        let (r, st) = d(1, 0).rotate(d(10, -1));
        assert_eq!(st, Status::INVALID);
        assert!(r.is_nan());
    }

    #[test]
    fn rhs_magnitude_above_precision_is_invalid() {
        let (r, st) = d(1, 0).rotate(d(35, 0));
        assert_eq!(st, Status::INVALID);
        assert!(r.is_nan());
    }

    #[test]
    fn lhs_infinity_passes_through() {
        let (r, st) = Decimal128::INFINITY.rotate(d(3, 0));
        assert!(st.is_ok());
        assert_eq!(r.to_bits(), Decimal128::INFINITY.to_bits());
    }

    #[test]
    fn signaling_nan_lhs_quiets_and_raises_invalid() {
        let (r, st) = Decimal128::SIGNALING_NAN.rotate(d(3, 0));
        assert_eq!(st, Status::INVALID);
        assert!(r.is_nan());
        assert!(!r.is_signaling_nan());
    }
}
