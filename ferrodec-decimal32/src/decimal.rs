//! The [`Decimal32`] type — a transparent wrapper over the BID-encoded `u32`.

use crate::bid;

/// IEEE 754-2019 decimal32 floating-point value, BID-encoded.
///
/// `Decimal32` carries 7 decimal digits of precision and an exponent
/// range of `10⁻¹⁰¹..=10⁹⁶`. The representation is the IEEE 754 BID-32
/// (Binary Integer Decimal) bit pattern.
///
/// ## NaN and equality
///
/// `Decimal32` deliberately implements `Eq`/`PartialEq` as **bitwise**
/// equality, not IEEE 754 numeric equality. Two values that compare
/// equal numerically (`+0` and `−0`, or different cohort encodings of
/// the same value) may have different bit patterns and therefore
/// compare unequal here. Use `Decimal32::partial_cmp` for IEEE numeric
/// comparison and `Decimal32::total_cmp` for the IEEE 754 totalOrder
/// predicate (added in subsequent commits).
///
/// This makes `Decimal32` usable as a `HashMap` key, predictable in
/// tests, and trivially `const`-comparable. It is the same trade-off
/// taken by `bytemuck` / `zerocopy`.
#[derive(Clone, Copy, Eq, PartialEq, Hash)]
#[repr(transparent)]
pub struct Decimal32(pub(crate) u32);

impl Decimal32 {
    /// Reinterpret a raw 32-bit pattern as a `Decimal32` without checking
    /// canonicality. Non-canonical inputs (Form B with coefficient ≥
    /// 10⁷, or biased exponent encodings beyond the canonical range)
    /// decode according to IEEE 754-2019 §3.5.2 — typically as zero
    /// with the encoded sign and biased exponent.
    #[inline]
    #[must_use]
    pub const fn from_bits(bits: u32) -> Self {
        Self(bits)
    }

    /// Return the raw 32-bit BID encoding.
    #[inline]
    #[must_use]
    pub const fn to_bits(self) -> u32 {
        self.0
    }

    /// Construct a `Decimal32` from a `(coefficient, exponent)` pair.
    ///
    /// The value is `coefficient × 10ᵉˣᵖᵒⁿᵉⁿᵗ`. Returns `Err` if the
    /// coefficient's magnitude exceeds 7 decimal digits or the exponent
    /// is outside the representable range `[−101, 90]` (biased
    /// `[0, 191]`).
    pub fn try_new(coefficient: i32, exponent: i32) -> Result<Self, Decimal32BuildError> {
        let sign = coefficient < 0;
        let magnitude = coefficient.unsigned_abs();
        Self::try_new_unsigned_with_sign(sign, magnitude, exponent)
    }

    /// Construct a non-negative `Decimal32` from a `(u32, exponent)`
    /// pair. The result has `sign = false`; for negative values use
    /// [`Decimal32::try_new`] (which takes `i32`).
    pub fn try_new_unsigned(coefficient: u32, exponent: i32) -> Result<Self, Decimal32BuildError> {
        Self::try_new_unsigned_with_sign(false, coefficient, exponent)
    }

    fn try_new_unsigned_with_sign(
        sign: bool,
        magnitude: u32,
        exponent: i32,
    ) -> Result<Self, Decimal32BuildError> {
        let coefficient = match bid::Coefficient::try_new(magnitude) {
            Some(c) => c,
            None => return Err(Decimal32BuildError::CoefficientOutOfRange),
        };
        let biased_exp = match bid::BiasedExp::try_from_unbiased(exponent) {
            Some(b) => b,
            None => return Err(Decimal32BuildError::ExponentOutOfRange),
        };
        Ok(Self(bid::pack_finite(sign, biased_exp, coefficient)))
    }

    // -- IEEE 754 distinguished values --------------------------------------

    /// `+0` with quantum exponent 0 (encoded as `0E+0`).
    pub const ZERO: Self = Self(bid::pack_finite(
        false,
        bid::BiasedExp::ZERO_QUANTUM,
        bid::Coefficient::ZERO,
    ));

    /// `−0` with quantum exponent 0.
    pub const NEG_ZERO: Self = Self(bid::pack_finite(
        true,
        bid::BiasedExp::ZERO_QUANTUM,
        bid::Coefficient::ZERO,
    ));

    /// `1` with quantum exponent 0.
    pub const ONE: Self = Self(bid::pack_finite(
        false,
        bid::BiasedExp::ZERO_QUANTUM,
        bid::Coefficient::ONE,
    ));

    /// `−1` with quantum exponent 0.
    pub const NEG_ONE: Self = Self(bid::pack_finite(
        true,
        bid::BiasedExp::ZERO_QUANTUM,
        bid::Coefficient::ONE,
    ));

    /// `+10` with quantum exponent 0.
    pub const TEN: Self = Self(bid::pack_finite(
        false,
        bid::BiasedExp::ZERO_QUANTUM,
        bid::Coefficient::try_new(10).unwrap(),
    ));

    /// Largest representable finite value: `9.999999 × 10⁹⁶`.
    pub const MAX: Self = Self(bid::pack_finite(
        false,
        bid::BiasedExp::MAX,
        bid::Coefficient::MAX,
    ));

    /// Smallest (most negative) finite value: `−Decimal32::MAX`.
    pub const MIN: Self = Self(bid::pack_finite(
        true,
        bid::BiasedExp::MAX,
        bid::Coefficient::MAX,
    ));

    /// Smallest positive value: `1 × 10⁻¹⁰¹` (subnormal).
    pub const MIN_POSITIVE: Self = Self(bid::pack_finite(
        false,
        bid::BiasedExp::MIN,
        bid::Coefficient::ONE,
    ));

    /// Smallest positive normal value: `1 × 10⁻⁹⁵`.
    ///
    /// Numbers below this magnitude (but above zero) are subnormal —
    /// representable but with reduced precision.
    pub const MIN_POSITIVE_NORMAL: Self = Self(bid::pack_finite(
        false,
        bid::BiasedExp::try_from_biased(bid::BIAS - bid::PRECISION + 1).unwrap(),
        bid::Coefficient::ONE,
    ));

    /// Canonical quiet NaN with sign bit clear and a zero payload.
    pub const NAN: Self = Self(bid::pack_quiet_nan(false, 0));

    /// Canonical signaling NaN with sign bit clear and a zero payload.
    pub const SIGNALING_NAN: Self = Self(bid::pack_signaling_nan(false, 0));

    /// `+∞`.
    pub const INFINITY: Self = Self(bid::pack_infinity(false));

    /// `−∞`.
    pub const NEG_INFINITY: Self = Self(bid::pack_infinity(true));
}

/// Error returned by [`Decimal32::try_new`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Decimal32BuildError {
    /// The coefficient magnitude is ≥ 10⁷ (more than 7 decimal digits).
    CoefficientOutOfRange,
    /// The exponent is outside `[−101, 90]`.
    ExponentOutOfRange,
}

impl core::fmt::Display for Decimal32BuildError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::CoefficientOutOfRange => {
                f.write_str("coefficient magnitude exceeds 7 decimal digits")
            }
            Self::ExponentOutOfRange => f.write_str("exponent outside [-101, 90]"),
        }
    }
}

impl core::error::Error for Decimal32BuildError {}

impl core::fmt::Debug for Decimal32 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Until the `fmt` module's Display impl lands, surface the
        // underlying bit pattern and the decoded class for debugging.
        let class = bid::classify_bits(self.0);
        write!(f, "Decimal32(0x{:08X} = {:?})", self.0, class)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_bits_to_bits_roundtrip() {
        for &bits in &[
            0u32,
            1,
            0x7800_0000, // +Inf
            0xF800_0000, // -Inf
            0x7C00_0000, // qNaN
            0x7E00_0000, // sNaN
            u32::MAX,
        ] {
            assert_eq!(Decimal32::from_bits(bits).to_bits(), bits);
        }
    }

    #[test]
    fn distinguished_constants_are_distinct() {
        let consts = [
            Decimal32::ZERO,
            Decimal32::NEG_ZERO,
            Decimal32::ONE,
            Decimal32::NEG_ONE,
            Decimal32::TEN,
            Decimal32::MAX,
            Decimal32::MIN,
            Decimal32::MIN_POSITIVE,
            Decimal32::MIN_POSITIVE_NORMAL,
            Decimal32::NAN,
            Decimal32::SIGNALING_NAN,
            Decimal32::INFINITY,
            Decimal32::NEG_INFINITY,
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
        assert_eq!(Decimal32::INFINITY.to_bits(), 0x7800_0000);
        assert_eq!(Decimal32::NEG_INFINITY.to_bits(), 0xF800_0000);
        assert_eq!(Decimal32::NAN.to_bits(), 0x7C00_0000);
        assert_eq!(Decimal32::SIGNALING_NAN.to_bits(), 0x7E00_0000);
    }

    #[test]
    fn try_new_basic() {
        let x = Decimal32::try_new(123, -2).unwrap();
        assert_eq!(
            x.to_bits(),
            bid::pack_finite(
                false,
                bid::BiasedExp::try_from_biased(bid::BIAS - 2).unwrap(),
                bid::Coefficient::try_new(123).unwrap(),
            )
        );

        let neg = Decimal32::try_new(-1, 0).unwrap();
        assert_eq!(neg.to_bits(), Decimal32::NEG_ONE.to_bits());

        let too_big = Decimal32::try_new(10_000_000, 0);
        assert_eq!(too_big, Err(Decimal32BuildError::CoefficientOutOfRange));

        let oob_exp = Decimal32::try_new(1, 100);
        assert_eq!(oob_exp, Err(Decimal32BuildError::ExponentOutOfRange));
    }
}
