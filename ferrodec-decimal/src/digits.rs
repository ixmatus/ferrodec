//! Coefficient digit-array helpers shared by the digit-wise operations.
//!
//! The General Decimal Arithmetic logical operations (`and` / `or` / `xor` /
//! `invert`) and the positioning operations (`shift` / `rotate`) manipulate the
//! decimal digits of a coefficient aligned at the units position, up to the
//! context precision. These helpers convert a [`DecBig`] coefficient to and
//! from a least-significant-first digit array so those operations can work on
//! digits directly. Following the fixed-width siblings (ADR-0031) the helpers
//! live in this crate rather than in `ferrodec-multiword`: only the decimal
//! layer needs digit-granular access, and the bignum stays a pure integer type.

use alloc::vec;
use alloc::vec::Vec;
use ferrodec_multiword::DecBig;

/// Decompose `coeff` into exactly `width` decimal digits, least significant
/// first (index `0` is the units digit, each element in `0..=9`). A coefficient
/// with fewer than `width` significant digits is zero-padded on the high end;
/// one with more is truncated to its low `width` digits (the digit-wise
/// operations work within the context precision and discard higher digits).
pub(crate) fn coeff_to_digits(coeff: &DecBig, width: usize) -> Vec<u8> {
    let mut out = vec![0u8; width];
    let mut cur = coeff.clone();
    for slot in &mut out {
        if cur.is_zero() {
            break;
        }
        let (q, d) = cur.div_rem10();
        *slot = d as u8;
        cur = q;
    }
    out
}

/// Recompose a least-significant-first decimal digit slice into a [`DecBig`].
/// Each element must be in `0..=9`; most-significant zeros are insignificant
/// and drop out of the normal form.
pub(crate) fn digits_to_coeff(digits: &[u8]) -> DecBig {
    // `from_ascii_digits` consumes most-significant-first ASCII bytes, so
    // reverse the units-first slice and map each digit to its ASCII byte.
    let mut ascii = Vec::with_capacity(digits.len());
    for &d in digits.iter().rev() {
        debug_assert!(d <= 9, "decimal digit out of range");
        ascii.push(b'0' + d);
    }
    DecBig::from_ascii_digits(&ascii)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_within_width() {
        // 20509 -> [9,0,5,0,2] (units first), and back.
        let c = DecBig::from_u32(20509);
        let d = coeff_to_digits(&c, 5);
        assert_eq!(d, [9, 0, 5, 0, 2]);
        assert_eq!(digits_to_coeff(&d).cmp_ref(&c), core::cmp::Ordering::Equal);
    }

    #[test]
    fn zero_pads_on_the_high_end() {
        let c = DecBig::from_u32(7);
        assert_eq!(coeff_to_digits(&c, 4), [7, 0, 0, 0]);
    }

    #[test]
    fn truncates_to_the_low_digits() {
        // 1234567 truncated to 3 digits keeps the low three (units first).
        let c = DecBig::from_u32(1_234_567);
        assert_eq!(coeff_to_digits(&c, 3), [7, 6, 5]);
    }

    #[test]
    fn zero_is_all_zero_digits() {
        assert_eq!(coeff_to_digits(&DecBig::zero(), 3), [0, 0, 0]);
        assert!(digits_to_coeff(&[0, 0, 0]).is_zero());
    }

    #[test]
    fn high_zero_digits_drop_out() {
        // [1,0,0] units-first is the value 1; leading zeros are insignificant.
        let c = digits_to_coeff(&[1, 0, 0]);
        assert_eq!(c.to_u128(), Some(1));
    }

    #[test]
    fn wide_round_trip_past_one_limb() {
        // A value spanning more than one base-10^9 limb survives the round trip.
        let c = DecBig::from_u128(123_456_789_012_345_678_901);
        let d = coeff_to_digits(&c, 21);
        assert_eq!(digits_to_coeff(&d).cmp_ref(&c), core::cmp::Ordering::Equal);
    }
}
