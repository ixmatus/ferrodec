//! General Decimal Arithmetic `rotate` on `Decimal32`. See
//! `Decimal128::rotate` and ADR-0031. Reuses the shared digit-shift
//! kernel from this module's sibling `shift.rs` with `wrap = true`.

use crate::decimal::Decimal32;
use ferrodec_ieee::Status;

impl Decimal32 {
    /// General Decimal Arithmetic `rotate(x, n)`. See
    /// `Decimal128::rotate` on the parent crate for the full contract.
    #[must_use]
    pub fn rotate(self, rhs: Self) -> (Self, Status) {
        crate::ops::shift::digit_shift(self, rhs, /*wrap=*/ true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(c: i32, e: i32) -> Decimal32 {
        Decimal32::try_new(c, e).unwrap()
    }

    #[test]
    fn rotate_full_precision_is_identity() {
        let x = d(1234567, 0);
        let (r, st) = x.rotate(d(7, 0));
        assert!(st.is_ok());
        assert_eq!(r.to_bits(), x.to_bits());
    }

    #[test]
    fn rotate_right_one_wraps_lsd_to_top() {
        let (r, st) = d(1, 0).rotate(d(-1, 0));
        assert!(st.is_ok());
        assert_eq!(r.to_bits(), d(10i32.pow(6), 0).to_bits());
    }

    #[test]
    fn rhs_magnitude_above_precision_is_invalid() {
        let (r, st) = d(1, 0).rotate(d(8, 0));
        assert_eq!(st, Status::INVALID);
        assert!(r.is_nan());
    }

    #[test]
    fn signaling_nan_lhs_quiets_and_raises_invalid() {
        let (r, st) = Decimal32::SIGNALING_NAN.rotate(d(3, 0));
        assert_eq!(st, Status::INVALID);
        assert!(r.is_nan());
        assert!(!r.is_signaling_nan());
    }
}
