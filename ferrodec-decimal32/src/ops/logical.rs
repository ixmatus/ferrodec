//! General Decimal Arithmetic digit-wise logical operations on
//! `Decimal32`: `logical_invert`, `logical_and`, `logical_or`,
//! `logical_xor`.
//!
//! Same shape as the parent and Decimal64 counterparts, sized to the
//! u32 coefficient and `PRECISION = 7`. No upstream `ds*`
//! conformance vectors exist; verification rests on the inline unit
//! tests below. See ADR-0031.

use crate::bid::{
    classify_bits, pack_finite, pack_quiet_nan, BiasedExp, Class, Coefficient, BIAS, PRECISION,
};
use crate::decimal::Decimal32;
use crate::digits::{coefficient_to_digits, digits_to_coefficient};
use ferrodec_ieee::Status;

fn as_logical_digits(d: Decimal32) -> Option<[u8; PRECISION as usize]> {
    let (sign, biased_exp, coef) = match classify_bits(d.0) {
        Class::Zero { sign, biased_exp } => (sign, biased_exp, 0u32),
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

impl Decimal32 {
    /// General Decimal Arithmetic `logical_invert(x)`.
    ///
    /// Digit-wise complement of a logical operand, padded to
    /// `PRECISION = 7` digits. See [`Decimal128::logical_invert`] for
    /// the full special-case table.
    ///
    /// [`Decimal128::logical_invert`]: ferrodec::Decimal128::logical_invert
    #[must_use]
    pub fn logical_invert(self) -> (Self, Status) {
        // GDA logical ops reject every NaN input as INVALID; the
        // logical-operand precondition is uniform across qNaN and
        // sNaN.
        match classify_bits(self.0) {
            Class::SignalingNaN { sign, payload } => {
                return (
                    Decimal32::from_bits(pack_quiet_nan(sign, payload)),
                    Status::INVALID,
                );
            }
            Class::QuietNaN { .. } => return (self, Status::INVALID),
            _ => {}
        }
        let mut digits = match as_logical_digits(self) {
            Some(d) => d,
            None => return (Self::NAN, Status::INVALID),
        };
        for d in &mut digits {
            *d = 1 - *d;
        }
        let coef = digits_to_coefficient(&digits);
        let coef_typed = Coefficient::try_new(coef).expect("digits-derived coef fits Decimal32");
        (
            Self::from_bits(pack_finite(false, BiasedExp::ZERO_QUANTUM, coef_typed)),
            Status::OK,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_ones_coef() -> u32 {
        (10u32.pow(7) - 1) / 9
    }

    #[test]
    fn invert_zero_is_all_ones() {
        let (r, st) = Decimal32::ZERO.logical_invert();
        assert!(st.is_ok());
        let expected = Decimal32::from_bits(pack_finite(
            false,
            BiasedExp::ZERO_QUANTUM,
            Coefficient::try_new(all_ones_coef()).unwrap(),
        ));
        assert_eq!(r.to_bits(), expected.to_bits());
    }

    #[test]
    fn invert_all_ones_is_zero() {
        let all_ones = Decimal32::from_bits(pack_finite(
            false,
            BiasedExp::ZERO_QUANTUM,
            Coefficient::try_new(all_ones_coef()).unwrap(),
        ));
        let (r, st) = all_ones.logical_invert();
        assert!(st.is_ok());
        assert_eq!(r.to_bits(), Decimal32::ZERO.to_bits());
    }

    #[test]
    fn negative_sign_is_invalid() {
        let neg = Decimal32::try_new(-1, 0).unwrap();
        let (r, st) = neg.logical_invert();
        assert_eq!(st, Status::INVALID);
        assert!(r.is_nan());
    }

    #[test]
    fn digit_above_one_is_invalid() {
        let (r, st) = Decimal32::try_new(2, 0).unwrap().logical_invert();
        assert_eq!(st, Status::INVALID);
        assert!(r.is_nan());
    }

    #[test]
    fn nonzero_exponent_is_invalid() {
        let (r, st) = Decimal32::try_new(1, 1).unwrap().logical_invert();
        assert_eq!(st, Status::INVALID);
        assert!(r.is_nan());
    }

    #[test]
    fn infinity_is_invalid() {
        let (r, st) = Decimal32::INFINITY.logical_invert();
        assert_eq!(st, Status::INVALID);
        assert!(r.is_nan());
    }

    #[test]
    fn signaling_nan_quiets_and_raises_invalid() {
        let (r, st) = Decimal32::SIGNALING_NAN.logical_invert();
        assert_eq!(st, Status::INVALID);
        assert!(r.is_nan());
        assert!(!r.is_signaling_nan());
    }

    #[test]
    fn quiet_nan_raises_invalid() {
        let (r, st) = Decimal32::NAN.logical_invert();
        assert_eq!(st, Status::INVALID);
        assert!(r.is_nan());
    }
}
