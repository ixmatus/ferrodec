//! BID (Binary Integer Decimal) encoding for IEEE 754-2019 decimal64.
//!
//! Layout of the 64-bit encoding (`bits[63]` = MSB, `bits[0]` = LSB):
//!
//! ```text
//! bit 63      : sign
//! bits 62..58 : 5-bit "type" field (combination top)
//! bits 57..50 : 8-bit exponent continuation
//! bits 49..0  : 50-bit trailing significand
//! ```
//!
//! Decoding the type field `T = bits[62..58]`:
//!
//! * `T = 11110` → ±Infinity (sign from bit 63)
//! * `T = 11111` → NaN. Bit 57 = 1 ⇒ signaling NaN, 0 ⇒ quiet NaN.
//!   Trailing 50 bits are the payload.
//! * `T[4..3] ∈ {00,01,10}` → Form A:
//!     * `biased_exp = T[4..3] || ec`  (2 + 8 = 10 bits, range 0..767)
//!     * `coefficient = 0 || T[2..0] || trailing_significand`
//!       (3 + 50 = 53 bits, range `0..2^53 − 1`)
//! * `T[4..3] = 11` (and not Inf/NaN) → Form B:
//!     * `biased_exp = T[2..1] || ec`  (2 + 8 = 10 bits, range 0..767)
//!     * `coefficient = 100 || T[0] || trailing_significand`
//!       (3 + 1 + 50 = 54 bits, value range `2^53 .. 2^53 + 2^51`)
//!
//! Like BID-32 and unlike BID-128, BID-64 uses Form B for the upper
//! portion of the canonical coefficient range:
//! `[2^53, 10^16) = [9_007_199_254_740_992, 10_000_000_000_000_000)`.
//! Form B encodings of coefficients ≥ 10¹⁶ are non-canonical and
//! decode to ±0 with the encoded sign and biased exponent (per IEEE
//! 754-2019 §3.5.2).
//!
//! IEEE 754 parameters for decimal64: precision p = 16 digits,
//! emax = 384, emin = −383, bias = 398, biased exponent range 0..767.

// Most items below are unused in the foundations layer but will be
// consumed by the classify, parse, format, and arithmetic modules
// landed in subsequent commits per the plan archived at
// `docs/decisions/plans/2026-05-09-workspace-and-decimal-siblings.md`.
// The blanket allow is removed as those modules begin consuming,
// leaving only the targeted #[allow(dead_code)] attributes on items
// genuinely consumed only by future modules.
#![allow(dead_code)]

// Bit-position constants -----------------------------------------------------

pub(crate) const SIGN_SHIFT: u32 = 63;

/// Top of the 5-bit type field.
pub(crate) const TYPE_SHIFT: u32 = 58;
/// Mask for the 5-bit type field, in place.
#[allow(dead_code)] // consumed by arithmetic modules
pub(crate) const TYPE_MASK: u64 = 0b11111u64 << TYPE_SHIFT;

/// Type-field marker for ±Infinity.
pub(crate) const TYPE_INFINITY: u64 = 0b11110;
/// Type-field marker for ±NaN (signaling bit at `NAN_SIGNALING_SHIFT`).
pub(crate) const TYPE_NAN: u64 = 0b11111;
/// Two-bit Form B prefix occupying T[4..3] of the type field.
pub(crate) const FORM_B_MARKER: u64 = 0b11;

/// Position of the signaling-bit within a NaN encoding.
pub(crate) const NAN_SIGNALING_SHIFT: u32 = 57;

/// Top of the 8-bit exponent continuation.
pub(crate) const EC_SHIFT: u32 = 50;
#[allow(dead_code)]
pub(crate) const EC_BITS: u32 = 8;
#[allow(dead_code)]
pub(crate) const EC_MASK: u64 = ((1u64 << EC_BITS) - 1) << EC_SHIFT;

/// Width of the trailing significand, in bits.
pub(crate) const T_BITS: u32 = 50;
pub(crate) const T_MASK: u64 = (1u64 << T_BITS) - 1;

// IEEE 754 decimal64 parameters --------------------------------------------

/// Decimal digits of precision.
pub(crate) const PRECISION: u32 = 16;
/// Maximum unbiased exponent.
#[allow(dead_code)]
pub(crate) const E_MAX: i32 = 384;
/// Minimum unbiased exponent.
#[allow(dead_code)]
pub(crate) const E_MIN: i32 = 1 - E_MAX;
/// Bias added to the unbiased quantum exponent for storage.
pub(crate) const BIAS: u32 = 398;
/// Largest valid biased exponent.
pub(crate) const BIASED_EXP_MAX: u32 = 767;
/// `10^16` — the strict upper bound on a canonical coefficient.
pub(crate) const COEFFICIENT_LIMIT: u64 = 10u64.pow(16);
/// `2^53` — the boundary between Form A (coefficient < 2⁵³) and Form B
/// (coefficient ≥ 2⁵³). Distinct from the canonical limit because
/// `2^53 < 10^16`: Form B carries canonical coefficients for the range
/// `[2^53, 10^16)`.
#[allow(dead_code)]
pub(crate) const FORM_B_THRESHOLD: u64 = 1u64 << 53;
/// `2^53 + 2^51` (= `10 · 2^50`) — the strict upper bound on any Form B coefficient.
/// Encodings between `COEFFICIENT_LIMIT` and this value are
/// non-canonical and decode to ±0.
#[allow(dead_code)]
pub(crate) const COEFFICIENT_FIELD_LIMIT: u64 = (1u64 << 53) + 2 * (1u64 << T_BITS);

// Type-level invariants on encoded fields -----------------------------------

/// A biased exponent in the canonical range `[0, BIASED_EXP_MAX]` for
/// decimal64. Constructed via the fallible or saturating constructors below;
/// `pack_finite` accepts only this typed value, so the encoded field is
/// guaranteed by the type system to fit in the BID-64 10-bit slot.
///
/// Replaces a pre-1.4.0 `debug_assert!` precondition that admitted release
/// mode garbage bits on input derived arithmetic (see `KNOWN_ISSUES.md` H3
/// and the 2026-05-11 correctness slice plan's "H3 fix shape" subsection).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
pub(crate) struct BiasedExp(u32);

impl BiasedExp {
    /// The minimum biased exponent: `0`, corresponding to quantum `-BIAS = -398`.
    pub(crate) const MIN: Self = Self(0);
    /// The maximum biased exponent: `BIASED_EXP_MAX = 767`, corresponding to
    /// quantum `+369`.
    pub(crate) const MAX: Self = Self(BIASED_EXP_MAX);
    /// The biased exponent for the canonical quantum `0`, which equals `BIAS`.
    pub(crate) const ZERO_QUANTUM: Self = Self(BIAS);

    /// Construct from a raw biased value. Returns `None` when `biased`
    /// exceeds `BIASED_EXP_MAX`.
    #[inline]
    #[must_use]
    pub(crate) const fn try_from_biased(biased: u32) -> Option<Self> {
        if biased <= BIASED_EXP_MAX {
            Some(Self(biased))
        } else {
            None
        }
    }

    /// Construct from an unbiased exponent by adding `BIAS`. Returns `None`
    /// when the unbiased value falls outside the representable range
    /// `[-BIAS, BIASED_EXP_MAX as i32 - BIAS as i32] = [-398, +369]` for
    /// decimal64.
    #[inline]
    #[must_use]
    pub(crate) const fn try_from_unbiased(unbiased: i32) -> Option<Self> {
        let biased = unbiased + BIAS as i32;
        if biased >= 0 && biased <= BIASED_EXP_MAX as i32 {
            Some(Self(biased as u32))
        } else {
            None
        }
    }

    /// Construct from an unbiased exponent, clamping to the representable
    /// range. Returns the typed value plus a flag indicating whether
    /// clamping occurred; callers should raise `Status::CLAMPED` when the
    /// flag is true. Per IEEE 754-2019 §6.3 (preferred exponent clamping
    /// on additive operations).
    #[inline]
    #[must_use]
    pub(crate) const fn clamp_unbiased(unbiased: i32) -> (Self, bool) {
        let biased = unbiased + BIAS as i32;
        if biased < 0 {
            (Self::MIN, true)
        } else if biased > BIASED_EXP_MAX as i32 {
            (Self::MAX, true)
        } else {
            (Self(biased as u32), false)
        }
    }

    /// The underlying biased exponent value.
    #[inline]
    #[must_use]
    pub(crate) const fn get(self) -> u32 {
        self.0
    }
}

/// A finite coefficient in the canonical range `[0, COEFFICIENT_LIMIT)` for
/// decimal64. Like `BiasedExp`, replaces a pre-1.4.0 `debug_assert!`
/// precondition on `pack_finite` so the invariant holds at the type level
/// rather than only in debug builds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
pub(crate) struct Coefficient(u64);

impl Coefficient {
    /// The zero coefficient.
    pub(crate) const ZERO: Self = Self(0);
    /// The unit coefficient `1`.
    pub(crate) const ONE: Self = Self(1);
    /// The maximum canonical coefficient: `COEFFICIENT_LIMIT - 1`.
    pub(crate) const MAX: Self = Self(COEFFICIENT_LIMIT - 1);

    /// Construct from a raw `u64`. Returns `None` when the value reaches or
    /// exceeds `COEFFICIENT_LIMIT = 10^16`.
    #[inline]
    #[must_use]
    pub(crate) const fn try_new(coefficient: u64) -> Option<Self> {
        if coefficient < COEFFICIENT_LIMIT {
            Some(Self(coefficient))
        } else {
            None
        }
    }

    /// The underlying coefficient value.
    #[inline]
    #[must_use]
    pub(crate) const fn get(self) -> u64 {
        self.0
    }
}

// Decoded form of an encoding ------------------------------------------------

/// Result of decoding the bit pattern of a [`Decimal64`].
///
/// The decode never fails: every 64-bit input maps to exactly one variant.
///
/// [`Decimal64`]: crate::Decimal64
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Class {
    /// Finite, non-zero value with `coefficient ∈ [1, 10^16 − 1]`.
    Finite {
        sign: bool,
        biased_exp: u32,
        coefficient: u64,
    },
    /// Numerical zero. `biased_exp` is preserved so `total_cmp` can
    /// distinguish cohorts.
    Zero { sign: bool, biased_exp: u32 },
    /// ±Infinity.
    Infinity { sign: bool },
    /// Quiet NaN with the given trailing-significand payload.
    QuietNaN { sign: bool, payload: u64 },
    /// Signaling NaN with the given trailing-significand payload.
    SignalingNaN { sign: bool, payload: u64 },
}

// Decoding -------------------------------------------------------------------

/// Read the sign bit.
#[inline]
pub(crate) const fn sign_of(bits: u64) -> bool {
    (bits >> SIGN_SHIFT) & 1 == 1
}

/// Read the 5-bit type field.
#[inline]
pub(crate) const fn type_field(bits: u64) -> u64 {
    (bits >> TYPE_SHIFT) & 0b11111
}

/// Decompose `bits` into its [`Class`].
#[inline]
pub(crate) const fn classify_bits(bits: u64) -> Class {
    let sign = sign_of(bits);
    let t = type_field(bits);

    if t == TYPE_INFINITY {
        return Class::Infinity { sign };
    }
    if t == TYPE_NAN {
        let signaling = ((bits >> NAN_SIGNALING_SHIFT) & 1) == 1;
        let payload = bits & T_MASK;
        return if signaling {
            Class::SignalingNaN { sign, payload }
        } else {
            Class::QuietNaN { sign, payload }
        };
    }

    let ec = (bits >> EC_SHIFT) & ((1u64 << EC_BITS) - 1);
    let top2 = t >> 3;

    if top2 == FORM_B_MARKER {
        // Form B
        let exp_high2 = (t >> 1) & 0b11;
        let biased_exp = ((exp_high2 << EC_BITS) | ec) as u32;
        // Significand prefix is the literal "100" (3 bits) || T[0]; the
        // implicit leading 1<<53 is added explicitly here.
        let coefficient = (1u64 << 53) | ((t & 0b1) << T_BITS) | (bits & T_MASK);

        // Per IEEE 754-2019 §3.5.2, a Form B coefficient that exceeds
        // 10^p − 1 is non-canonical: the value is zero with the encoded
        // sign and biased exponent.
        if coefficient >= COEFFICIENT_LIMIT {
            return Class::Zero { sign, biased_exp };
        }
        return Class::Finite {
            sign,
            biased_exp,
            coefficient,
        };
    }

    // Form A
    let exp_high2 = top2;
    let coef_high3 = t & 0b111;
    let biased_exp = ((exp_high2 << EC_BITS) | ec) as u32;
    let coefficient = (coef_high3 << T_BITS) | (bits & T_MASK);

    // Form A coefficients always fit in [0, 2^53) < 10^16, so no
    // canonical check needed beyond the zero distinction.
    if coefficient == 0 {
        Class::Zero { sign, biased_exp }
    } else {
        Class::Finite {
            sign,
            biased_exp,
            coefficient,
        }
    }
}

// Encoding -------------------------------------------------------------------

/// Pack a finite (or zero) value, choosing Form A or Form B based on the
/// coefficient magnitude.
///
/// The `BiasedExp` and `Coefficient` types statically guarantee the
/// invariants `biased_exp.get() <= BIASED_EXP_MAX` and
/// `coefficient.get() < COEFFICIENT_LIMIT`; no runtime check is needed.
#[inline]
pub(crate) const fn pack_finite(
    sign: bool,
    biased_exp: BiasedExp,
    coefficient: Coefficient,
) -> u64 {
    let s = (sign as u64) << SIGN_SHIFT;
    let bexp = biased_exp.get();
    let exp_high2 = ((bexp >> EC_BITS) & 0b11) as u64;
    let ec = (bexp & ((1 << EC_BITS) - 1)) as u64;
    let coef = coefficient.get();
    let t = coef & T_MASK;

    if coef < FORM_B_THRESHOLD {
        // Form A.
        let coef_high3 = (coef >> T_BITS) & 0b111;
        let type_bits = (exp_high2 << 3) | coef_high3;
        s | (type_bits << TYPE_SHIFT) | (ec << EC_SHIFT) | t
    } else {
        // Form B.
        let d = (coef >> T_BITS) & 0b1;
        let type_bits = (FORM_B_MARKER << 3) | (exp_high2 << 1) | d;
        s | (type_bits << TYPE_SHIFT) | (ec << EC_SHIFT) | t
    }
}

#[inline]
pub(crate) const fn pack_infinity(sign: bool) -> u64 {
    let s = (sign as u64) << SIGN_SHIFT;
    s | (TYPE_INFINITY << TYPE_SHIFT)
}

#[inline]
pub(crate) const fn pack_quiet_nan(sign: bool, payload: u64) -> u64 {
    let s = (sign as u64) << SIGN_SHIFT;
    s | (TYPE_NAN << TYPE_SHIFT) | (payload & T_MASK)
}

#[inline]
pub(crate) const fn pack_signaling_nan(sign: bool, payload: u64) -> u64 {
    let s = (sign as u64) << SIGN_SHIFT;
    s | (TYPE_NAN << TYPE_SHIFT) | (1u64 << NAN_SIGNALING_SHIFT) | (payload & T_MASK)
}

// Helpers --------------------------------------------------------------------

/// Number of significant decimal digits in `n`. Returns `1` when `n == 0`.
#[inline]
#[allow(dead_code)] // consumed by arithmetic modules
pub(crate) const fn decimal_digit_count(n: u64) -> u32 {
    if n == 0 {
        1
    } else {
        n.ilog10() + 1
    }
}

/// `10^k` for `k <= 19` (the largest power of ten that fits in `u64`).
#[inline]
#[allow(dead_code)] // consumed by arithmetic modules
pub(crate) const fn pow10(k: u32) -> u64 {
    debug_assert!(k <= 19);
    10u64.pow(k)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn precision_consistent() {
        assert_eq!(PRECISION, 16);
        assert_eq!(COEFFICIENT_LIMIT, 10_000_000_000_000_000);
        assert!(COEFFICIENT_LIMIT < COEFFICIENT_FIELD_LIMIT);
        assert!(FORM_B_THRESHOLD < COEFFICIENT_LIMIT);
        assert_eq!(FORM_B_THRESHOLD, 1u64 << 53);
    }

    #[test]
    fn biased_exp_max_consistent() {
        assert_eq!(BIASED_EXP_MAX, (2 * E_MAX - 1) as u32);
        assert_eq!(BIAS, E_MAX as u32 + PRECISION - 2);
    }

    #[test]
    fn pack_unpack_roundtrip_zero() {
        let bits = pack_finite(false, BiasedExp::ZERO_QUANTUM, Coefficient::ZERO);
        match classify_bits(bits) {
            Class::Zero { sign, biased_exp } => {
                assert!(!sign);
                assert_eq!(biased_exp, BIAS);
            }
            other => panic!("expected Zero, got {other:?}"),
        }
    }

    #[test]
    fn pack_unpack_roundtrip_one() {
        let bits = pack_finite(false, BiasedExp::ZERO_QUANTUM, Coefficient::ONE);
        match classify_bits(bits) {
            Class::Finite {
                sign,
                biased_exp,
                coefficient,
            } => {
                assert!(!sign);
                assert_eq!(biased_exp, BIAS);
                assert_eq!(coefficient, 1);
            }
            other => panic!("expected Finite, got {other:?}"),
        }
    }

    #[test]
    fn pack_unpack_roundtrip_form_a_max() {
        let coef = FORM_B_THRESHOLD - 1;
        let bits = pack_finite(
            false,
            BiasedExp::ZERO_QUANTUM,
            Coefficient::try_new(coef).unwrap(),
        );
        match classify_bits(bits) {
            Class::Finite { coefficient, .. } => assert_eq!(coefficient, coef),
            other => panic!("expected Finite, got {other:?}"),
        }
    }

    #[test]
    fn pack_unpack_roundtrip_form_b_min() {
        let coef = FORM_B_THRESHOLD;
        let bits = pack_finite(
            false,
            BiasedExp::ZERO_QUANTUM,
            Coefficient::try_new(coef).unwrap(),
        );
        match classify_bits(bits) {
            Class::Finite { coefficient, .. } => assert_eq!(coefficient, coef),
            other => panic!("expected Finite, got {other:?}"),
        }
    }

    #[test]
    fn pack_unpack_roundtrip_max_canonical_coefficient() {
        let bits = pack_finite(true, BiasedExp::MAX, Coefficient::MAX);
        match classify_bits(bits) {
            Class::Finite {
                sign,
                biased_exp,
                coefficient,
            } => {
                assert!(sign);
                assert_eq!(biased_exp, BIASED_EXP_MAX);
                assert_eq!(coefficient, COEFFICIENT_LIMIT - 1);
            }
            other => panic!("expected Finite, got {other:?}"),
        }
    }

    #[test]
    fn non_canonical_form_b_decodes_as_zero() {
        // Build a Form B encoding with coefficient = COEFFICIENT_LIMIT
        // = 10^16 (just past the canonical boundary).
        let coef_target = COEFFICIENT_LIMIT;
        // coef_target = 1 << 53 | (d << 50) | t  where d is bit 50 and
        // t is the low 50 bits.
        let d = (coef_target >> T_BITS) & 0b1;
        let t = coef_target & T_MASK;
        let exp_high2 = 0u64;
        let type_bits = (FORM_B_MARKER << 3) | (exp_high2 << 1) | d;
        let bits = (type_bits << TYPE_SHIFT) | t;
        match classify_bits(bits) {
            Class::Zero { sign, biased_exp } => {
                assert!(!sign);
                assert_eq!(biased_exp, 0);
            }
            other => panic!("expected Zero from non-canonical Form B, got {other:?}"),
        }
    }

    #[test]
    fn infinity_classification() {
        assert_eq!(
            classify_bits(pack_infinity(false)),
            Class::Infinity { sign: false }
        );
        assert_eq!(
            classify_bits(pack_infinity(true)),
            Class::Infinity { sign: true }
        );
        // Intel reference: MASK_INF for decimal64 = 0x78 << 56.
        assert_eq!(pack_infinity(false), 0x7800_0000_0000_0000u64);
        assert_eq!(pack_infinity(true), 0xF800_0000_0000_0000u64);
    }

    #[test]
    fn nan_classification() {
        let qnan = pack_quiet_nan(false, 0);
        assert_eq!(qnan, 0x7C00_0000_0000_0000u64);
        match classify_bits(qnan) {
            Class::QuietNaN { sign, payload } => {
                assert!(!sign);
                assert_eq!(payload, 0);
            }
            other => panic!("expected QuietNaN, got {other:?}"),
        }
        let snan = pack_signaling_nan(false, 0);
        assert_eq!(snan, 0x7E00_0000_0000_0000u64);
        match classify_bits(snan) {
            Class::SignalingNaN { sign, payload } => {
                assert!(!sign);
                assert_eq!(payload, 0);
            }
            other => panic!("expected SignalingNaN, got {other:?}"),
        }
    }

    #[test]
    fn pack_finite_roundtrip_sweep() {
        for sign_bit in [false, true] {
            for &biased_exp in &[0u32, 1, BIAS - 1, BIAS, BIAS + 1, BIASED_EXP_MAX] {
                for &coef in &[
                    0u64,
                    1,
                    1_000,
                    FORM_B_THRESHOLD - 1,
                    FORM_B_THRESHOLD,
                    FORM_B_THRESHOLD + 1,
                    9_000_000_000_000_000,
                    COEFFICIENT_LIMIT - 1,
                ] {
                    let bits = pack_finite(
                        sign_bit,
                        BiasedExp::try_from_biased(biased_exp).unwrap(),
                        Coefficient::try_new(coef).unwrap(),
                    );
                    let class = classify_bits(bits);
                    if coef == 0 {
                        assert_eq!(
                            class,
                            Class::Zero {
                                sign: sign_bit,
                                biased_exp
                            }
                        );
                    } else {
                        assert_eq!(
                            class,
                            Class::Finite {
                                sign: sign_bit,
                                biased_exp,
                                coefficient: coef
                            }
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn decimal_digit_count_basic() {
        assert_eq!(decimal_digit_count(0), 1);
        assert_eq!(decimal_digit_count(9), 1);
        assert_eq!(decimal_digit_count(10), 2);
        assert_eq!(decimal_digit_count(9_999_999_999_999_999), 16);
    }

    #[test]
    fn pow10_basic() {
        assert_eq!(pow10(0), 1);
        assert_eq!(pow10(16), 10_000_000_000_000_000);
        assert_eq!(pow10(19), 10_000_000_000_000_000_000);
    }
}
