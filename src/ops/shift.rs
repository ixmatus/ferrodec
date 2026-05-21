//! General Decimal Arithmetic `shift` — coefficient-digit shift
//! within the format's precision-wide digit window.
//!
//! `shift(x, n)` moves the digits of `x`'s coefficient `n` positions:
//! positive `n` is left shift (zero-fill on the right), negative `n`
//! is right shift (low digits dropped). `|n|` must be in
//! `[0, PRECISION]`; the rhs must be a *true integer* (finite,
//! exponent zero, coefficient quantised to an integer — `1.0` is
//! rejected even though numerically `1`). The result's sign and
//! exponent equal the lhs's. NaN propagation follows the normal rule
//! (sNaN raises `INVALID` and quietens; qNaN passes through). The op
//! never raises `INEXACT`. See ADR-0031.

use crate::bid::{classify_bits, pack_finite, Class, BIAS, PRECISION};
use crate::decimal::Decimal128;
use crate::digits::{coefficient_to_digits, digits_to_coefficient};
use crate::ops::nan_from;
use crate::status::Status;

/// Validate the rhs of `shift` / `rotate` and return the signed shift
/// count, or `None` to indicate the rhs is not a valid integer
/// magnitude `<= PRECISION`.
pub(crate) fn validate_shift_rhs(rhs: Decimal128) -> Option<i32> {
    let (sign, biased_exp, coef) = match classify_bits(rhs.0) {
        Class::Zero { sign, biased_exp } => (sign, biased_exp, 0u128),
        Class::Finite {
            sign,
            biased_exp,
            coefficient,
        } => (sign, biased_exp, coefficient),
        _ => return None,
    };
    if biased_exp != BIAS {
        return None;
    }
    if coef > u128::from(PRECISION) {
        return None;
    }
    let n = coef as i32;
    Some(if sign { -n } else { n })
}

impl Decimal128 {
    /// General Decimal Arithmetic `shift(x, n)`.
    ///
    /// Shifts the lhs coefficient by `n` digit positions inside the
    /// format's precision-wide digit window. Positive `n` is left
    /// shift (zero-fill on the right); negative `n` is right shift
    /// (low digits dropped). The result's sign and exponent equal
    /// the lhs's.
    ///
    /// `n` must encode an integer in `[-PRECISION, PRECISION]`. Any
    /// other rhs (non-zero exponent, infinity, NaN-after-precondition
    /// or magnitude out of range) raises `INVALID`. Lhs infinity
    /// passes through unchanged; qNaN-on-lhs passes through;
    /// sNaN-on-lhs raises `INVALID` and quietens.
    ///
    /// The operation is exact and never raises `INEXACT`.
    ///
    /// # Examples
    ///
    /// ```
    /// use ferrodec::Decimal128;
    ///
    /// let one = Decimal128::ONE;
    /// let two = Decimal128::try_new(2, 0).unwrap();
    /// let (r, st) = one.shift(two);
    /// assert!(st.is_ok());
    /// assert_eq!(r.to_bits(), Decimal128::try_new(100, 0).unwrap().to_bits());
    /// ```
    #[must_use]
    pub fn shift(self, rhs: Self) -> (Self, Status) {
        digit_shift(self, rhs, /*wrap=*/ false)
    }
}

/// Shared kernel for `shift` (`wrap = false`) and `rotate` (`wrap =
/// true`). The rotation variant lives in `src/ops/rotate.rs`; both
/// route through this single digit-stream manipulator.
pub(crate) fn digit_shift(lhs: Decimal128, rhs: Decimal128, wrap: bool) -> (Decimal128, Status) {
    if lhs.is_signaling_nan() {
        return (nan_from(lhs), Status::INVALID);
    }
    if rhs.is_signaling_nan() {
        return (nan_from(rhs), Status::INVALID);
    }
    if lhs.is_nan() {
        return (lhs, Status::OK);
    }
    if rhs.is_nan() {
        return (rhs, Status::OK);
    }
    let n = match validate_shift_rhs(rhs) {
        Some(v) => v,
        None => return (Decimal128::NAN, Status::INVALID),
    };
    if lhs.is_infinite() {
        return (lhs, Status::OK);
    }
    let (sign, biased_exp, coef) = match classify_bits(lhs.0) {
        Class::Zero { sign, biased_exp } => (sign, biased_exp, 0u128),
        Class::Finite {
            sign,
            biased_exp,
            coefficient,
        } => (sign, biased_exp, coefficient),
        _ => unreachable!("NaN / Infinity handled above"),
    };
    // Zero shifts and rotates trivially to itself, sign and exponent
    // preserved.
    if coef == 0 {
        return (
            Decimal128::from_bits(pack_finite(sign, biased_exp, 0)),
            Status::OK,
        );
    }
    let mut digits = [0u8; PRECISION as usize];
    coefficient_to_digits(coef, &mut digits);
    let p = PRECISION as i32;
    debug_assert!(n.abs() <= p);
    let mut out = [0u8; PRECISION as usize];
    if wrap {
        let n_mod = n.rem_euclid(p) as usize;
        for i in 0..PRECISION as usize {
            out[(i + n_mod) % PRECISION as usize] = digits[i];
        }
    } else if n > 0 {
        let n = n as usize;
        let len = PRECISION as usize - n;
        out[n..n + len].copy_from_slice(&digits[..len]);
    } else if n < 0 {
        let n_abs = (-n) as usize;
        let len = PRECISION as usize - n_abs;
        out[..len].copy_from_slice(&digits[n_abs..n_abs + len]);
    } else {
        out = digits;
    }
    let new_coef = digits_to_coefficient(&out);
    (
        Decimal128::from_bits(pack_finite(sign, biased_exp, new_coef)),
        Status::OK,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(c: i128, e: i32) -> Decimal128 {
        Decimal128::try_new(c, e).unwrap()
    }

    #[test]
    fn shift_one_by_two_is_one_hundred() {
        let (r, st) = d(1, 0).shift(d(2, 0));
        assert!(st.is_ok());
        assert_eq!(r.to_bits(), d(100, 0).to_bits());
    }

    #[test]
    fn shift_full_precision_left_drops_all_digits() {
        // shift 1 PRECISION (= 34) shifts the LSD all the way out.
        let (r, st) = d(1, 0).shift(d(34, 0));
        assert!(st.is_ok());
        assert_eq!(r.to_bits(), d(0, 0).to_bits());
    }

    #[test]
    fn shift_right_drops_low_digits() {
        // 1234 shifted right by 2 keeps the high two digits: 12.
        let (r, st) = d(1234, 0).shift(d(-2, 0));
        assert!(st.is_ok());
        assert_eq!(r.to_bits(), d(12, 0).to_bits());
    }

    #[test]
    fn shift_zero_yields_zero() {
        let (r, st) = d(0, 0).shift(d(5, 0));
        assert!(st.is_ok());
        assert!(r.is_zero());
    }

    #[test]
    fn shift_negative_lhs_preserves_sign() {
        let (r, st) = d(-1, 0).shift(d(2, 0));
        assert!(st.is_ok());
        assert_eq!(r.to_bits(), d(-100, 0).to_bits());
    }

    #[test]
    fn rhs_non_integer_is_invalid() {
        // 1.0 is numerically 1 but its exponent is -1; not an integer.
        let (r, st) = d(1, 0).shift(d(10, -1));
        assert_eq!(st, Status::INVALID);
        assert!(r.is_nan());
    }

    #[test]
    fn rhs_magnitude_above_precision_is_invalid() {
        // 35 > PRECISION = 34.
        let (r, st) = d(1, 0).shift(d(35, 0));
        assert_eq!(st, Status::INVALID);
        assert!(r.is_nan());
    }

    #[test]
    fn rhs_infinity_is_invalid() {
        let (r, st) = d(1, 0).shift(Decimal128::INFINITY);
        assert_eq!(st, Status::INVALID);
        assert!(r.is_nan());
    }

    #[test]
    fn lhs_infinity_passes_through() {
        let (r, st) = Decimal128::INFINITY.shift(d(3, 0));
        assert!(st.is_ok());
        assert_eq!(r.to_bits(), Decimal128::INFINITY.to_bits());
        let (r, _) = Decimal128::NEG_INFINITY.shift(d(3, 0));
        assert_eq!(r.to_bits(), Decimal128::NEG_INFINITY.to_bits());
    }

    #[test]
    fn quiet_nan_lhs_passes_through() {
        let (r, st) = Decimal128::NAN.shift(d(3, 0));
        assert!(st.is_ok());
        assert!(r.is_nan());
    }

    #[test]
    fn signaling_nan_lhs_quiets_and_raises_invalid() {
        let (r, st) = Decimal128::SIGNALING_NAN.shift(d(3, 0));
        assert_eq!(st, Status::INVALID);
        assert!(r.is_nan());
        assert!(!r.is_signaling_nan());
    }
}
