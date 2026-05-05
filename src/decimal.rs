//! The [`Decimal128`] type — a transparent wrapper over the BID-encoded `u128`.

use crate::bid;

/// IEEE 754-2019 decimal128 floating-point value, BID-encoded.
///
/// `Decimal128` carries 34 decimal digits of precision and an exponent range
/// of `10^−6143 ..= 10^+6144`. The representation is the IEEE 754 BID-128
/// (Binary Integer Decimal) bit pattern.
///
/// ## NaN and equality
///
/// `Decimal128` deliberately implements `Eq`/`PartialEq` as **bitwise**
/// equality, not IEEE 754 numeric equality. Two values that compare equal
/// numerically (`+0` and `−0`, or different cohort encodings of the same
/// value) may have different bit patterns and therefore compare unequal
/// here. Use [`Decimal128::partial_cmp`] for IEEE numeric comparison and
/// [`Decimal128::total_cmp`] for the IEEE 754 totalOrder predicate.
///
/// This makes `Decimal128` usable as a `HashMap` key, predictable in tests,
/// and trivially `const`-comparable. It is the same trade-off taken by the
/// `bytemuck`/`zerocopy` ecosystem.
#[derive(Clone, Copy, Eq, PartialEq, Hash)]
#[repr(transparent)]
pub struct Decimal128(pub(crate) u128);

impl Decimal128 {
    /// Reinterpret a raw 128-bit pattern as a `Decimal128` without checking
    /// canonicality. Non-canonical inputs (Form B, `biased_exp` > 12287,
    /// coefficient ≥ 10^34) decode according to IEEE 754 — typically as
    /// zero with an unusual quantum.
    #[inline]
    #[must_use]
    pub const fn from_bits(bits: u128) -> Self {
        Self(bits)
    }

    /// Return the raw 128-bit BID encoding.
    #[inline]
    #[must_use]
    pub const fn to_bits(self) -> u128 {
        self.0
    }

    /// Construct a `Decimal128` from a `(coefficient, exponent)` pair.
    ///
    /// The value is `coefficient × 10^exponent`. Returns `Err` if the
    /// coefficient's magnitude exceeds 34 decimal digits or the exponent
    /// is outside the representable range `[−6176, 6111]`.
    ///
    /// This is a direct alternative to round-tripping through
    /// `parse_str(format!("{coef}E{exp}"))` for callers that already have
    /// the coefficient and exponent as integers.
    pub fn try_new(coefficient: i128, exponent: i32) -> Result<Self, Decimal128BuildError> {
        let sign = coefficient < 0;
        let magnitude = coefficient.unsigned_abs();
        if magnitude >= bid::COEFFICIENT_LIMIT {
            return Err(Decimal128BuildError::CoefficientOutOfRange);
        }
        let biased = exponent as i64 + bid::BIAS as i64;
        if biased < 0 || biased > bid::BIASED_EXP_MAX as i64 {
            return Err(Decimal128BuildError::ExponentOutOfRange);
        }
        Ok(Self(bid::pack_finite(sign, biased as u32, magnitude)))
    }

    // -- IEEE 754 distinguished values --------------------------------------

    /// `+0` with quantum exponent 0 (encoded as `0E+0`).
    pub const ZERO: Self = Self(bid::pack_finite(false, bid::BIAS, 0));

    /// `−0` with quantum exponent 0.
    pub const NEG_ZERO: Self = Self(bid::pack_finite(true, bid::BIAS, 0));

    /// `1` with quantum exponent 0.
    pub const ONE: Self = Self(bid::pack_finite(false, bid::BIAS, 1));

    /// `−1` with quantum exponent 0.
    pub const NEG_ONE: Self = Self(bid::pack_finite(true, bid::BIAS, 1));

    /// `+10` with quantum exponent 0.
    pub const TEN: Self = Self(bid::pack_finite(false, bid::BIAS, 10));

    /// Largest representable finite value:
    /// `9.999_999_999_999_999_999_999_999_999_999_999 × 10^6144`.
    pub const MAX: Self = Self(bid::pack_finite(
        false,
        bid::BIASED_EXP_MAX,
        bid::COEFFICIENT_LIMIT - 1,
    ));

    /// Smallest (most negative) finite value: `−Decimal128::MAX`.
    pub const MIN: Self = Self(bid::pack_finite(
        true,
        bid::BIASED_EXP_MAX,
        bid::COEFFICIENT_LIMIT - 1,
    ));

    /// Smallest positive value: `1 × 10^−6176` (subnormal).
    pub const MIN_POSITIVE: Self = Self(bid::pack_finite(false, 0, 1));

    /// Smallest positive normal value: `1 × 10^−6143`.
    ///
    /// Numbers below this magnitude (but above zero) are subnormal —
    /// representable but with reduced precision.
    pub const MIN_POSITIVE_NORMAL: Self =
        Self(bid::pack_finite(false, bid::BIAS - bid::PRECISION + 1, 1));

    /// Canonical quiet NaN with sign bit clear and a zero payload.
    pub const NAN: Self = Self(bid::pack_quiet_nan(false, 0));

    /// Canonical signaling NaN with sign bit clear and a zero payload.
    pub const SIGNALING_NAN: Self = Self(bid::pack_signaling_nan(false, 0));

    /// `+∞`.
    pub const INFINITY: Self = Self(bid::pack_infinity(false));

    /// `−∞`.
    pub const NEG_INFINITY: Self = Self(bid::pack_infinity(true));
}

/// Error returned by [`Decimal128::try_new`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Decimal128BuildError {
    /// The coefficient magnitude is ≥ 10^34 (more than 34 decimal digits).
    CoefficientOutOfRange,
    /// The exponent is outside `[−6176, 6111]`.
    ExponentOutOfRange,
}

impl core::fmt::Debug for Decimal128 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Until the `fmt` module lands, surface the underlying bit pattern
        // and the decoded class for debugging. This is `Debug`, not `Display`.
        let class = bid::classify_bits(self.0);
        write!(f, "Decimal128(0x{:032X} = {:?})", self.0, class)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_bits_to_bits_roundtrip() {
        for &bits in &[
            0u128,
            1,
            0x7800_0000_0000_0000_0000_0000_0000_0000, // +Inf
            0xF800_0000_0000_0000_0000_0000_0000_0000, // -Inf
            0x7C00_0000_0000_0000_0000_0000_0000_0000, // qNaN
            0x7E00_0000_0000_0000_0000_0000_0000_0000, // sNaN
            u128::MAX,
        ] {
            assert_eq!(Decimal128::from_bits(bits).to_bits(), bits);
        }
    }

    #[test]
    fn distinguished_constants_are_distinct() {
        let consts = [
            Decimal128::ZERO,
            Decimal128::NEG_ZERO,
            Decimal128::ONE,
            Decimal128::NEG_ONE,
            Decimal128::TEN,
            Decimal128::MAX,
            Decimal128::MIN,
            Decimal128::MIN_POSITIVE,
            Decimal128::MIN_POSITIVE_NORMAL,
            Decimal128::NAN,
            Decimal128::SIGNALING_NAN,
            Decimal128::INFINITY,
            Decimal128::NEG_INFINITY,
        ];
        for (i, a) in consts.iter().enumerate() {
            for (j, b) in consts.iter().enumerate() {
                if i != j {
                    assert_ne!(
                        a.to_bits(),
                        b.to_bits(),
                        "constants at index {i} and {j} share a bit pattern"
                    );
                }
            }
        }
    }

    #[test]
    fn infinity_bit_patterns_match_intel_reference() {
        assert_eq!(
            Decimal128::INFINITY.to_bits(),
            0x7800_0000_0000_0000_0000_0000_0000_0000
        );
        assert_eq!(
            Decimal128::NEG_INFINITY.to_bits(),
            0xF800_0000_0000_0000_0000_0000_0000_0000
        );
        assert_eq!(
            Decimal128::NAN.to_bits(),
            0x7C00_0000_0000_0000_0000_0000_0000_0000
        );
        assert_eq!(
            Decimal128::SIGNALING_NAN.to_bits(),
            0x7E00_0000_0000_0000_0000_0000_0000_0000
        );
    }
}
