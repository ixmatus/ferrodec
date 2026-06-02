//! Conversions to and from the fixed-width ferrodec formats, behind the
//! `interop` feature.
//!
//! Every finite fixed-format value is exactly representable in the
//! arbitrary-precision type, so widening (`From`) is lossless; narrowing
//! rounds to the target format under an explicit rounding mode. Both directions
//! ride the exact decimal-string bridge: the fixed format's `Display` and the
//! arbitrary type's `parse_str` (and the reverse) are both faithful General
//! Decimal Arithmetic to-scientific encoders, so the conversion preserves the
//! exact value and cohort.

use crate::Decimal;
use alloc::string::ToString;
use ferrodec::Decimal128;
use ferrodec_decimal32::Decimal32;
use ferrodec_decimal64::Decimal64;
use ferrodec_ieee::{RoundingMode, Status};

macro_rules! widen_from {
    ($fixed:ty) => {
        impl From<$fixed> for Decimal {
            fn from(value: $fixed) -> Decimal {
                // The fixed-format Display is always a valid numeric string.
                Decimal::parse_str(&value.to_string())
                    .expect("fixed-format Display is a valid decimal string")
            }
        }
    };
}

widen_from!(Decimal128);
widen_from!(Decimal64);
widen_from!(Decimal32);

impl Decimal {
    /// Round `self` into a [`Decimal128`], returning the value and status.
    #[must_use]
    pub fn to_decimal128(&self, rounding: RoundingMode) -> (Decimal128, Status) {
        Decimal128::parse_str(&self.to_string(), rounding)
            .expect("arbitrary Display is a valid decimal string")
    }

    /// Round `self` into a [`Decimal64`].
    #[must_use]
    pub fn to_decimal64(&self, rounding: RoundingMode) -> (Decimal64, Status) {
        Decimal64::parse_str(&self.to_string(), rounding)
            .expect("arbitrary Display is a valid decimal string")
    }

    /// Round `self` into a [`Decimal32`].
    #[must_use]
    pub fn to_decimal32(&self, rounding: RoundingMode) -> (Decimal32, Status) {
        Decimal32::parse_str(&self.to_string(), rounding)
            .expect("arbitrary Display is a valid decimal string")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrodec_multiword::DecBig;

    #[test]
    fn widen_is_lossless() {
        // A Decimal128 widens to the same value and cohort.
        let (d128, _) = Decimal128::parse_str("1.230", RoundingMode::NearestEven).unwrap();
        let wide: Decimal = d128.into();
        assert_eq!(wide, Decimal::finite(false, DecBig::from_u32(1230), -3));
        // Specials widen too.
        let (inf, _) = Decimal128::parse_str("-Infinity", RoundingMode::NearestEven).unwrap();
        assert!(Decimal::from(inf).is_infinite() && Decimal::from(inf).is_negative());
    }

    #[test]
    fn narrow_rounds_to_format() {
        // An arbitrary value with more than 16 digits narrows to Decimal64
        // (16 digits) with the inexact flag.
        let wide = Decimal::parse_str("1.234567890123456789").unwrap();
        let (d64, status) = wide.to_decimal64(RoundingMode::NearestEven);
        assert_eq!(d64.to_string(), "1.234567890123457");
        assert!(status.inexact());
        // A value that fits narrows exactly.
        let exact = Decimal::parse_str("2.5").unwrap();
        let (d32, st) = exact.to_decimal32(RoundingMode::NearestEven);
        assert_eq!(d32.to_string(), "2.5");
        assert!(st.is_ok());
    }

    #[test]
    fn round_trip_through_arbitrary() {
        // Widening then narrowing back is the identity for an in-format value.
        let (d128, _) = Decimal128::parse_str("3.14159", RoundingMode::NearestEven).unwrap();
        let wide: Decimal = d128.into();
        let (back, st) = wide.to_decimal128(RoundingMode::NearestEven);
        assert_eq!(back, d128);
        assert!(st.is_ok());
    }
}
