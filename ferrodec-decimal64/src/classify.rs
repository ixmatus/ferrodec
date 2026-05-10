//! Classification predicates: `is_nan`, `is_infinite`, `ieee_class`,
//! `abs`, `neg`, `is_canonical`, `canonicalize`.
//!
//! Mirrors ferrodec-decimal32's classify.rs surface, scaled to
//! Decimal64's 64-bit width. Every method here is `const fn` (with
//! the small exception of `*_with_status` variants composing
//! `Status` values).

use core::num::FpCategory;

use crate::bid::{
    classify_bits, decimal_digit_count, pack_finite, pack_infinity, pack_quiet_nan,
    pack_signaling_nan, sign_of, type_field, Class, COEFFICIENT_LIMIT, FORM_B_MARKER,
    NAN_SIGNALING_SHIFT, PRECISION, SIGN_SHIFT, TYPE_INFINITY, TYPE_NAN, T_BITS, T_MASK,
};
use crate::classify_types::IeeeClass;
use crate::decimal::Decimal64;
use crate::status::Status;

/// Maximum canonical NaN payload: `10¹⁵`. For BID-64 the payload
/// field is 50 bits (range 0..2⁵⁰), but only payloads strictly below
/// `10¹⁵` are canonical per IEEE 754-2019 §3.5.2.
const MAX_CANONICAL_NAN_PAYLOAD: u64 = 1_000_000_000_000_000;

/// Mask for bits 56..50 — the slots between the signaling-NaN marker
/// (bit 57) and the trailing significand (bits 49..0). For canonical
/// NaN these seven bits must be zero.
const NAN_HIGH_UNUSED_MASK: u64 = ((1u64 << NAN_SIGNALING_SHIFT) - 1) & !T_MASK;

/// Mask for bits 57..0 — everything below the 5-bit type field. For
/// canonical Infinity these bits must all be zero.
const INF_BELOW_TYPE_MASK: u64 = (1u64 << 58) - 1;

impl Decimal64 {
    /// `true` if this value is *any* NaN (quiet or signaling).
    #[inline]
    #[must_use]
    pub const fn is_nan(self) -> bool {
        type_field(self.0) == TYPE_NAN
    }

    /// `true` if this value is a signaling NaN.
    #[inline]
    #[must_use]
    pub const fn is_signaling_nan(self) -> bool {
        self.is_nan() && ((self.0 >> NAN_SIGNALING_SHIFT) & 1) == 1
    }

    /// `true` if this value is a quiet NaN.
    #[inline]
    #[must_use]
    pub const fn is_quiet_nan(self) -> bool {
        self.is_nan() && ((self.0 >> NAN_SIGNALING_SHIFT) & 1) == 0
    }

    /// `true` if this value is ±∞.
    #[inline]
    #[must_use]
    pub const fn is_infinite(self) -> bool {
        type_field(self.0) == TYPE_INFINITY
    }

    /// `true` if this value is finite (not NaN and not ±∞).
    #[inline]
    #[must_use]
    pub const fn is_finite(self) -> bool {
        type_field(self.0) < TYPE_INFINITY
    }

    /// `true` if this value is ±0.
    #[inline]
    #[must_use]
    pub const fn is_zero(self) -> bool {
        matches!(classify_bits(self.0), Class::Zero { .. })
    }

    /// `true` if this value is finite, non-zero, and at or above
    /// [`Decimal64::MIN_POSITIVE_NORMAL`] in magnitude.
    #[inline]
    #[must_use]
    pub const fn is_normal(self) -> bool {
        match classify_bits(self.0) {
            Class::Finite {
                biased_exp,
                coefficient,
                ..
            } => {
                let digits = decimal_digit_count(coefficient);
                biased_exp + digits >= PRECISION
            }
            _ => false,
        }
    }

    /// `true` if this value is finite, non-zero, and strictly below
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

    /// `true` if the sign bit is set.
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

    /// Absolute value. Bit-flips the sign bit; no status flags raised.
    #[inline]
    #[must_use]
    pub const fn abs(self) -> Self {
        Self::from_bits(self.0 & !(1u64 << SIGN_SHIFT))
    }

    /// IEEE 754 §5.5.1-compliant absolute value.
    #[inline]
    #[must_use]
    pub fn abs_with_status(self) -> (Self, Status) {
        if self.is_signaling_nan() {
            return (Self::NAN, Status::INVALID);
        }
        (self.abs(), Status::OK)
    }

    /// Negate.
    #[inline]
    #[must_use]
    pub const fn neg(self) -> Self {
        Self::from_bits(self.0 ^ (1u64 << SIGN_SHIFT))
    }

    /// IEEE 754 §5.5.1-compliant negation.
    #[inline]
    #[must_use]
    pub fn neg_with_status(self) -> (Self, Status) {
        if self.is_signaling_nan() {
            return (Self::NAN, Status::INVALID);
        }
        if self.is_zero() {
            return (self.abs(), Status::OK);
        }
        (self.neg(), Status::OK)
    }

    /// Copy the sign of `sign` onto `self`.
    #[inline]
    #[must_use]
    pub const fn copysign(self, sign: Self) -> Self {
        let s = sign.0 & (1u64 << SIGN_SHIFT);
        Self::from_bits((self.0 & !(1u64 << SIGN_SHIFT)) | s)
    }

    /// IEEE 754-2019 §5.7.2 `isCanonical(x)`.
    ///
    /// For BID-64:
    ///
    /// * **Form A** (T[4..3] ∈ {00, 01, 10}): always canonical
    ///   (Form A coefficient < 2⁵³ < 10¹⁶).
    /// * **Form B** (T[4..3] = 11, T[2..1] ∈ {00, 01, 10}):
    ///   canonical iff the decoded coefficient `< 10¹⁶`.
    ///   Coefficients in `[10¹⁶, 3·2⁵³)` decode to ±0.
    /// * **Infinity**: bits 57..0 are all zero.
    /// * **NaN**: bits 56..50 are zero *and* the payload `< 10¹⁵`.
    #[inline]
    #[must_use]
    pub const fn is_canonical(self) -> bool {
        let bits = self.0;
        let t = type_field(bits);
        if t == TYPE_INFINITY {
            return (bits & INF_BELOW_TYPE_MASK) == 0;
        }
        if t == TYPE_NAN {
            if (bits & NAN_HIGH_UNUSED_MASK) != 0 {
                return false;
            }
            return (bits & T_MASK) < MAX_CANONICAL_NAN_PAYLOAD;
        }
        if (t >> 3) == FORM_B_MARKER {
            let coef_high4 = 0b1000 | (t & 0b1);
            let coef = (coef_high4 << T_BITS) | (bits & T_MASK);
            return coef < COEFFICIENT_LIMIT;
        }
        true
    }

    /// IEEE 754-2019 §5.4.2 `canonicalize(x)`.
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
                let canonical_payload = if payload < MAX_CANONICAL_NAN_PAYLOAD {
                    payload
                } else {
                    0
                };
                Self::from_bits(pack_quiet_nan(sign, canonical_payload))
            }
            Class::SignalingNaN { sign, payload } => {
                let canonical_payload = if payload < MAX_CANONICAL_NAN_PAYLOAD {
                    payload
                } else {
                    0
                };
                Self::from_bits(pack_signaling_nan(sign, canonical_payload))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nan_predicates() {
        assert!(Decimal64::NAN.is_nan());
        assert!(Decimal64::NAN.is_quiet_nan());
        assert!(!Decimal64::NAN.is_signaling_nan());
        assert!(Decimal64::SIGNALING_NAN.is_nan());
        assert!(Decimal64::SIGNALING_NAN.is_signaling_nan());
        assert!(!Decimal64::ONE.is_nan());
    }

    #[test]
    fn infinity_predicates() {
        assert!(Decimal64::INFINITY.is_infinite());
        assert!(Decimal64::NEG_INFINITY.is_infinite());
        assert!(!Decimal64::ONE.is_infinite());
        assert!(!Decimal64::NAN.is_infinite());
        assert!(Decimal64::ONE.is_finite());
        assert!(!Decimal64::INFINITY.is_finite());
    }

    #[test]
    fn zero_predicates() {
        assert!(Decimal64::ZERO.is_zero());
        assert!(Decimal64::NEG_ZERO.is_zero());
        assert!(!Decimal64::ONE.is_zero());
    }

    #[test]
    fn normal_subnormal() {
        assert!(Decimal64::ONE.is_normal());
        assert!(Decimal64::MIN_POSITIVE_NORMAL.is_normal());
        assert!(Decimal64::MIN_POSITIVE.is_subnormal());
        assert!(!Decimal64::ZERO.is_normal());
        assert!(!Decimal64::NAN.is_normal());
    }

    #[test]
    fn sign_predicates() {
        assert!(Decimal64::ONE.is_sign_positive());
        assert!(Decimal64::NEG_ONE.is_sign_negative());
        assert!(Decimal64::NEG_ZERO.is_sign_negative());
    }

    #[test]
    fn classify_fp_category() {
        assert_eq!(Decimal64::ZERO.classify(), FpCategory::Zero);
        assert_eq!(Decimal64::ONE.classify(), FpCategory::Normal);
        assert_eq!(Decimal64::INFINITY.classify(), FpCategory::Infinite);
        assert_eq!(Decimal64::NAN.classify(), FpCategory::Nan);
        assert_eq!(Decimal64::MIN_POSITIVE.classify(), FpCategory::Subnormal);
    }

    #[test]
    fn ieee_class_full_ten() {
        assert_eq!(Decimal64::SIGNALING_NAN.ieee_class(), IeeeClass::SignalingNaN);
        assert_eq!(Decimal64::NAN.ieee_class(), IeeeClass::QuietNaN);
        assert_eq!(Decimal64::INFINITY.ieee_class(), IeeeClass::PositiveInfinity);
        assert_eq!(Decimal64::NEG_INFINITY.ieee_class(), IeeeClass::NegativeInfinity);
        assert_eq!(Decimal64::ZERO.ieee_class(), IeeeClass::PositiveZero);
        assert_eq!(Decimal64::NEG_ZERO.ieee_class(), IeeeClass::NegativeZero);
        assert_eq!(Decimal64::ONE.ieee_class(), IeeeClass::PositiveNormal);
        assert_eq!(Decimal64::NEG_ONE.ieee_class(), IeeeClass::NegativeNormal);
        assert_eq!(Decimal64::MIN_POSITIVE.ieee_class(), IeeeClass::PositiveSubnormal);
        let neg_subnormal = Decimal64::MIN_POSITIVE.neg();
        assert_eq!(neg_subnormal.ieee_class(), IeeeClass::NegativeSubnormal);
    }

    #[test]
    fn abs_neg_basic() {
        assert_eq!(Decimal64::NEG_ONE.abs().to_bits(), Decimal64::ONE.to_bits());
        assert_eq!(Decimal64::ONE.neg().to_bits(), Decimal64::NEG_ONE.to_bits());
    }

    #[test]
    fn abs_neg_with_status_signaling_nan() {
        let (r, s) = Decimal64::SIGNALING_NAN.abs_with_status();
        assert!(r.is_quiet_nan());
        assert!(s.invalid());

        let (r, s) = Decimal64::SIGNALING_NAN.neg_with_status();
        assert!(r.is_quiet_nan());
        assert!(s.invalid());
    }

    #[test]
    fn neg_with_status_zero_yields_positive_zero() {
        let (r, _) = Decimal64::NEG_ZERO.neg_with_status();
        assert_eq!(r.to_bits(), Decimal64::ZERO.to_bits());
    }

    #[test]
    fn copysign_basic() {
        assert_eq!(
            Decimal64::ONE.copysign(Decimal64::NEG_ONE).to_bits(),
            Decimal64::NEG_ONE.to_bits()
        );
        assert_eq!(
            Decimal64::NEG_ONE.copysign(Decimal64::ONE).to_bits(),
            Decimal64::ONE.to_bits()
        );
    }

    #[test]
    fn is_canonical_distinguished_constants() {
        assert!(Decimal64::ZERO.is_canonical());
        assert!(Decimal64::ONE.is_canonical());
        assert!(Decimal64::MAX.is_canonical());
        assert!(Decimal64::MIN_POSITIVE.is_canonical());
        assert!(Decimal64::INFINITY.is_canonical());
        assert!(Decimal64::NAN.is_canonical());
        assert!(Decimal64::SIGNALING_NAN.is_canonical());
    }

    #[test]
    fn is_canonical_dirty_inf() {
        let dirty = Decimal64::from_bits(Decimal64::INFINITY.to_bits() | 0xFF);
        assert!(!dirty.is_canonical());
    }

    #[test]
    fn is_canonical_dirty_nan_high_bits() {
        let dirty = Decimal64::from_bits(Decimal64::NAN.to_bits() | (1u64 << 53));
        assert!(!dirty.is_canonical());
    }

    #[test]
    fn is_canonical_nan_payload_at_limit() {
        let dirty = Decimal64::from_bits(Decimal64::NAN.to_bits() | MAX_CANONICAL_NAN_PAYLOAD);
        assert!(!dirty.is_canonical());
        let ok = Decimal64::from_bits(Decimal64::NAN.to_bits() | (MAX_CANONICAL_NAN_PAYLOAD - 1));
        assert!(ok.is_canonical());
    }

    #[test]
    fn canonicalize_dirty_inf_and_nan() {
        let dirty_inf = Decimal64::from_bits(Decimal64::INFINITY.to_bits() | 0xFF);
        assert_eq!(
            dirty_inf.canonicalize().to_bits(),
            Decimal64::INFINITY.to_bits()
        );

        let dirty_nan = Decimal64::from_bits(Decimal64::NAN.to_bits() | (1u64 << 53));
        let canonicalised = dirty_nan.canonicalize();
        assert!(canonicalised.is_canonical());
        assert!(canonicalised.is_quiet_nan());
    }
}
