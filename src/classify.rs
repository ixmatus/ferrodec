//! Classification predicates: `is_nan`, `is_infinite`, `signum`, `abs`, etc.
//!
//! These are zero-Status, branchless-on-bit-pattern operations. Every
//! method here is `const fn`.

use core::num::FpCategory;

use crate::bid::{
    classify_bits, decimal_digit_count, pack_finite, pack_quiet_nan, sign_of, type_field, Class,
    BIAS, NAN_SIGNALING_SHIFT, PRECISION, SIGN_SHIFT,
};
use crate::decimal::Decimal128;

impl Decimal128 {
    /// `true` if this value is *any* NaN (quiet or signaling).
    #[inline]
    #[must_use]
    pub const fn is_nan(self) -> bool {
        type_field(self.0) == 0b1_1111
    }

    /// `true` if this value is a signaling NaN.
    #[inline]
    #[must_use]
    pub const fn is_signaling_nan(self) -> bool {
        self.is_nan() && ((self.0 >> NAN_SIGNALING_SHIFT) & 1) == 1
    }

    /// `true` if this value is a quiet NaN (i.e. NaN and not signaling).
    #[inline]
    #[must_use]
    pub const fn is_quiet_nan(self) -> bool {
        self.is_nan() && ((self.0 >> NAN_SIGNALING_SHIFT) & 1) == 0
    }

    /// `true` if this value is ±∞.
    #[inline]
    #[must_use]
    pub const fn is_infinite(self) -> bool {
        type_field(self.0) == 0b1_1110
    }

    /// `true` if this value is finite (not NaN and not ±∞).
    #[inline]
    #[must_use]
    pub const fn is_finite(self) -> bool {
        type_field(self.0) < 0b1_1110
    }

    /// `true` if this value is ±0.
    ///
    /// Both Form A with coefficient 0 and Form B (always zero in BID-128)
    /// are recognised.
    #[inline]
    #[must_use]
    pub const fn is_zero(self) -> bool {
        matches!(classify_bits(self.0), Class::Zero { .. })
    }

    /// `true` if this value is finite, non-zero, and ≥ `MIN_POSITIVE_NORMAL`
    /// in magnitude.
    ///
    /// Subnormals — finite, non-zero values smaller than `MIN_POSITIVE_NORMAL` —
    /// return `false`. So do `±0`, `±∞`, and NaN.
    #[inline]
    #[must_use]
    pub const fn is_normal(self) -> bool {
        match classify_bits(self.0) {
            Class::Finite {
                biased_exp,
                coefficient,
                ..
            } => {
                // A value c × 10^(qe) is normal iff its magnitude is ≥ 10^E_MIN.
                // i.e. number of decimal digits in c, plus qe, ≥ E_MIN + 1.
                // Equivalently: digit_count(c) + qe > E_MIN, where qe = biased_exp - BIAS
                // and E_MIN = -(E_MAX - 1) = -(BIAS - PRECISION + 1).
                // Rearrange: digit_count(c) >= PRECISION - biased_exp ≡ biased_exp >= PRECISION - digit_count(c)
                let digits = decimal_digit_count(coefficient);
                biased_exp + digits >= PRECISION
            }
            _ => false,
        }
    }

    /// `true` if this value is finite, non-zero, and below
    /// `MIN_POSITIVE_NORMAL` in magnitude.
    #[inline]
    #[must_use]
    pub const fn is_subnormal(self) -> bool {
        match classify_bits(self.0) {
            Class::Finite {
                biased_exp,
                coefficient,
                ..
            } => {
                let digits = decimal_digit_count(coefficient);
                biased_exp + digits < PRECISION
            }
            _ => false,
        }
    }

    /// `true` if the sign bit is set. Signed zeros and signed NaN included.
    #[inline]
    #[must_use]
    pub const fn is_sign_negative(self) -> bool {
        sign_of(self.0)
    }

    /// `true` if the sign bit is clear.
    #[inline]
    #[must_use]
    pub const fn is_sign_positive(self) -> bool {
        !sign_of(self.0)
    }

    /// IEEE 754 floating-point class.
    ///
    /// Maps to [`core::num::FpCategory`] for parity with `f32`/`f64`.
    #[inline]
    #[must_use]
    pub const fn classify(self) -> FpCategory {
        match classify_bits(self.0) {
            Class::QuietNaN { .. } | Class::SignalingNaN { .. } => FpCategory::Nan,
            Class::Infinity { .. } => FpCategory::Infinite,
            Class::Zero { .. } => FpCategory::Zero,
            Class::Finite {
                biased_exp,
                coefficient,
                ..
            } => {
                let digits = decimal_digit_count(coefficient);
                if biased_exp + digits >= PRECISION {
                    FpCategory::Normal
                } else {
                    FpCategory::Subnormal
                }
            }
        }
    }

    /// Absolute value. NaN passes through (with sign cleared on the bit
    /// pattern). **No status flags raised** — matches `f64::abs`. For the
    /// IEEE 754 §5.5.1 compliant variant that raises `INVALID` on
    /// signaling NaN, use [`Decimal128::abs_with_status`].
    #[inline]
    #[must_use]
    pub const fn abs(self) -> Self {
        Self(self.0 & !(1u128 << SIGN_SHIFT))
    }

    /// IEEE 754 §5.5.1 compliant absolute value: raises `INVALID` for
    /// signaling-NaN inputs and quietens the result. Otherwise
    /// equivalent to [`Decimal128::abs`].
    #[inline]
    #[must_use]
    pub fn abs_with_status(self) -> (Self, crate::status::Status) {
        if self.is_signaling_nan() {
            return (Self::NAN, crate::status::Status::INVALID);
        }
        (self.abs(), crate::status::Status::OK)
    }

    /// Negate. Flips the sign bit, even on NaN. **No status flags raised.**
    /// For the IEEE 754 §5.5.1 compliant variant, see
    /// [`Decimal128::neg_with_status`].
    #[inline]
    #[must_use]
    pub const fn neg(self) -> Self {
        Self(self.0 ^ (1u128 << SIGN_SHIFT))
    }

    /// IEEE 754 §5.5.1 compliant negation: raises `INVALID` for
    /// signaling-NaN inputs and quietens the result.
    ///
    /// Per the General Decimal Arithmetic Specification, `minus(x)` is
    /// defined as `subtract(0, x)` under the active rounding context,
    /// which yields `+0` for zero operands under round-to-nearest-even
    /// (the default). We preserve that here: zeros return `+0` with
    /// the same cohort as `self`. Non-zero finite values bit-flip the
    /// sign as `Decimal128::neg` does.
    #[inline]
    #[must_use]
    pub fn neg_with_status(self) -> (Self, crate::status::Status) {
        if self.is_signaling_nan() {
            return (Self::NAN, crate::status::Status::INVALID);
        }
        if self.is_zero() {
            return (self.abs(), crate::status::Status::OK);
        }
        (self.neg(), crate::status::Status::OK)
    }

    /// Copy the sign of `sign` onto `self`. NaN payload preserved.
    #[inline]
    #[must_use]
    pub const fn copysign(self, sign: Self) -> Self {
        let s = sign.0 & (1u128 << SIGN_SHIFT);
        Self((self.0 & !(1u128 << SIGN_SHIFT)) | s)
    }

    /// Sign indicator.
    ///
    /// * `+1` for any positive value, including `+∞`
    /// * `−1` for any negative value, including `−∞`
    /// * `+0` / `−0` for `+0` / `−0` (sign preserved)
    /// * NaN passes through (sign preserved, payload cleared, quieted)
    ///
    /// Matches `f64::signum` semantics: equivalent to
    /// `copysign(1, x)` for non-zero, non-NaN inputs.
    #[inline]
    #[must_use]
    pub const fn signum(self) -> Self {
        if self.is_nan() {
            return Self(pack_quiet_nan(self.is_sign_negative(), 0));
        }
        let neg = self.is_sign_negative();
        if self.is_zero() {
            return if neg { Self::NEG_ZERO } else { Self::ZERO };
        }
        Self(pack_finite(neg, BIAS, 1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nan_classification() {
        assert!(Decimal128::NAN.is_nan());
        assert!(Decimal128::NAN.is_quiet_nan());
        assert!(!Decimal128::NAN.is_signaling_nan());
        assert!(!Decimal128::NAN.is_finite());
        assert!(!Decimal128::NAN.is_infinite());
        assert!(!Decimal128::NAN.is_zero());

        assert!(Decimal128::SIGNALING_NAN.is_nan());
        assert!(Decimal128::SIGNALING_NAN.is_signaling_nan());
        assert!(!Decimal128::SIGNALING_NAN.is_quiet_nan());

        assert_eq!(Decimal128::NAN.classify(), FpCategory::Nan);
        assert_eq!(Decimal128::SIGNALING_NAN.classify(), FpCategory::Nan);
    }

    #[test]
    fn infinity_classification() {
        assert!(Decimal128::INFINITY.is_infinite());
        assert!(!Decimal128::INFINITY.is_finite());
        assert!(!Decimal128::INFINITY.is_nan());
        assert!(!Decimal128::INFINITY.is_zero());
        assert_eq!(Decimal128::INFINITY.classify(), FpCategory::Infinite);

        assert!(Decimal128::NEG_INFINITY.is_infinite());
        assert!(Decimal128::NEG_INFINITY.is_sign_negative());
        assert_eq!(Decimal128::NEG_INFINITY.classify(), FpCategory::Infinite);
    }

    #[test]
    fn zero_classification() {
        for z in [Decimal128::ZERO, Decimal128::NEG_ZERO] {
            assert!(z.is_zero());
            assert!(z.is_finite());
            assert!(!z.is_normal());
            assert!(!z.is_subnormal());
            assert!(!z.is_infinite());
            assert!(!z.is_nan());
            assert_eq!(z.classify(), FpCategory::Zero);
        }
        assert!(!Decimal128::ZERO.is_sign_negative());
        assert!(Decimal128::NEG_ZERO.is_sign_negative());
    }

    #[test]
    fn one_is_normal() {
        assert!(Decimal128::ONE.is_finite());
        assert!(Decimal128::ONE.is_normal());
        assert!(!Decimal128::ONE.is_subnormal());
        assert!(!Decimal128::ONE.is_zero());
        assert_eq!(Decimal128::ONE.classify(), FpCategory::Normal);
    }

    #[test]
    fn min_positive_is_subnormal() {
        // 1 × 10^-6176 is subnormal (single digit at the smallest exponent).
        assert!(Decimal128::MIN_POSITIVE.is_finite());
        assert!(!Decimal128::MIN_POSITIVE.is_normal());
        assert!(Decimal128::MIN_POSITIVE.is_subnormal());
        assert_eq!(Decimal128::MIN_POSITIVE.classify(), FpCategory::Subnormal);
    }

    #[test]
    fn min_positive_normal_is_normal() {
        let v = Decimal128::MIN_POSITIVE_NORMAL;
        assert!(v.is_finite());
        assert!(v.is_normal());
        assert!(!v.is_subnormal());
        assert_eq!(v.classify(), FpCategory::Normal);
    }

    #[test]
    fn max_is_normal() {
        assert!(Decimal128::MAX.is_normal());
        assert!(Decimal128::MIN.is_normal());
        assert!(!Decimal128::MAX.is_sign_negative());
        assert!(Decimal128::MIN.is_sign_negative());
    }

    #[test]
    fn signum_basics() {
        assert_eq!(Decimal128::ONE.signum().to_bits(), Decimal128::ONE.to_bits());
        assert_eq!(
            Decimal128::NEG_ONE.signum().to_bits(),
            Decimal128::NEG_ONE.to_bits()
        );
        assert_eq!(Decimal128::TEN.signum().to_bits(), Decimal128::ONE.to_bits());
        assert_eq!(
            Decimal128::INFINITY.signum().to_bits(),
            Decimal128::ONE.to_bits()
        );
        assert_eq!(
            Decimal128::NEG_INFINITY.signum().to_bits(),
            Decimal128::NEG_ONE.to_bits()
        );
        assert_eq!(
            Decimal128::ZERO.signum().to_bits(),
            Decimal128::ZERO.to_bits()
        );
        assert_eq!(
            Decimal128::NEG_ZERO.signum().to_bits(),
            Decimal128::NEG_ZERO.to_bits()
        );
        assert!(Decimal128::NAN.signum().is_nan());
    }

    #[test]
    fn abs_clears_sign() {
        assert_eq!(Decimal128::NEG_ONE.abs().to_bits(), Decimal128::ONE.to_bits());
        assert_eq!(Decimal128::ONE.abs().to_bits(), Decimal128::ONE.to_bits());
        assert_eq!(
            Decimal128::NEG_ZERO.abs().to_bits(),
            Decimal128::ZERO.to_bits()
        );
        assert_eq!(
            Decimal128::NEG_INFINITY.abs().to_bits(),
            Decimal128::INFINITY.to_bits()
        );
    }

    #[test]
    fn neg_flips_sign() {
        assert_eq!(Decimal128::ONE.neg().to_bits(), Decimal128::NEG_ONE.to_bits());
        assert_eq!(Decimal128::NEG_ONE.neg().to_bits(), Decimal128::ONE.to_bits());
        assert_eq!(
            Decimal128::INFINITY.neg().to_bits(),
            Decimal128::NEG_INFINITY.to_bits()
        );
        assert_eq!(Decimal128::ZERO.neg().to_bits(), Decimal128::NEG_ZERO.to_bits());
    }

    #[test]
    fn copysign_takes_sign_from_arg() {
        assert_eq!(
            Decimal128::ONE.copysign(Decimal128::NEG_ONE).to_bits(),
            Decimal128::NEG_ONE.to_bits()
        );
        assert_eq!(
            Decimal128::NEG_ONE.copysign(Decimal128::ONE).to_bits(),
            Decimal128::ONE.to_bits()
        );
    }

    #[test]
    fn classify_categories_disjoint_for_constants() {
        let pairs = [
            (Decimal128::ZERO, FpCategory::Zero),
            (Decimal128::NEG_ZERO, FpCategory::Zero),
            (Decimal128::ONE, FpCategory::Normal),
            (Decimal128::MAX, FpCategory::Normal),
            (Decimal128::MIN_POSITIVE, FpCategory::Subnormal),
            (Decimal128::MIN_POSITIVE_NORMAL, FpCategory::Normal),
            (Decimal128::INFINITY, FpCategory::Infinite),
            (Decimal128::NEG_INFINITY, FpCategory::Infinite),
            (Decimal128::NAN, FpCategory::Nan),
            (Decimal128::SIGNALING_NAN, FpCategory::Nan),
        ];
        for (v, expected) in pairs {
            assert_eq!(v.classify(), expected, "{v:?}");
        }
    }
}
