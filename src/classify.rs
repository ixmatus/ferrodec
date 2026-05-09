//! Classification predicates: `is_nan`, `is_infinite`, `signum`, `abs`, etc.
//!
//! These are zero-Status, branchless-on-bit-pattern operations. Every
//! method here is `const fn`.

use core::num::FpCategory;

use crate::bid::{
    classify_bits, decimal_digit_count, pack_finite, pack_infinity, pack_quiet_nan,
    pack_signaling_nan, sign_of, type_field, Class, BIAS, COEFFICIENT_LIMIT, NAN_SIGNALING_SHIFT,
    PRECISION, SIGN_SHIFT, T_BITS, T_MASK,
};
use crate::decimal::Decimal128;

/// IEEE 754-2019 §5.7.2 `class(x)` enum, exposing all ten standard
/// classes a decimal floating-point datum can occupy.
///
/// Each value of [`Decimal128`] belongs to exactly one variant.
/// Use [`Decimal128::ieee_class`] to obtain it. The standard's class
/// operation is required to be quiet — calling `ieee_class` on a
/// signaling NaN does *not* raise `Status::INVALID`.
///
/// NaN classes do not carry sign by IEEE convention: a sign bit set
/// on a NaN is observable through [`Decimal128::is_sign_negative`]
/// but does not split [`IeeeClass::QuietNaN`] or
/// [`IeeeClass::SignalingNaN`] into signed variants.
///
/// For a coarser classification matching `f32` / `f64`, use
/// [`Decimal128::classify`], which returns [`core::num::FpCategory`]
/// (five variants).
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum IeeeClass {
    /// Signaling NaN. Most operations consume this and raise
    /// `Status::INVALID`; the class operation itself is quiet
    /// (per IEEE 754-2019 §5.7.2).
    SignalingNaN,
    /// Quiet NaN. Propagates through arithmetic without raising
    /// `Status::INVALID` (a property called *quiet* propagation).
    QuietNaN,
    /// `−∞`.
    NegativeInfinity,
    /// Negative finite value with magnitude at or above
    /// [`Decimal128::MIN_POSITIVE_NORMAL`] (`10^−6143`).
    NegativeNormal,
    /// Negative finite value with magnitude strictly below
    /// [`Decimal128::MIN_POSITIVE_NORMAL`] but strictly above zero.
    NegativeSubnormal,
    /// `−0`. Distinct from [`IeeeClass::PositiveZero`] under
    /// [`Decimal128::total_cmp`] but equal under
    /// [`Decimal128::partial_cmp`].
    NegativeZero,
    /// `+0`. See [`IeeeClass::NegativeZero`] for the comparison
    /// semantics.
    PositiveZero,
    /// Positive finite value strictly below
    /// [`Decimal128::MIN_POSITIVE_NORMAL`] and strictly above zero.
    PositiveSubnormal,
    /// Positive finite value at or above
    /// [`Decimal128::MIN_POSITIVE_NORMAL`].
    PositiveNormal,
    /// `+∞`.
    PositiveInfinity,
}

/// Maximum canonical NaN payload: `10^33` (i.e. payload representable as a
/// 33-decimal-digit integer). Used by `is_canonical` and `canonicalize`.
const MAX_CANONICAL_NAN_PAYLOAD: u128 = 10u128.pow(33);

/// Mask for bits 120..110 — the EC slots that are *unused* in NaN
/// encodings (the signaling marker is bit 121; the payload occupies bits
/// 109..0). For canonical NaN these eleven bits must be zero.
const NAN_HIGH_UNUSED_MASK: u128 = ((1u128 << NAN_SIGNALING_SHIFT) - 1) & !T_MASK;

/// Mask for bits 121..0 — everything below the 5-bit type field. For
/// canonical Infinity these bits must all be zero.
const INF_BELOW_TYPE_MASK: u128 = (1u128 << 122) - 1;

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
    /// For the full IEEE 754-2019 §5.7.2 ten-class enumeration that
    /// distinguishes sign and signaling-NaN versus quiet-NaN, see
    /// [`Decimal128::ieee_class`].
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

    /// IEEE 754-2019 §5.7.2 `class(x)` operation.
    ///
    /// Returns the value's exact class out of the ten the standard
    /// distinguishes (see [`IeeeClass`] for the variant list).
    /// Quiet by IEEE definition — does *not* raise `Status::INVALID`
    /// on a signaling-NaN input.
    ///
    /// # Examples
    ///
    /// ```
    /// use ferrodec::{Decimal128, IeeeClass};
    /// assert_eq!(Decimal128::ONE.ieee_class(), IeeeClass::PositiveNormal);
    /// assert_eq!(Decimal128::NEG_ZERO.ieee_class(), IeeeClass::NegativeZero);
    /// assert_eq!(Decimal128::INFINITY.ieee_class(), IeeeClass::PositiveInfinity);
    /// assert_eq!(Decimal128::SIGNALING_NAN.ieee_class(), IeeeClass::SignalingNaN);
    /// assert_eq!(
    ///     Decimal128::MIN_POSITIVE.ieee_class(),
    ///     IeeeClass::PositiveSubnormal,
    /// );
    /// ```
    #[inline]
    #[must_use]
    pub const fn ieee_class(self) -> IeeeClass {
        match classify_bits(self.0) {
            Class::SignalingNaN { .. } => IeeeClass::SignalingNaN,
            Class::QuietNaN { .. } => IeeeClass::QuietNaN,
            Class::Infinity { sign: true } => IeeeClass::NegativeInfinity,
            Class::Infinity { sign: false } => IeeeClass::PositiveInfinity,
            Class::Zero { sign: true, .. } => IeeeClass::NegativeZero,
            Class::Zero { sign: false, .. } => IeeeClass::PositiveZero,
            Class::Finite {
                sign,
                biased_exp,
                coefficient,
            } => {
                // Same normal/subnormal split as `classify`.
                let digits = decimal_digit_count(coefficient);
                let normal = biased_exp + digits >= PRECISION;
                match (sign, normal) {
                    (true, true) => IeeeClass::NegativeNormal,
                    (true, false) => IeeeClass::NegativeSubnormal,
                    (false, true) => IeeeClass::PositiveNormal,
                    (false, false) => IeeeClass::PositiveSubnormal,
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

    /// IEEE 754-2019 §5.7.2 `isCanonical(x)`.
    ///
    /// For BID-128 a bit pattern is canonical iff:
    ///
    /// * **Finite (Form A)**: coefficient `< 10^34`. Form A holds
    ///   coefficients up to `2^113 − 1 ≈ 1.038 × 10^34`; values in
    ///   `[10^34, 2^113)` are non-canonical and decode to `±0` per
    ///   §3.5.2.
    /// * **Form B**: never canonical for BID-128 (the implicit `100`
    ///   prefix forces the coefficient ≥ `2^113 > 10^34`).
    /// * **Infinity**: bits 121..0 are all zero. The trailing significand
    ///   and exponent continuation are unused in `±∞` encodings.
    /// * **NaN**: bits 120..110 are zero *and* the payload `< 10^33`.
    ///   Bit 121 is the signaling marker and is part of the canonical
    ///   encoding; the eleven bits between it and the payload must be
    ///   zero.
    ///
    /// No status flags raised. See [`Decimal128::canonicalize`] for the
    /// rewrite operation.
    ///
    /// # Examples
    ///
    /// ```
    /// use ferrodec::Decimal128;
    ///
    /// // Distinguished constants are canonical.
    /// assert!(Decimal128::ONE.is_canonical());
    /// assert!(Decimal128::INFINITY.is_canonical());
    /// assert!(Decimal128::NAN.is_canonical());
    ///
    /// // Junk bits below the Inf type field break canonicity.
    /// let dirty = Decimal128::from_bits(Decimal128::INFINITY.to_bits() | 0xFF);
    /// assert!(!dirty.is_canonical());
    /// ```
    #[inline]
    #[must_use]
    pub const fn is_canonical(self) -> bool {
        let bits = self.0;
        let t = type_field(bits);
        if t == 0b1_1110 {
            return (bits & INF_BELOW_TYPE_MASK) == 0;
        }
        if t == 0b1_1111 {
            if (bits & NAN_HIGH_UNUSED_MASK) != 0 {
                return false;
            }
            return (bits & T_MASK) < MAX_CANONICAL_NAN_PAYLOAD;
        }
        // Finite branch. Top two bits of the type field separate Form A
        // (00 / 01 / 10) from Form B (11, with sub-types non-Inf-non-NaN).
        if (t >> 3) == 0b11 {
            return false;
        }
        let coef = (((t & 0b111) as u128) << T_BITS) | (bits & T_MASK);
        coef < COEFFICIENT_LIMIT
    }

    /// IEEE 754-2019 §5.4.2 `canonicalize(x)`.
    ///
    /// Returns the canonical encoding of `self`. Quiet — never raises
    /// any status flag.
    ///
    /// Rewrites:
    /// * Non-canonical finite (Form B, or Form A with coefficient
    ///   ≥ `10^34`) → `±0` at the same quantum exponent (matches
    ///   IEEE 754 §3.5.2 decoding behavior).
    /// * Infinity with junk bits set → canonical `±∞`.
    /// * NaN with bits 120..110 set or payload ≥ `10^33` → canonical
    ///   NaN with sign and signaling preserved; payload preserved iff
    ///   `< 10^33`, otherwise zeroed.
    ///
    /// Already-canonical inputs are returned unchanged. See
    /// [`Decimal128::is_canonical`] for the predicate.
    ///
    /// # Examples
    ///
    /// ```
    /// use ferrodec::Decimal128;
    ///
    /// // Already canonical: bit-identical round-trip.
    /// assert_eq!(
    ///     Decimal128::ONE.canonicalize().to_bits(),
    ///     Decimal128::ONE.to_bits(),
    /// );
    ///
    /// // Junk bits on Infinity are stripped.
    /// let dirty = Decimal128::from_bits(Decimal128::INFINITY.to_bits() | 0xFF);
    /// assert_eq!(dirty.canonicalize().to_bits(), Decimal128::INFINITY.to_bits());
    /// ```
    #[inline]
    #[must_use]
    pub const fn canonicalize(self) -> Self {
        match classify_bits(self.0) {
            Class::Finite {
                sign,
                biased_exp,
                coefficient,
            } => {
                debug_assert!(coefficient < COEFFICIENT_LIMIT);
                Self::from_bits(pack_finite(sign, biased_exp, coefficient))
            }
            Class::Zero { sign, biased_exp } => Self::from_bits(pack_finite(sign, biased_exp, 0)),
            Class::Infinity { sign } => Self::from_bits(pack_infinity(sign)),
            Class::QuietNaN { sign, payload } => {
                let p = if payload < MAX_CANONICAL_NAN_PAYLOAD {
                    payload
                } else {
                    0
                };
                Self::from_bits(pack_quiet_nan(sign, p))
            }
            Class::SignalingNaN { sign, payload } => {
                let p = if payload < MAX_CANONICAL_NAN_PAYLOAD {
                    payload
                } else {
                    0
                };
                Self::from_bits(pack_signaling_nan(sign, p))
            }
        }
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

    /// Return `true` iff `self` represents a mathematical integer.
    ///
    /// `±0` and any finite value with non-negative quantum exponent
    /// (`biased_exp ≥ BIAS`) is an integer trivially. For finite values
    /// with negative quantum (e.g. `1.5` stored as `coef = 15` at
    /// quantum `−1`), the coefficient must be an exact integer multiple
    /// of `10^|quantum|`.
    ///
    /// `±∞` and NaN return `false`.
    ///
    /// # Examples
    ///
    /// ```
    /// use ferrodec::Decimal128;
    ///
    /// assert!(Decimal128::ONE.is_integer());
    /// assert!(Decimal128::TEN.is_integer());
    /// assert!(Decimal128::try_new(20, -1).unwrap().is_integer()); // 2.0
    /// assert!(!Decimal128::try_new(15, -1).unwrap().is_integer()); // 1.5
    /// assert!(!Decimal128::INFINITY.is_integer());
    /// assert!(!Decimal128::NAN.is_integer());
    /// ```
    #[inline]
    #[must_use]
    pub const fn is_integer(self) -> bool {
        match classify_bits(self.0) {
            Class::Zero { .. } => true,
            Class::Finite {
                biased_exp,
                coefficient,
                ..
            } => {
                if biased_exp >= BIAS {
                    return true;
                }
                let drop = BIAS - biased_exp;
                if drop > 38 {
                    // 10^39 already exceeds u128. A 34-digit coefficient
                    // can't be a multiple of 10^39, so not integer.
                    return false;
                }
                let divisor = 10u128.pow(drop);
                coefficient % divisor == 0
            }
            _ => false,
        }
    }

    /// Return the unit in the last place at `self`'s stored quantum:
    /// `10^(biased_exp − BIAS)`.
    ///
    /// For finite `self` this is the spacing between values that share
    /// `self`'s cohort — useful for tolerance bookkeeping at a known
    /// magnitude. Cohort matters: `Decimal128::ONE.ulp()` returns `1`
    /// (the unit at the `1E+0` cohort), but `1.0E+0` parsed as `10E−1`
    /// would return `10⁻¹` instead.
    ///
    /// Edge cases:
    /// * `±0` — returns the smallest positive subnormal magnitude at
    ///   the stored quantum (`1 × 10^(biased_exp − BIAS)`).
    /// * `±∞` and NaN — returns `self` (no defined ULP at the
    ///   non-finite boundary).
    ///
    /// # Examples
    ///
    /// ```
    /// use ferrodec::Decimal128;
    ///
    /// // ULP at the ONE cohort is 1 — neighbours stay at the same
    /// // quantum (next_up moves to a finer cohort, but ulp doesn't).
    /// assert_eq!(Decimal128::ONE.ulp().to_bits(), Decimal128::ONE.to_bits());
    ///
    /// // ULP at 1.5 (= 15 × 10⁻¹) is 10⁻¹ = 0.1.
    /// let x = Decimal128::try_new(15, -1).unwrap();
    /// let want = Decimal128::try_new(1, -1).unwrap();
    /// assert_eq!(x.ulp().to_bits(), want.to_bits());
    /// ```
    #[inline]
    #[must_use]
    pub const fn ulp(self) -> Self {
        match classify_bits(self.0) {
            Class::Zero { biased_exp, .. } | Class::Finite { biased_exp, .. } => {
                Self(pack_finite(false, biased_exp, 1))
            }
            _ => self,
        }
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
    fn ieee_class_covers_all_ten_variants() {
        // Walk every IEEE 754-2019 §5.7.2 class against a
        // representative input. The set of inputs is exhaustive on
        // the class enum: every variant has at least one input that
        // produces it, and every input maps to exactly one variant.
        assert_eq!(
            Decimal128::SIGNALING_NAN.ieee_class(),
            IeeeClass::SignalingNaN,
        );
        assert_eq!(Decimal128::NAN.ieee_class(), IeeeClass::QuietNaN);
        // qNaN with sign set still lands in QuietNaN (NaN class
        // doesn't carry sign per IEEE).
        assert_eq!(Decimal128::NAN.neg().ieee_class(), IeeeClass::QuietNaN,);
        assert_eq!(
            Decimal128::NEG_INFINITY.ieee_class(),
            IeeeClass::NegativeInfinity,
        );
        assert_eq!(
            Decimal128::INFINITY.ieee_class(),
            IeeeClass::PositiveInfinity,
        );
        assert_eq!(Decimal128::NEG_ZERO.ieee_class(), IeeeClass::NegativeZero);
        assert_eq!(Decimal128::ZERO.ieee_class(), IeeeClass::PositiveZero);
        assert_eq!(
            Decimal128::MIN_POSITIVE.ieee_class(),
            IeeeClass::PositiveSubnormal,
        );
        assert_eq!(
            Decimal128::MIN_POSITIVE.neg().ieee_class(),
            IeeeClass::NegativeSubnormal,
        );
        assert_eq!(Decimal128::ONE.ieee_class(), IeeeClass::PositiveNormal);
        assert_eq!(Decimal128::NEG_ONE.ieee_class(), IeeeClass::NegativeNormal,);
        assert_eq!(Decimal128::MAX.ieee_class(), IeeeClass::PositiveNormal);
        assert_eq!(Decimal128::MIN.ieee_class(), IeeeClass::NegativeNormal);
        assert_eq!(
            Decimal128::MIN_POSITIVE_NORMAL.ieee_class(),
            IeeeClass::PositiveNormal,
        );
    }

    #[test]
    fn ieee_class_is_quiet_on_signaling_nan() {
        // IEEE 754-2019 §5.7.2 specifies the class operation as
        // *quiet*: an sNaN input must NOT raise a status flag.
        // ferrodec's `ieee_class` returns plain `IeeeClass`, so
        // there is nowhere for INVALID to land — verify by
        // constructing an sNaN with a distinctive payload and
        // confirming the call has no side-effect (no panic, plain
        // SignalingNaN result).
        let snan = Decimal128::from_bits(crate::bid::pack_signaling_nan(true, 0xCAFE));
        assert_eq!(snan.ieee_class(), IeeeClass::SignalingNaN);
        // Idempotent: calling again gives the same answer.
        assert_eq!(snan.ieee_class(), IeeeClass::SignalingNaN);
    }

    #[test]
    fn ieee_class_agrees_with_fpcategory_on_coarse_classes() {
        // The ten IeeeClass variants collapse to the five
        // FpCategory variants under a documented mapping. Pin the
        // mapping for every IeeeClass so a future split (e.g.
        // someone adding a sign to NaN classes by mistake) fails
        // loud.
        for (d, ieee, fp) in [
            (
                Decimal128::SIGNALING_NAN,
                IeeeClass::SignalingNaN,
                FpCategory::Nan,
            ),
            (Decimal128::NAN, IeeeClass::QuietNaN, FpCategory::Nan),
            (
                Decimal128::INFINITY,
                IeeeClass::PositiveInfinity,
                FpCategory::Infinite,
            ),
            (
                Decimal128::NEG_INFINITY,
                IeeeClass::NegativeInfinity,
                FpCategory::Infinite,
            ),
            (Decimal128::ZERO, IeeeClass::PositiveZero, FpCategory::Zero),
            (
                Decimal128::NEG_ZERO,
                IeeeClass::NegativeZero,
                FpCategory::Zero,
            ),
            (
                Decimal128::ONE,
                IeeeClass::PositiveNormal,
                FpCategory::Normal,
            ),
            (
                Decimal128::NEG_ONE,
                IeeeClass::NegativeNormal,
                FpCategory::Normal,
            ),
            (
                Decimal128::MIN_POSITIVE,
                IeeeClass::PositiveSubnormal,
                FpCategory::Subnormal,
            ),
        ] {
            assert_eq!(d.ieee_class(), ieee);
            assert_eq!(d.classify(), fp);
        }
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
        assert_eq!(
            Decimal128::ONE.signum().to_bits(),
            Decimal128::ONE.to_bits()
        );
        assert_eq!(
            Decimal128::NEG_ONE.signum().to_bits(),
            Decimal128::NEG_ONE.to_bits()
        );
        assert_eq!(
            Decimal128::TEN.signum().to_bits(),
            Decimal128::ONE.to_bits()
        );
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
        assert_eq!(
            Decimal128::NEG_ONE.abs().to_bits(),
            Decimal128::ONE.to_bits()
        );
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
        assert_eq!(
            Decimal128::ONE.neg().to_bits(),
            Decimal128::NEG_ONE.to_bits()
        );
        assert_eq!(
            Decimal128::NEG_ONE.neg().to_bits(),
            Decimal128::ONE.to_bits()
        );
        assert_eq!(
            Decimal128::INFINITY.neg().to_bits(),
            Decimal128::NEG_INFINITY.to_bits()
        );
        assert_eq!(
            Decimal128::ZERO.neg().to_bits(),
            Decimal128::NEG_ZERO.to_bits()
        );
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

    // ---- is_canonical / canonicalize ----------------------------------

    #[test]
    fn distinguished_constants_are_canonical() {
        for c in [
            Decimal128::ZERO,
            Decimal128::NEG_ZERO,
            Decimal128::ONE,
            Decimal128::NEG_ONE,
            Decimal128::TEN,
            Decimal128::MAX,
            Decimal128::MIN,
            Decimal128::MIN_POSITIVE,
            Decimal128::MIN_POSITIVE_NORMAL,
            Decimal128::INFINITY,
            Decimal128::NEG_INFINITY,
            Decimal128::NAN,
            Decimal128::SIGNALING_NAN,
        ] {
            assert!(c.is_canonical(), "expected canonical: {c:?}");
            assert_eq!(c.canonicalize().to_bits(), c.to_bits());
        }
    }

    #[test]
    fn form_b_is_non_canonical() {
        // Hand-built Form B: T[4:3] = 11, T[2:1] != 11. Use T = 11000.
        // Decoder reports Zero with biased_exp = 0x123.
        let bits = (0b1_1000u128 << 122) | (0x123u128 << 110) | 0xABCD_u128;
        let d = Decimal128::from_bits(bits);
        assert!(!d.is_canonical(), "Form B must be non-canonical");
        // canonicalize → Form A zero at same biased_exp.
        let c = d.canonicalize();
        assert!(c.is_canonical());
        assert!(c.is_zero());
        // Same biased_exp → same_quantum is true.
        let target_zero = Decimal128::from_bits(crate::bid::pack_finite(false, 0x123, 0));
        assert!(c.same_quantum(target_zero));
    }

    #[test]
    fn form_a_oversized_coefficient_is_non_canonical() {
        // Hand-build Form A with coefficient = 2^113 - 1 (above 10^34 - 1).
        let coef = crate::bid::COEFFICIENT_FIELD_LIMIT - 1;
        let bits = crate::bid::pack_finite(false, BIAS, coef);
        let d = Decimal128::from_bits(bits);
        assert!(
            !d.is_canonical(),
            "Form A with coefficient >= 10^34 is not canonical"
        );
        // canonicalize → ±0 at same biased_exp.
        let c = d.canonicalize();
        assert!(c.is_canonical());
        assert!(c.is_zero());
        let zero_at_bias = Decimal128::from_bits(crate::bid::pack_finite(false, BIAS, 0));
        assert_eq!(c.to_bits(), zero_at_bias.to_bits());
    }

    #[test]
    fn infinity_with_junk_bits_is_non_canonical() {
        // Set some bits below the type field on an Inf encoding.
        let dirty = Decimal128::INFINITY.to_bits() | 0x0000_0000_0000_0000_0000_0000_0000_00FF;
        let d = Decimal128::from_bits(dirty);
        assert!(d.is_infinite());
        assert!(
            !d.is_canonical(),
            "Inf with junk trailing bits is not canonical"
        );
        let c = d.canonicalize();
        assert!(c.is_canonical());
        assert_eq!(c.to_bits(), Decimal128::INFINITY.to_bits());
    }

    #[test]
    fn nan_with_unused_ec_bits_is_non_canonical() {
        // Set a bit in the 120..110 "unused" range on a NaN encoding.
        let dirty = Decimal128::NAN.to_bits() | (1u128 << 115);
        let d = Decimal128::from_bits(dirty);
        assert!(d.is_nan());
        assert!(
            !d.is_canonical(),
            "NaN with bits 120..110 set is not canonical"
        );
        let c = d.canonicalize();
        assert!(c.is_canonical());
        assert!(c.is_nan());
        // Signaling preserved (input was qNaN).
        assert!(c.is_quiet_nan());
    }

    #[test]
    fn nan_with_oversized_payload_is_non_canonical() {
        // Payload = 10^33 — equals MAX_CANONICAL_NAN_PAYLOAD, so non-canonical.
        let d = Decimal128::from_bits(crate::bid::pack_quiet_nan(false, 10u128.pow(33)));
        assert!(!d.is_canonical());
        let c = d.canonicalize();
        assert!(c.is_canonical());
        // Payload zeroed; signaling preserved.
        assert!(c.is_quiet_nan());
        assert_eq!(c.to_bits() & crate::bid::T_MASK, 0);
    }

    #[test]
    fn signaling_nan_canonicalize_preserves_signaling() {
        // sNaN with payload = 10^33 → boundary, non-canonical (the spec
        // requires payload *strictly less than* 10^33). The pack helper
        // masks by T_MASK = 2^110 - 1, and 10^33 < 2^110, so the value
        // survives the mask intact.
        let d = Decimal128::from_bits(crate::bid::pack_signaling_nan(true, 10u128.pow(33)));
        assert!(!d.is_canonical());
        let c = d.canonicalize();
        assert!(c.is_canonical());
        assert!(c.is_signaling_nan(), "signaling bit must be preserved");
        assert!(c.is_sign_negative());
    }

    #[test]
    fn canonicalize_is_idempotent() {
        // canonicalize(canonicalize(x)) == canonicalize(x) for arbitrary inputs.
        let inputs = [
            Decimal128::ONE.to_bits(),
            Decimal128::INFINITY.to_bits() | 0x42, // dirty Inf
            Decimal128::NAN.to_bits() | (1u128 << 115), // dirty qNaN
            crate::bid::pack_finite(false, BIAS, 10u128.pow(34) + 7), // oversized coef
            (0b1_1000u128 << 122) | 0xDEAD,        // Form B
        ];
        for &b in &inputs {
            let once = Decimal128::from_bits(b).canonicalize();
            let twice = once.canonicalize();
            assert_eq!(once.to_bits(), twice.to_bits());
            assert!(once.is_canonical());
        }
    }

    #[test]
    fn is_integer_basics() {
        assert!(Decimal128::ZERO.is_integer());
        assert!(Decimal128::NEG_ZERO.is_integer());
        assert!(Decimal128::ONE.is_integer());
        assert!(Decimal128::NEG_ONE.is_integer());
        assert!(Decimal128::TEN.is_integer());
        assert!(Decimal128::MAX.is_integer()); // MAX is an integer at quantum +6111
        assert!(!Decimal128::INFINITY.is_integer());
        assert!(!Decimal128::NEG_INFINITY.is_integer());
        assert!(!Decimal128::NAN.is_integer());
        assert!(!Decimal128::SIGNALING_NAN.is_integer());

        // 2.0 (= 20 × 10⁻¹) is an integer; 1.5 (= 15 × 10⁻¹) is not.
        let two_point_zero = Decimal128::try_new(20, -1).unwrap();
        assert!(two_point_zero.is_integer());
        let one_and_a_half = Decimal128::try_new(15, -1).unwrap();
        assert!(!one_and_a_half.is_integer());

        // 1000 represented at quantum +3 (= 1 × 10³) is an integer.
        let thousand = Decimal128::try_new(1, 3).unwrap();
        assert!(thousand.is_integer());
    }

    #[test]
    fn ulp_basics() {
        // ULP at ONE's cohort is 1.
        assert_eq!(Decimal128::ONE.ulp().to_bits(), Decimal128::ONE.to_bits());
        // ULP at 1.5 (quantum -1) is 10⁻¹ = 0.1.
        let one_and_a_half = Decimal128::try_new(15, -1).unwrap();
        let tenth = Decimal128::try_new(1, -1).unwrap();
        assert_eq!(one_and_a_half.ulp().to_bits(), tenth.to_bits());
        // ULP at MIN_POSITIVE is itself (the smallest representable
        // increment).
        assert_eq!(
            Decimal128::MIN_POSITIVE.ulp().to_bits(),
            Decimal128::MIN_POSITIVE.to_bits()
        );
        // NaN / Inf pass through.
        assert_eq!(
            Decimal128::INFINITY.ulp().to_bits(),
            Decimal128::INFINITY.to_bits()
        );
        assert!(Decimal128::NAN.ulp().is_nan());
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
