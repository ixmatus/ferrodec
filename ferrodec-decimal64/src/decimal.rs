//! The [`Decimal64`] type — a transparent wrapper over the BID-encoded `u64`.

use crate::bid;

/// IEEE 754-2019 decimal64 floating-point value, BID-encoded.
///
/// `Decimal64` carries 16 decimal digits of precision and an exponent
/// range of `10⁻³⁸³..=10³⁸⁴`. The representation is the IEEE 754
/// BID-64 (Binary Integer Decimal) bit pattern.
///
/// ## NaN and equality
///
/// `Decimal64` deliberately implements `Eq`/`PartialEq` as **bitwise**
/// equality, not IEEE 754 numeric equality. Two values that compare
/// equal numerically (`+0` and `−0`, or different cohort encodings of
/// the same value) may have different bit patterns and therefore
/// compare unequal here. Use `Decimal64::partial_cmp` for IEEE
/// numeric comparison and `Decimal64::total_cmp` for the IEEE 754
/// totalOrder predicate (added in subsequent commits).
#[derive(Clone, Copy, Eq, PartialEq, Hash)]
#[repr(transparent)]
pub struct Decimal64(pub(crate) u64);

impl Decimal64 {
    /// Reinterpret a raw 64-bit pattern as a `Decimal64` without
    /// checking canonicality. Non-canonical inputs (Form B with
    /// coefficient ≥ 10¹⁶) decode according to IEEE 754-2019 §3.5.2 —
    /// typically as zero with the encoded sign and biased exponent.
    #[inline]
    #[must_use]
    pub const fn from_bits(bits: u64) -> Self {
        Self(bits)
    }

    /// Return the raw 64-bit BID encoding.
    #[inline]
    #[must_use]
    pub const fn to_bits(self) -> u64 {
        self.0
    }

    /// Construct a `Decimal64` from a `(coefficient, exponent)` pair.
    ///
    /// The value is `coefficient × 10ᵉˣᵖᵒⁿᵉⁿᵗ`. Returns `Err` if the
    /// coefficient's magnitude exceeds 16 decimal digits or the
    /// exponent is outside the representable range `[−398, 369]`
    /// (biased `[0, 767]`).
    pub fn try_new(coefficient: i64, exponent: i32) -> Result<Self, Decimal64BuildError> {
        let sign = coefficient < 0;
        let magnitude = coefficient.unsigned_abs();
        Self::try_new_unsigned_with_sign(sign, magnitude, exponent)
    }

    /// Construct a non-negative `Decimal64` from a `(u64, exponent)`
    /// pair.
    pub fn try_new_unsigned(coefficient: u64, exponent: i32) -> Result<Self, Decimal64BuildError> {
        Self::try_new_unsigned_with_sign(false, coefficient, exponent)
    }

    fn try_new_unsigned_with_sign(
        sign: bool,
        magnitude: u64,
        exponent: i32,
    ) -> Result<Self, Decimal64BuildError> {
        let coefficient = match bid::Coefficient::try_new(magnitude) {
            Some(c) => c,
            None => return Err(Decimal64BuildError::CoefficientOutOfRange),
        };
        let biased_exp = match bid::BiasedExp::try_from_unbiased(exponent) {
            Some(b) => b,
            None => return Err(Decimal64BuildError::ExponentOutOfRange),
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

    /// Largest representable finite value: `9.999_999_999_999_999 × 10³⁸⁴`.
    pub const MAX: Self = Self(bid::pack_finite(
        false,
        bid::BiasedExp::MAX,
        bid::Coefficient::MAX,
    ));

    /// Smallest (most negative) finite value: `−Decimal64::MAX`.
    pub const MIN: Self = Self(bid::pack_finite(
        true,
        bid::BiasedExp::MAX,
        bid::Coefficient::MAX,
    ));

    /// Smallest positive value: `1 × 10⁻³⁹⁸` (subnormal).
    pub const MIN_POSITIVE: Self = Self(bid::pack_finite(
        false,
        bid::BiasedExp::MIN,
        bid::Coefficient::ONE,
    ));

    /// Smallest positive normal value: `1 × 10⁻³⁸³`.
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

/// The decoded `(sign, coefficient, exponent)` components of a finite
/// [`Decimal64`], as returned by [`Decimal64::decode`].
///
/// The represented value is exactly
/// `(−1)^negative × coefficient × 10^exponent`. The decode is quantum
/// preserving: it returns the stored cohort member, so `1.00` (coefficient
/// `100`, exponent `−2`) and `1` (coefficient `1`, exponent `0`) decode to
/// distinct `Decimal64Parts`. Use [`Decimal64::canonicalize`] first if a
/// normalized cohort is wanted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Decimal64Parts {
    /// `true` when the value carries a negative sign, including `−0`.
    pub negative: bool,
    /// The integer coefficient (significand), in `[0, 10^16)`.
    pub coefficient: u64,
    /// The unbiased quantum exponent, in `[−398, 369]`.
    pub exponent: i16,
}

/// Error returned by [`Decimal64::try_new`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Decimal64BuildError {
    /// The coefficient magnitude is ≥ 10¹⁶ (more than 16 decimal digits).
    CoefficientOutOfRange,
    /// The exponent is outside `[−398, 369]`.
    ExponentOutOfRange,
}

impl core::fmt::Display for Decimal64BuildError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::CoefficientOutOfRange => {
                f.write_str("coefficient magnitude exceeds 16 decimal digits")
            }
            Self::ExponentOutOfRange => f.write_str(
                "unbiased exponent outside the Decimal64 quantum range [-398, 369] \
                 (Etiny = -398, the largest adjusted exponent is +384)",
            ),
        }
    }
}

impl core::error::Error for Decimal64BuildError {}

impl core::fmt::Debug for Decimal64 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // L16 (Agent 5 B11): render a *stable* classification rather
        // than forwarding the internal `bid::Class` derived `Debug`.
        // `Class` is a private implementation detail; deriving its
        // `Debug` here would leak its variant and field names into a
        // public, semver-relevant surface and couple downstream
        // snapshot tests to crate internals. These labels are part
        // of the documented `Debug` contract instead.
        write!(f, "Decimal64(0x{:016X} = ", self.0)?;
        match bid::classify_bits(self.0) {
            bid::Class::SignalingNaN { sign, payload } => {
                write!(f, "sNaN{{sign:{sign}, payload:{payload}}}")
            }
            bid::Class::QuietNaN { sign, payload } => {
                write!(f, "qNaN{{sign:{sign}, payload:{payload}}}")
            }
            bid::Class::Infinity { sign } => write!(f, "Infinity{{sign:{sign}}}"),
            bid::Class::Zero { sign, biased_exp } => {
                write!(f, "Zero{{sign:{sign}, biased_exp:{biased_exp}}}")
            }
            bid::Class::Finite {
                sign,
                biased_exp,
                coefficient,
            } => write!(
                f,
                "Finite{{sign:{sign}, biased_exp:{biased_exp}, coefficient:{coefficient}}}"
            ),
        }?;
        f.write_str(")")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_bits_to_bits_roundtrip() {
        for &bits in &[
            0u64,
            1,
            0x7800_0000_0000_0000, // +Inf
            0xF800_0000_0000_0000, // -Inf
            0x7C00_0000_0000_0000, // qNaN
            0x7E00_0000_0000_0000, // sNaN
            u64::MAX,
        ] {
            assert_eq!(Decimal64::from_bits(bits).to_bits(), bits);
        }
    }

    #[test]
    fn distinguished_constants_are_distinct() {
        let consts = [
            Decimal64::ZERO,
            Decimal64::NEG_ZERO,
            Decimal64::ONE,
            Decimal64::NEG_ONE,
            Decimal64::TEN,
            Decimal64::MAX,
            Decimal64::MIN,
            Decimal64::MIN_POSITIVE,
            Decimal64::MIN_POSITIVE_NORMAL,
            Decimal64::NAN,
            Decimal64::SIGNALING_NAN,
            Decimal64::INFINITY,
            Decimal64::NEG_INFINITY,
        ];
        for (i, a) in consts.iter().enumerate() {
            for (j, b) in consts.iter().enumerate() {
                if i != j {
                    assert_ne!(
                        a.to_bits(),
                        b.to_bits(),
                        "constants at {i} and {j} share a bit pattern"
                    );
                }
            }
        }
    }

    #[test]
    fn infinity_bit_patterns_match_intel_reference() {
        assert_eq!(Decimal64::INFINITY.to_bits(), 0x7800_0000_0000_0000);
        assert_eq!(Decimal64::NEG_INFINITY.to_bits(), 0xF800_0000_0000_0000);
        assert_eq!(Decimal64::NAN.to_bits(), 0x7C00_0000_0000_0000);
        assert_eq!(Decimal64::SIGNALING_NAN.to_bits(), 0x7E00_0000_0000_0000);
    }

    #[test]
    fn try_new_basic() {
        let x = Decimal64::try_new(123, -2).unwrap();
        assert_eq!(
            x.to_bits(),
            bid::pack_finite(
                false,
                bid::BiasedExp::try_from_biased(bid::BIAS - 2).unwrap(),
                bid::Coefficient::try_new(123).unwrap(),
            )
        );

        let neg = Decimal64::try_new(-1, 0).unwrap();
        assert_eq!(neg.to_bits(), Decimal64::NEG_ONE.to_bits());

        // 17-digit coefficient is out of range.
        let too_big = Decimal64::try_new(10_000_000_000_000_000, 0);
        assert_eq!(too_big, Err(Decimal64BuildError::CoefficientOutOfRange));

        let oob_exp = Decimal64::try_new(1, 400);
        assert_eq!(oob_exp, Err(Decimal64BuildError::ExponentOutOfRange));
    }

    #[test]
    fn max_canonical_coefficient() {
        let max_coef = bid::COEFFICIENT_LIMIT - 1; // 10^16 - 1
        let x = Decimal64::try_new_unsigned(max_coef, 0).unwrap();
        assert_eq!(
            x.to_bits(),
            bid::pack_finite(
                false,
                bid::BiasedExp::ZERO_QUANTUM,
                bid::Coefficient::try_new(max_coef).unwrap(),
            )
        );
    }
}
