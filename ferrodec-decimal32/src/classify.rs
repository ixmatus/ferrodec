//! Classification predicates: `is_nan`, `is_infinite`, `ieee_class`,
//! `abs`, `neg`, `is_canonical`, `canonicalize`.
//!
//! These are zero-`Status`, branchless-on-bit-pattern operations. Every
//! method here is `const fn` (with the small exception of the
//! `*_with_status` variants that compose `Status` values, which the
//! compiler still fully evaluates).

use core::num::FpCategory;

use crate::bid::{
    classify_bits, decimal_digit_count, pack_finite, pack_infinity, pack_quiet_nan,
    pack_signaling_nan, sign_of, type_field, BiasedExp, Class, Coefficient, COEFFICIENT_LIMIT,
    FORM_B_MARKER, NAN_SIGNALING_SHIFT, PRECISION, SIGN_SHIFT, TYPE_INFINITY, TYPE_NAN, T_BITS,
    T_MASK,
};
use crate::decimal::Decimal32;
use ferrodec_ieee::IeeeClass;
use ferrodec_ieee::Status;

/// Maximum canonical NaN payload: `10^6` (i.e. payload representable as
/// a 6-decimal-digit integer). For BID-32 the payload field is 20 bits
/// (range 0..2²⁰ = `0..1_048_575`), but only payloads strictly below
/// `10^6` are canonical per IEEE 754-2019 §3.5.2.
const MAX_CANONICAL_NAN_PAYLOAD: u32 = 1_000_000;

/// Mask for bits 24..20 — the slots between the signaling-NaN marker
/// (bit 25) and the trailing significand (bits 19..0). For canonical
/// NaN these five bits must be zero.
const NAN_HIGH_UNUSED_MASK: u32 = ((1u32 << NAN_SIGNALING_SHIFT) - 1) & !T_MASK;

/// Mask for bits 25..0 — everything below the 5-bit type field. For
/// canonical Infinity these bits must all be zero.
const INF_BELOW_TYPE_MASK: u32 = (1u32 << 26) - 1;

impl Decimal32 {
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
        type_field(self.0) == TYPE_INFINITY
    }

    /// `true` if this value is finite (not NaN and not ±∞).
    #[inline]
    #[must_use]
    pub const fn is_finite(self) -> bool {
        type_field(self.0) < TYPE_INFINITY
    }

    /// `true` if this value is ±0.
    ///
    /// Both Form A with coefficient 0 and Form B with coefficient ≥ 10⁷
    /// (which decodes as zero per IEEE 754-2019 §3.5.2) are recognised.
    #[inline]
    #[must_use]
    pub const fn is_zero(self) -> bool {
        matches!(classify_bits(self.0), Class::Zero { .. })
    }

    /// `true` if this value is finite, non-zero, and at or above
    /// [`Decimal32::MIN_POSITIVE_NORMAL`] in magnitude.
    ///
    /// Subnormals — finite, non-zero values smaller than
    /// `MIN_POSITIVE_NORMAL` — return `false`. So do `±0`, `±∞`, and
    /// NaN.
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

    /// `true` if the sign bit is set. Signed zeros and signed NaN
    /// included.
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
    /// Maps to [`core::num::FpCategory`] for parity with `f32` / `f64`.
    /// For the full IEEE 754-2019 §5.7.2 ten-class enumeration that
    /// distinguishes sign and signaling-NaN versus quiet-NaN, see
    /// [`Decimal32::ieee_class`].
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
    /// distinguishes (see [`IeeeClass`] for the variant list). Quiet
    /// by IEEE definition — does *not* raise [`Status::INVALID`] on a
    /// signaling-NaN input.
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

    /// Absolute value. NaN passes through (with sign cleared on the bit
    /// pattern). **No status flags raised** — matches `f32::abs`. For
    /// the IEEE 754 §5.5.1-compliant variant that raises
    /// [`Status::INVALID`] on signaling NaN, use
    /// [`Decimal32::abs_with_status`].
    #[inline]
    #[must_use]
    pub const fn abs(self) -> Self {
        Self::from_bits(self.0 & !(1u32 << SIGN_SHIFT))
    }

    /// IEEE 754 §5.5.1-compliant absolute value: raises
    /// [`Status::INVALID`] for signaling-NaN inputs and quietens the
    /// result. Otherwise equivalent to [`Decimal32::abs`].
    #[inline]
    #[must_use]
    pub fn abs_with_status(self) -> (Self, Status) {
        if self.is_signaling_nan() {
            return (Self::NAN, Status::INVALID);
        }
        (self.abs(), Status::OK)
    }

    /// Negate. Flips the sign bit, even on NaN. **No status flags
    /// raised.** For the IEEE 754 §5.5.1-compliant variant, see
    /// [`Decimal32::neg_with_status`].
    #[inline]
    #[must_use]
    pub const fn neg(self) -> Self {
        Self::from_bits(self.0 ^ (1u32 << SIGN_SHIFT))
    }

    /// IEEE 754 §5.5.1-compliant negation: raises [`Status::INVALID`]
    /// for signaling-NaN inputs and quietens the result.
    ///
    /// Per the General Decimal Arithmetic Specification, `minus(x)` is
    /// defined as `subtract(0, x)` under the active rounding context,
    /// which yields `+0` for zero operands under round-to-nearest-even
    /// (the default). We preserve that here: zeros return `+0` with
    /// the same cohort as `self`.
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

    /// Copy the sign of `sign` onto `self`. NaN payload preserved.
    #[inline]
    #[must_use]
    pub const fn copysign(self, sign: Self) -> Self {
        let s = sign.0 & (1u32 << SIGN_SHIFT);
        Self::from_bits((self.0 & !(1u32 << SIGN_SHIFT)) | s)
    }

    /// IEEE 754-2019 §5.7.2 `isCanonical(x)`.
    ///
    /// For BID-32 a bit pattern is canonical iff:
    ///
    /// * **Finite Form A** (T[4..3] ∈ {00, 01, 10}): always canonical
    ///   (the implicit zero prefix bounds the coefficient strictly
    ///   below 2²³ < 10⁷).
    /// * **Finite Form B** (T[4..3] = 11, T[2..1] ∈ {00, 01, 10}):
    ///   canonical iff the decoded coefficient `< 10⁷`. Coefficients in
    ///   `[10⁷, 2²³ + 2 · 2²⁰)` decode to ±0 per IEEE 754-2019
    ///   §3.5.2.
    /// * **Infinity**: bits 25..0 are all zero. The trailing
    ///   significand and exponent continuation are unused in `±∞`
    ///   encodings.
    /// * **NaN**: bits 24..20 are zero *and* the payload `< 10⁶`.
    ///   Bit 25 is the signaling marker and is part of the canonical
    ///   encoding; the five bits between it and the payload must be
    ///   zero.
    ///
    /// No status flags raised. See [`Decimal32::canonicalize`] for the
    /// rewrite operation.
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
        // Finite branch. Top two bits of the type field separate Form A
        // (00 / 01 / 10) from Form B (11, with sub-types non-Inf-non-NaN).
        if (t >> 3) == FORM_B_MARKER {
            // Form B: canonical iff the decoded coefficient is < 10⁷.
            let coef_high4 = 0b1000 | (t & 0b1);
            let coef = (coef_high4 << T_BITS) | (bits & T_MASK);
            return coef < COEFFICIENT_LIMIT;
        }
        // Form A: always canonical.
        true
    }

    /// IEEE 754-2019 §5.4.2 `canonicalize(x)`.
    ///
    /// Returns the canonical encoding of `self`. Quiet — never raises
    /// any status flag.
    ///
    /// Rewrites:
    /// * Non-canonical finite (Form B with coefficient ≥ 10⁷) → ±0
    ///   at the same quantum exponent (matches IEEE 754-2019 §3.5.2
    ///   decoding behaviour).
    /// * Infinity with junk bits set → canonical ±∞.
    /// * NaN with bits 24..20 set or payload ≥ 10⁶ → canonical NaN
    ///   with sign and signaling preserved; payload preserved iff
    ///   `< 10⁶`, otherwise zeroed.
    ///
    /// Already-canonical inputs are returned unchanged.
    #[inline]
    #[must_use]
    pub const fn canonicalize(self) -> Self {
        match classify_bits(self.0) {
            Class::Finite {
                sign,
                biased_exp,
                coefficient,
            } => {
                // `classify_bits` guarantees `biased_exp <= BIASED_EXP_MAX`
                // by the 8-bit field decode and `coefficient <
                // COEFFICIENT_LIMIT` per IEEE 754-2019 §3.5.2 (Form A and
                // canonical Form B encodings only — non-canonical Form B
                // already maps to `Class::Zero` upstream).
                let biased_exp =
                    BiasedExp::try_from_biased(biased_exp).expect("biased_exp from classify_bits");
                let coefficient =
                    Coefficient::try_new(coefficient).expect("coefficient from classify_bits");
                Self::from_bits(pack_finite(sign, biased_exp, coefficient))
            }
            Class::Zero { sign, biased_exp } => {
                let biased_exp =
                    BiasedExp::try_from_biased(biased_exp).expect("biased_exp from classify_bits");
                Self::from_bits(pack_finite(sign, biased_exp, Coefficient::ZERO))
            }
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

// Suppress dead_code on bid items that are now consumed by classify.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nan_predicates() {
        assert!(Decimal32::NAN.is_nan());
        assert!(Decimal32::NAN.is_quiet_nan());
        assert!(!Decimal32::NAN.is_signaling_nan());
        assert!(Decimal32::SIGNALING_NAN.is_nan());
        assert!(Decimal32::SIGNALING_NAN.is_signaling_nan());
        assert!(!Decimal32::SIGNALING_NAN.is_quiet_nan());

        assert!(!Decimal32::ONE.is_nan());
        assert!(!Decimal32::INFINITY.is_nan());
        assert!(!Decimal32::ZERO.is_nan());
    }

    #[test]
    fn infinity_predicates() {
        assert!(Decimal32::INFINITY.is_infinite());
        assert!(Decimal32::NEG_INFINITY.is_infinite());
        assert!(!Decimal32::ONE.is_infinite());
        assert!(!Decimal32::NAN.is_infinite());

        assert!(Decimal32::ONE.is_finite());
        assert!(Decimal32::ZERO.is_finite());
        assert!(!Decimal32::INFINITY.is_finite());
        assert!(!Decimal32::NAN.is_finite());
    }

    #[test]
    fn zero_predicates() {
        assert!(Decimal32::ZERO.is_zero());
        assert!(Decimal32::NEG_ZERO.is_zero());
        assert!(!Decimal32::ONE.is_zero());
        assert!(!Decimal32::INFINITY.is_zero());
        assert!(!Decimal32::NAN.is_zero());
    }

    #[test]
    fn normal_subnormal_predicates() {
        assert!(Decimal32::ONE.is_normal());
        assert!(Decimal32::MAX.is_normal());
        assert!(Decimal32::MIN_POSITIVE_NORMAL.is_normal());
        assert!(!Decimal32::ZERO.is_normal());
        assert!(!Decimal32::NAN.is_normal());
        assert!(!Decimal32::INFINITY.is_normal());

        // MIN_POSITIVE = 1 × 10^-101: subnormal because magnitude is
        // strictly below MIN_POSITIVE_NORMAL = 1 × 10^-95.
        assert!(Decimal32::MIN_POSITIVE.is_subnormal());
        assert!(!Decimal32::MIN_POSITIVE_NORMAL.is_subnormal());
        assert!(!Decimal32::ONE.is_subnormal());
    }

    #[test]
    fn sign_predicates() {
        assert!(!Decimal32::ZERO.is_sign_negative());
        assert!(Decimal32::NEG_ZERO.is_sign_negative());
        assert!(!Decimal32::ONE.is_sign_negative());
        assert!(Decimal32::NEG_ONE.is_sign_negative());
        assert!(Decimal32::ONE.is_sign_positive());
    }

    #[test]
    fn classify_fp_category() {
        assert_eq!(Decimal32::ZERO.classify(), FpCategory::Zero);
        assert_eq!(Decimal32::ONE.classify(), FpCategory::Normal);
        assert_eq!(Decimal32::INFINITY.classify(), FpCategory::Infinite);
        assert_eq!(Decimal32::NAN.classify(), FpCategory::Nan);
        assert_eq!(Decimal32::SIGNALING_NAN.classify(), FpCategory::Nan);
        assert_eq!(Decimal32::MIN_POSITIVE.classify(), FpCategory::Subnormal);
    }

    #[test]
    fn ieee_class_full_ten() {
        assert_eq!(
            Decimal32::SIGNALING_NAN.ieee_class(),
            IeeeClass::SignalingNaN
        );
        assert_eq!(Decimal32::NAN.ieee_class(), IeeeClass::QuietNaN);
        assert_eq!(
            Decimal32::INFINITY.ieee_class(),
            IeeeClass::PositiveInfinity
        );
        assert_eq!(
            Decimal32::NEG_INFINITY.ieee_class(),
            IeeeClass::NegativeInfinity
        );
        assert_eq!(Decimal32::ZERO.ieee_class(), IeeeClass::PositiveZero);
        assert_eq!(Decimal32::NEG_ZERO.ieee_class(), IeeeClass::NegativeZero);
        assert_eq!(Decimal32::ONE.ieee_class(), IeeeClass::PositiveNormal);
        assert_eq!(Decimal32::NEG_ONE.ieee_class(), IeeeClass::NegativeNormal);
        assert_eq!(
            Decimal32::MIN_POSITIVE.ieee_class(),
            IeeeClass::PositiveSubnormal
        );
        // NEG of a subnormal: also subnormal.
        let neg_subnormal = Decimal32::MIN_POSITIVE.neg();
        assert_eq!(neg_subnormal.ieee_class(), IeeeClass::NegativeSubnormal);
    }

    #[test]
    fn abs_neg_basic() {
        assert_eq!(Decimal32::NEG_ONE.abs().to_bits(), Decimal32::ONE.to_bits());
        assert_eq!(Decimal32::ONE.abs().to_bits(), Decimal32::ONE.to_bits());
        assert_eq!(Decimal32::ONE.neg().to_bits(), Decimal32::NEG_ONE.to_bits());
        assert_eq!(Decimal32::NEG_ONE.neg().to_bits(), Decimal32::ONE.to_bits());
    }

    #[test]
    fn abs_neg_with_status_signaling_nan() {
        let (r, s) = Decimal32::SIGNALING_NAN.abs_with_status();
        assert!(r.is_quiet_nan());
        assert!(s.invalid());

        let (r, s) = Decimal32::SIGNALING_NAN.neg_with_status();
        assert!(r.is_quiet_nan());
        assert!(s.invalid());
    }

    #[test]
    fn neg_with_status_zero_yields_positive_zero() {
        let (r, s) = Decimal32::NEG_ZERO.neg_with_status();
        assert_eq!(r.to_bits(), Decimal32::ZERO.to_bits());
        assert!(s.is_ok());

        let (r, s) = Decimal32::ZERO.neg_with_status();
        assert_eq!(r.to_bits(), Decimal32::ZERO.to_bits());
        assert!(s.is_ok());
    }

    #[test]
    fn copysign_basic() {
        assert_eq!(
            Decimal32::ONE.copysign(Decimal32::NEG_ONE).to_bits(),
            Decimal32::NEG_ONE.to_bits()
        );
        assert_eq!(
            Decimal32::NEG_ONE.copysign(Decimal32::ONE).to_bits(),
            Decimal32::ONE.to_bits()
        );
    }

    #[test]
    fn is_canonical_distinguished_constants() {
        assert!(Decimal32::ZERO.is_canonical());
        assert!(Decimal32::NEG_ZERO.is_canonical());
        assert!(Decimal32::ONE.is_canonical());
        assert!(Decimal32::MAX.is_canonical());
        assert!(Decimal32::MIN.is_canonical());
        assert!(Decimal32::MIN_POSITIVE.is_canonical());
        assert!(Decimal32::INFINITY.is_canonical());
        assert!(Decimal32::NEG_INFINITY.is_canonical());
        assert!(Decimal32::NAN.is_canonical());
        assert!(Decimal32::SIGNALING_NAN.is_canonical());
    }

    #[test]
    fn is_canonical_dirty_inf() {
        let dirty = Decimal32::from_bits(Decimal32::INFINITY.to_bits() | 0xFF);
        assert!(!dirty.is_canonical());
    }

    #[test]
    fn is_canonical_dirty_nan_high_bits() {
        // Set a bit in the unused 24..20 range.
        let dirty = Decimal32::from_bits(Decimal32::NAN.to_bits() | (1u32 << 22));
        assert!(!dirty.is_canonical());
    }

    #[test]
    fn is_canonical_nan_payload_at_limit() {
        // Payload exactly at 10^6 (the limit, exclusive) is non-canonical.
        let dirty = Decimal32::from_bits(Decimal32::NAN.to_bits() | MAX_CANONICAL_NAN_PAYLOAD);
        assert!(!dirty.is_canonical());
        // Payload one below the limit is canonical.
        let ok = Decimal32::from_bits(Decimal32::NAN.to_bits() | (MAX_CANONICAL_NAN_PAYLOAD - 1));
        assert!(ok.is_canonical());
    }

    #[test]
    fn canonicalize_dirty_inf_and_nan() {
        let dirty_inf = Decimal32::from_bits(Decimal32::INFINITY.to_bits() | 0xFF);
        assert_eq!(
            dirty_inf.canonicalize().to_bits(),
            Decimal32::INFINITY.to_bits()
        );

        let dirty_nan = Decimal32::from_bits(Decimal32::NAN.to_bits() | (1u32 << 22));
        let canonicalised = dirty_nan.canonicalize();
        assert!(canonicalised.is_canonical());
        assert!(canonicalised.is_quiet_nan());
    }
}
