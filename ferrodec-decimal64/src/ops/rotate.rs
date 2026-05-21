//! General Decimal Arithmetic `rotate` on `Decimal64`. See
//! `Decimal128::rotate` and ADR-0031. Reuses the shared digit-shift
//! kernel from this module's sibling `shift.rs` with `wrap = true`.

use crate::decimal::Decimal64;
use ferrodec_ieee::Status;

impl Decimal64 {
    /// General Decimal Arithmetic `rotate(x, n)`. See
    /// [`Decimal128::rotate`] for the full contract.
    ///
    /// [`Decimal128::rotate`]: ferrodec::Decimal128::rotate
    #[must_use]
    pub fn rotate(self, rhs: Self) -> (Self, Status) {
        crate::ops::shift::digit_shift(self, rhs, /*wrap=*/ true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(c: i64, e: i32) -> Decimal64 {
        Decimal64::try_new(c, e).unwrap()
    }

    #[test]
    fn rotate_full_precision_is_identity() {
        let x = d(1234567890123, 0);
        let (r, st) = x.rotate(d(16, 0));
        assert!(st.is_ok());
        assert_eq!(r.to_bits(), x.to_bits());
    }

    #[test]
    fn rotate_left_one_multiplies_low_window() {
        let (r, st) = d(1, 0).rotate(d(1, 0));
        assert!(st.is_ok());
        assert_eq!(r.to_bits(), d(10, 0).to_bits());
    }

    #[test]
    fn rotate_right_one_wraps_lsd_to_top() {
        let (r, st) = d(1, 0).rotate(d(-1, 0));
        assert!(st.is_ok());
        assert_eq!(r.to_bits(), d(10i64.pow(15), 0).to_bits());
    }

    #[test]
    fn rhs_magnitude_above_precision_is_invalid() {
        let (r, st) = d(1, 0).rotate(d(17, 0));
        assert_eq!(st, Status::INVALID);
        assert!(r.is_nan());
    }

    #[test]
    fn signaling_nan_lhs_quiets_and_raises_invalid() {
        let (r, st) = Decimal64::SIGNALING_NAN.rotate(d(3, 0));
        assert_eq!(st, Status::INVALID);
        assert!(r.is_nan());
        assert!(!r.is_signaling_nan());
    }
}
