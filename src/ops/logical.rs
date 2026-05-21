//! General Decimal Arithmetic digit-wise logical operations:
//! `logical_invert`, `logical_and`, `logical_or`, `logical_xor`.
//!
//! All four take *logical operands*: finite non-negative integers
//! whose coefficient digits all lie in `{0, 1}` and whose exponent is
//! zero. Any other input raises `Invalid_operation` and yields a
//! quiet NaN. The result is a logical operand at the format's full
//! precision: positive sign, exponent zero, every digit in `{0, 1}`,
//! padded to `PRECISION` digits where the input was shorter.
//!
//! Each op is exact and never raises `INEXACT`. See ADR-0031 for the
//! lens relitigation admitting these and the seven other GDA
//! extensions into the 1.x line.

use crate::bid::{classify_bits, pack_finite, Class, BIAS, PRECISION};
use crate::decimal::Decimal128;
use crate::digits::{coefficient_to_digits, digits_to_coefficient};
use crate::ops::nan_from;
use crate::status::Status;

/// Returns `Some(digits)` if `d` is a *logical operand* (positive sign,
/// exponent zero, every digit in `{0, 1}`) and `None` otherwise. The
/// returned buffer is LSD-first and padded with zeros to PRECISION.
fn as_logical_digits(d: Decimal128) -> Option<[u8; PRECISION as usize]> {
    let (sign, biased_exp, coef) = match classify_bits(d.0) {
        Class::Zero { sign, biased_exp } => (sign, biased_exp, 0u128),
        Class::Finite {
            sign,
            biased_exp,
            coefficient,
        } => (sign, biased_exp, coefficient),
        _ => return None,
    };
    if sign || biased_exp != BIAS {
        return None;
    }
    let mut buf = [0u8; PRECISION as usize];
    coefficient_to_digits(coef, &mut buf);
    for &b in &buf {
        if b > 1 {
            return None;
        }
    }
    Some(buf)
}

impl Decimal128 {
    /// General Decimal Arithmetic `logical_invert(x)`.
    ///
    /// Digit-wise complement of a logical operand: each base-10 digit
    /// (which must be 0 or 1 on input) flips to its opposite, with the
    /// result padded on the left to the format's `PRECISION = 34`
    /// digits. `logical_invert(0) = 10^34 - 1` (all 34 ones);
    /// `logical_invert(1) = (10^34 - 1) - 1`.
    ///
    /// Logical-operand precondition: the input must be a finite
    /// non-negative integer at exponent zero with every digit in
    /// `{0, 1}`. Any violation (negative sign, non-zero exponent,
    /// digit `≥ 2`, infinity, NaN-with-payload outside the
    /// signaling-quietens path) raises `INVALID` and returns a quiet
    /// NaN. Signaling NaN raises `INVALID` and quiets, preserving the
    /// payload.
    ///
    /// The operation is exact and never raises `INEXACT`.
    ///
    /// # Examples
    ///
    /// ```
    /// use ferrodec::Decimal128;
    ///
    /// let zero = Decimal128::ZERO;
    /// let (r, st) = zero.logical_invert();
    /// assert!(st.is_ok());
    /// // 10^34 - 1, 34 ones in base 10.
    /// let all_ones = Decimal128::try_new(10i128.pow(34) - 1, 0).unwrap();
    /// assert_eq!(r.to_bits(), all_ones.to_bits());
    /// ```
    #[must_use]
    pub fn logical_invert(self) -> (Self, Status) {
        // GDA logical ops reject every NaN input as INVALID, not just
        // signaling NaN — the logical-operand precondition is global.
        if self.is_signaling_nan() {
            return (nan_from(self), Status::INVALID);
        }
        if self.is_nan() {
            return (self, Status::INVALID);
        }
        let mut digits = match as_logical_digits(self) {
            Some(d) => d,
            None => return (Self::NAN, Status::INVALID),
        };
        for d in &mut digits {
            *d = 1 - *d;
        }
        let coef = digits_to_coefficient(&digits);
        (Self::from_bits(pack_finite(false, BIAS, coef)), Status::OK)
    }

    /// General Decimal Arithmetic `logical_and(x, y)`. Digit-wise
    /// boolean AND over two logical operands, padded to the format's
    /// `PRECISION = 34` digits. See [`Self::logical_invert`] for the
    /// shared precondition and NaN-as-INVALID rule.
    #[must_use]
    pub fn logical_and(self, rhs: Self) -> (Self, Status) {
        logical_binary(self, rhs, |a, b| a & b)
    }

    /// General Decimal Arithmetic `logical_or(x, y)`. Digit-wise
    /// boolean OR.
    #[must_use]
    pub fn logical_or(self, rhs: Self) -> (Self, Status) {
        logical_binary(self, rhs, |a, b| a | b)
    }

    /// General Decimal Arithmetic `logical_xor(x, y)`. Digit-wise
    /// boolean XOR.
    #[must_use]
    pub fn logical_xor(self, rhs: Self) -> (Self, Status) {
        logical_binary(self, rhs, |a, b| a ^ b)
    }
}

/// Shared kernel for `logical_and / or / xor`. `op` is the 2-bit
/// truth-table function applied digit-wise. Both operands must be
/// logical operands; otherwise the result is `(NaN, INVALID)`.
fn logical_binary(a: Decimal128, b: Decimal128, op: fn(u8, u8) -> u8) -> (Decimal128, Status) {
    // Reject every NaN (qNaN or sNaN) on either side; sNaN quietens.
    if a.is_signaling_nan() {
        return (nan_from(a), Status::INVALID);
    }
    if b.is_signaling_nan() {
        return (nan_from(b), Status::INVALID);
    }
    if a.is_nan() {
        return (a, Status::INVALID);
    }
    if b.is_nan() {
        return (b, Status::INVALID);
    }
    let da = match as_logical_digits(a) {
        Some(d) => d,
        None => return (Decimal128::NAN, Status::INVALID),
    };
    let db = match as_logical_digits(b) {
        Some(d) => d,
        None => return (Decimal128::NAN, Status::INVALID),
    };
    let mut out = [0u8; PRECISION as usize];
    for i in 0..(PRECISION as usize) {
        out[i] = op(da[i], db[i]);
    }
    let coef = digits_to_coefficient(&out);
    (
        Decimal128::from_bits(pack_finite(false, BIAS, coef)),
        Status::OK,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invert_zero_is_all_ones() {
        let (r, st) = Decimal128::ZERO.logical_invert();
        assert!(st.is_ok());
        let all_ones = Decimal128::try_new(10i128.pow(34) - 1, 0).unwrap();
        // 10^34 - 1 is the 34-digit all-nines integer; we want all
        // 34 *ones*, which is (10^34 - 1) / 9.
        let expected_coef = (10u128.pow(34) - 1) / 9;
        assert_eq!(
            r.to_bits(),
            Decimal128::from_bits(pack_finite(false, BIAS, expected_coef)).to_bits()
        );
        let _ = all_ones; // alias kept for readability above
    }

    #[test]
    fn invert_one_is_all_ones_minus_one() {
        let one = Decimal128::ONE;
        let (r, _) = one.logical_invert();
        let ones_34 = (10u128.pow(34) - 1) / 9;
        let expected = ones_34 - 1;
        assert_eq!(
            r.to_bits(),
            Decimal128::from_bits(pack_finite(false, BIAS, expected)).to_bits()
        );
    }

    #[test]
    fn invert_all_ones_is_zero() {
        let ones_34 = (10u128.pow(34) - 1) / 9;
        let all_ones = Decimal128::from_bits(pack_finite(false, BIAS, ones_34));
        let (r, st) = all_ones.logical_invert();
        assert!(st.is_ok());
        assert_eq!(r.to_bits(), Decimal128::ZERO.to_bits());
    }

    #[test]
    fn negative_sign_is_invalid() {
        let neg = Decimal128::try_new(-1, 0).unwrap();
        let (r, st) = neg.logical_invert();
        assert_eq!(st, Status::INVALID);
        assert!(r.is_nan());
    }

    #[test]
    fn nonzero_exponent_is_invalid() {
        let bad = Decimal128::try_new(1, 1).unwrap();
        let (r, st) = bad.logical_invert();
        assert_eq!(st, Status::INVALID);
        assert!(r.is_nan());
    }

    #[test]
    fn digit_above_one_is_invalid() {
        let two = Decimal128::try_new(2, 0).unwrap();
        let (r, st) = two.logical_invert();
        assert_eq!(st, Status::INVALID);
        assert!(r.is_nan());
    }

    #[test]
    fn infinity_is_invalid() {
        let (r, st) = Decimal128::INFINITY.logical_invert();
        assert_eq!(st, Status::INVALID);
        assert!(r.is_nan());
    }

    #[test]
    fn quiet_nan_raises_invalid() {
        // GDA logical ops are special: qNaN input is rejected as
        // INVALID, not passed through as OK.
        let (r, st) = Decimal128::NAN.logical_invert();
        assert_eq!(st, Status::INVALID);
        assert!(r.is_nan());
    }

    #[test]
    fn signaling_nan_quiets_and_raises_invalid() {
        let (r, st) = Decimal128::SIGNALING_NAN.logical_invert();
        assert_eq!(st, Status::INVALID);
        assert!(r.is_nan());
        assert!(!r.is_signaling_nan());
    }

    #[test]
    fn and_one_zero_is_zero() {
        let one = Decimal128::ONE;
        let zero = Decimal128::ZERO;
        let (r, st) = one.logical_and(zero);
        assert!(st.is_ok());
        assert_eq!(r.to_bits(), Decimal128::ZERO.to_bits());
    }

    #[test]
    fn and_one_one_is_one_padded() {
        // logical_and(1, 1) = 1 padded with leading zeros to 34
        // digits — numerically just 1, but with the canonical
        // logical-operand exponent (BIAS).
        let one = Decimal128::ONE;
        let (r, st) = one.logical_and(one);
        assert!(st.is_ok());
        assert_eq!(r.to_bits(), one.to_bits());
    }

    #[test]
    fn or_zero_zero_is_zero() {
        let (r, st) = Decimal128::ZERO.logical_or(Decimal128::ZERO);
        assert!(st.is_ok());
        assert_eq!(r.to_bits(), Decimal128::ZERO.to_bits());
    }

    #[test]
    fn xor_one_one_is_zero() {
        let (r, st) = Decimal128::ONE.logical_xor(Decimal128::ONE);
        assert!(st.is_ok());
        assert_eq!(r.to_bits(), Decimal128::ZERO.to_bits());
    }

    #[test]
    fn xor_one_zero_is_one() {
        let (r, st) = Decimal128::ONE.logical_xor(Decimal128::ZERO);
        assert!(st.is_ok());
        assert_eq!(r.to_bits(), Decimal128::ONE.to_bits());
    }

    #[test]
    fn binary_negative_operand_is_invalid() {
        let neg = Decimal128::try_new(-1, 0).unwrap();
        let (r, st) = neg.logical_and(Decimal128::ONE);
        assert_eq!(st, Status::INVALID);
        assert!(r.is_nan());
        let (r, st) = Decimal128::ONE.logical_or(neg);
        assert_eq!(st, Status::INVALID);
        assert!(r.is_nan());
    }

    #[test]
    fn binary_nan_raises_invalid() {
        let (r, st) = Decimal128::NAN.logical_and(Decimal128::ONE);
        assert_eq!(st, Status::INVALID);
        assert!(r.is_nan());
        let (r, st) = Decimal128::ONE.logical_xor(Decimal128::SIGNALING_NAN);
        assert_eq!(st, Status::INVALID);
        assert!(r.is_nan());
        assert!(!r.is_signaling_nan());
    }
}
