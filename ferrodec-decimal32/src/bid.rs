//! BID (Binary Integer Decimal) encoding for IEEE 754-2019 decimal32.
//!
//! Layout of the 32-bit encoding (`bits[31]` = MSB, `bits[0]` = LSB):
//!
//! ```text
//! bit 31      : sign
//! bits 30..26 : 5-bit "type" field (combination top)
//! bits 25..20 : 6-bit exponent continuation
//! bits 19..0  : 20-bit trailing significand
//! ```
//!
//! Decoding the type field `T = bits[30..26]`:
//!
//! * `T = 11110` → ±Infinity (sign from bit 31)
//! * `T = 11111` → NaN. Bit 25 = 1 ⇒ signaling NaN, 0 ⇒ quiet NaN.
//!   Trailing 20 bits are the payload.
//! * `T[4..3] ∈ {00,01,10}` → Form A:
//!     * `biased_exp = T[4..3] || ec`  (2 + 6 = 8 bits, range 0..191)
//!     * `coefficient = 0 || T[2..0] || trailing_significand`
//!       (3 + 20 = 23 bits, range `0..8_388_607`)
//! * `T[4..3] = 11` (and not Inf/NaN) → Form B:
//!     * `biased_exp = T[2..1] || ec`  (2 + 6 = 8 bits, range 0..191)
//!     * `coefficient = 100 || T[0] || trailing_significand`
//!       (3 + 1 + 20 = 24 bits, range `8_388_608..10_485_759`)
//!
//! Unlike BID-128 (where Form B is always non-canonical because every
//! Form B coefficient exceeds the canonical limit 10³⁴ − 1), BID-32 uses
//! Form B for the upper ~16% of the canonical coefficient range,
//! `8_388_608..9_999_999`. Form B encodings of coefficients ≥ 10⁷ are
//! non-canonical and decode to ±0 with the encoded sign and biased
//! exponent (per IEEE 754-2019 §3.5.2).
//!
//! IEEE 754 parameters for decimal32: precision p = 7 digits, emax = 96,
//! emin = −95, bias = 101, biased exponent range 0..191.

// Items marked `#[allow(dead_code)]` below are unused in the foundations
// layer but will be consumed by the parse, format, and arithmetic modules
// landed in subsequent commits per the plan archived at
// `docs/decisions/plans/2026-05-09-workspace-and-decimal-siblings.md`.
// As those modules begin consuming, the per-item `allow` attributes are
// removed.

// Bit-position constants -----------------------------------------------------

pub(crate) const SIGN_SHIFT: u32 = 31;

/// Top of the 5-bit type field.
pub(crate) const TYPE_SHIFT: u32 = 26;
/// Mask for the 5-bit type field, in place.
#[allow(dead_code)] // consumed by arithmetic modules
pub(crate) const TYPE_MASK: u32 = 0b11111u32 << TYPE_SHIFT;

/// Type-field marker for ±Infinity.
pub(crate) const TYPE_INFINITY: u32 = 0b11110;
/// Type-field marker for ±NaN (signaling bit at `NAN_SIGNALING_SHIFT`).
pub(crate) const TYPE_NAN: u32 = 0b11111;
/// Two-bit Form B prefix occupying T[4..3] of the type field.
pub(crate) const FORM_B_MARKER: u32 = 0b11;

/// Position of the signaling-bit within a NaN encoding.
pub(crate) const NAN_SIGNALING_SHIFT: u32 = 25;

/// Top of the 6-bit exponent continuation.
pub(crate) const EC_SHIFT: u32 = 20;
#[allow(dead_code)]
pub(crate) const EC_BITS: u32 = 6;
#[allow(dead_code)]
pub(crate) const EC_MASK: u32 = ((1u32 << EC_BITS) - 1) << EC_SHIFT;

/// Width of the trailing significand, in bits.
pub(crate) const T_BITS: u32 = 20;
pub(crate) const T_MASK: u32 = (1u32 << T_BITS) - 1;

// IEEE 754 decimal32 parameters --------------------------------------------

/// Decimal digits of precision.
pub(crate) const PRECISION: u32 = 7;
/// Maximum unbiased exponent.
#[allow(dead_code)]
pub(crate) const E_MAX: i32 = 96;
/// Minimum unbiased exponent.
#[allow(dead_code)]
pub(crate) const E_MIN: i32 = 1 - E_MAX;
/// Bias added to the unbiased quantum exponent for storage.
pub(crate) const BIAS: u32 = 101;
/// Largest valid biased exponent.
pub(crate) const BIASED_EXP_MAX: u32 = 191;
/// `10^7` — the strict upper bound on a canonical coefficient.
pub(crate) const COEFFICIENT_LIMIT: u32 = 10u32.pow(7);
/// `2^23` — the boundary between Form A (coefficient < 2²³) and Form B
/// (coefficient ≥ 2²³). Distinct from the canonical limit because
/// `2^23 < 10^7`: Form B carries canonical coefficients for the range
/// `[2^23, 10^7)`.
#[allow(dead_code)]
pub(crate) const FORM_B_THRESHOLD: u32 = 1u32 << 23;
/// `2^23 + 2 · 2^20` (= `10 · 2^20`) — the strict upper bound on any
/// Form B coefficient. Encodings between `COEFFICIENT_LIMIT` and this
/// value are non-canonical and decode to ±0.
#[allow(dead_code)]
pub(crate) const COEFFICIENT_FIELD_LIMIT: u32 = (1u32 << 23) + 2 * (1u32 << T_BITS);

// Type-level invariants on encoded fields -----------------------------------

/// A biased exponent in the canonical range `[0, BIASED_EXP_MAX]` for
/// decimal32. Constructed via the fallible or saturating constructors below;
/// `pack_finite` accepts only this typed value, so the encoded field is
/// guaranteed by the type system to fit in the BID-32 8-bit slot.
///
/// Replaces a pre-slice `debug_assert!` precondition that admitted release
/// mode garbage bits on input derived arithmetic (see `KNOWN_ISSUES.md` H3
/// and the decimal32 correctness slice plan). The invariant now holds at the
/// type level rather than only in debug builds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
pub(crate) struct BiasedExp(u32);

impl BiasedExp {
    /// The minimum biased exponent: `0`, corresponding to quantum `-BIAS = -101`.
    pub(crate) const MIN: Self = Self(0);
    /// The maximum biased exponent: `BIASED_EXP_MAX = 191`, corresponding to
    /// quantum `+90`.
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
    /// `[-BIAS, BIASED_EXP_MAX as i32 - BIAS as i32] = [-101, +90]` for
    /// decimal32.
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
/// decimal32. Like `BiasedExp`, replaces a pre-slice `debug_assert!`
/// precondition on `pack_finite` so the invariant holds at the type level
/// rather than only in debug builds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
pub(crate) struct Coefficient(u32);

impl Coefficient {
    /// The zero coefficient.
    pub(crate) const ZERO: Self = Self(0);
    /// The unit coefficient `1`.
    pub(crate) const ONE: Self = Self(1);
    /// The maximum canonical coefficient: `COEFFICIENT_LIMIT - 1`.
    pub(crate) const MAX: Self = Self(COEFFICIENT_LIMIT - 1);

    /// Construct from a raw `u32`. Returns `None` when the value reaches or
    /// exceeds `COEFFICIENT_LIMIT = 10^7`.
    #[inline]
    #[must_use]
    pub(crate) const fn try_new(coefficient: u32) -> Option<Self> {
        if coefficient < COEFFICIENT_LIMIT {
            Some(Self(coefficient))
        } else {
            None
        }
    }

    /// The underlying coefficient value.
    #[inline]
    #[must_use]
    pub(crate) const fn get(self) -> u32 {
        self.0
    }
}

// Decoded form of an encoding ------------------------------------------------

/// Result of decoding the bit pattern of a [`Decimal32`].
///
/// The decode never fails: every 32-bit input maps to exactly one variant.
///
/// [`Decimal32`]: crate::Decimal32
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Class {
    /// Finite, non-zero value with `coefficient ∈ [1, 10^7 − 1]`.
    Finite {
        sign: bool,
        biased_exp: u32,
        coefficient: u32,
    },
    /// Numerical zero. `biased_exp` is preserved so `total_cmp` can
    /// distinguish cohorts (`+0E+0`, `+0E+1`, …).
    Zero { sign: bool, biased_exp: u32 },
    /// ±Infinity.
    Infinity { sign: bool },
    /// Quiet NaN with the given trailing-significand payload.
    QuietNaN { sign: bool, payload: u32 },
    /// Signaling NaN with the given trailing-significand payload.
    SignalingNaN { sign: bool, payload: u32 },
}

// Decoding -------------------------------------------------------------------

/// Read the sign bit.
#[inline]
pub(crate) const fn sign_of(bits: u32) -> bool {
    (bits >> SIGN_SHIFT) & 1 == 1
}

/// Read the 5-bit type field.
#[inline]
pub(crate) const fn type_field(bits: u32) -> u32 {
    (bits >> TYPE_SHIFT) & 0b1_1111
}

/// Decompose `bits` into its [`Class`].
#[inline]
pub(crate) const fn classify_bits(bits: u32) -> Class {
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

    let ec = (bits >> EC_SHIFT) & ((1 << EC_BITS) - 1);
    let top2 = t >> 3;

    if top2 == FORM_B_MARKER {
        // Form B
        let exp_high2 = (t >> 1) & 0b11; // T[2..1]
        let biased_exp = (exp_high2 << EC_BITS) | ec;
        // Significand prefix is the literal "100" (3 bits) || T[0]; the
        // implicit leading 1<<23 is added explicitly here.
        let coefficient = (1u32 << 23) | ((t & 0b1) << T_BITS) | (bits & T_MASK);

        // Per IEEE 754-2019 §3.5.2, a Form B coefficient that exceeds 10^p − 1
        // is non-canonical: the value is zero with the encoded sign and biased
        // exponent. Canonicalising at decode time keeps every downstream
        // consumer (arithmetic, DPD encode, format) safe by construction.
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
    let exp_high2 = top2; // T[4..3]
    let coef_high3 = t & 0b111; // T[2..0]
    let biased_exp = (exp_high2 << EC_BITS) | ec;
    let coefficient = (coef_high3 << T_BITS) | (bits & T_MASK);

    // Form A coefficients always fit in [0, 2^23) < 10^7, so no canonical
    // check needed beyond the zero distinction. Coefficient zero is decoded
    // as Zero so downstream arithmetic does not have to special-case
    // coefficient = 0 alongside true zeros.
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
) -> u32 {
    let s = (sign as u32) << SIGN_SHIFT;
    let bexp = biased_exp.get();
    let exp_high2 = (bexp >> EC_BITS) & 0b11; // 2 bits → top 2 of biased_exp
    let ec = bexp & ((1 << EC_BITS) - 1);
    let coef = coefficient.get();
    let t = coef & T_MASK;

    if coef < FORM_B_THRESHOLD {
        // Form A: coefficient fits in 23 bits, top 3 bits go in T[2..0].
        let coef_high3 = (coef >> T_BITS) & 0b111;
        let type_bits = (exp_high2 << 3) | coef_high3;
        s | (type_bits << TYPE_SHIFT) | (ec << EC_SHIFT) | t
    } else {
        // Form B: coefficient is in [2^23, 10^7), encoded as 100_d_xxxx
        // where d is bit 20 of the coefficient. T[4..3] = 11 marks Form B,
        // T[2..1] = exp_high2, T[0] = d.
        let d = (coef >> T_BITS) & 0b1;
        let type_bits = (FORM_B_MARKER << 3) | (exp_high2 << 1) | d;
        s | (type_bits << TYPE_SHIFT) | (ec << EC_SHIFT) | t
    }
}

#[inline]
pub(crate) const fn pack_infinity(sign: bool) -> u32 {
    let s = (sign as u32) << SIGN_SHIFT;
    s | (TYPE_INFINITY << TYPE_SHIFT)
}

#[inline]
pub(crate) const fn pack_quiet_nan(sign: bool, payload: u32) -> u32 {
    let s = (sign as u32) << SIGN_SHIFT;
    s | (TYPE_NAN << TYPE_SHIFT) | (payload & T_MASK)
}

#[inline]
pub(crate) const fn pack_signaling_nan(sign: bool, payload: u32) -> u32 {
    let s = (sign as u32) << SIGN_SHIFT;
    s | (TYPE_NAN << TYPE_SHIFT) | (1u32 << NAN_SIGNALING_SHIFT) | (payload & T_MASK)
}

// Helpers --------------------------------------------------------------------

/// Number of significant decimal digits in `n`. Returns `1` when `n == 0`,
/// matching the IEEE 754 convention for "digits of zero".
#[inline]
#[allow(dead_code)] // consumed by arithmetic modules
pub(crate) const fn decimal_digit_count(n: u32) -> u32 {
    if n == 0 {
        1
    } else {
        n.ilog10() + 1
    }
}

/// `10^k` for `k <= 9` (the largest power of ten that fits in `u32`).
///
/// `k = 10` overflows `u32`. Caller is responsible for staying within range;
/// in `const` contexts this is enforced by `debug_assert!`.
#[inline]
#[allow(dead_code)] // consumed by arithmetic modules
pub(crate) const fn pow10(k: u32) -> u32 {
    debug_assert!(k <= 9);
    10u32.pow(k)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn precision_consistent() {
        assert_eq!(PRECISION, 7);
        assert_eq!(COEFFICIENT_LIMIT, 10_000_000);
        // Form B's encodable range strictly contains the canonical range
        // beyond 2^23.
        assert!(COEFFICIENT_LIMIT < COEFFICIENT_FIELD_LIMIT);
        // Form A holds canonical coefficients below 2^23.
        assert!(FORM_B_THRESHOLD < COEFFICIENT_LIMIT);
    }

    #[test]
    fn biased_exp_max_consistent() {
        // Q_MAX - Q_MIN = (E_MAX - p + 1) - (E_MIN - p + 1) = E_MAX - E_MIN.
        // E_MIN = 1 - E_MAX, so range = 2*E_MAX - 1.
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
        // Largest coefficient that still fits in Form A: 2^23 - 1.
        let coef = FORM_B_THRESHOLD - 1;
        let bits = pack_finite(
            false,
            BiasedExp::ZERO_QUANTUM,
            Coefficient::try_new(coef).unwrap(),
        );
        match classify_bits(bits) {
            Class::Finite {
                sign,
                biased_exp,
                coefficient,
            } => {
                assert!(!sign);
                assert_eq!(biased_exp, BIAS);
                assert_eq!(coefficient, coef);
            }
            other => panic!("expected Finite, got {other:?}"),
        }
    }

    #[test]
    fn pack_unpack_roundtrip_form_b_min() {
        // Smallest Form B coefficient: 2^23.
        let coef = FORM_B_THRESHOLD;
        let bits = pack_finite(
            false,
            BiasedExp::ZERO_QUANTUM,
            Coefficient::try_new(coef).unwrap(),
        );
        match classify_bits(bits) {
            Class::Finite {
                sign,
                biased_exp,
                coefficient,
            } => {
                assert!(!sign);
                assert_eq!(biased_exp, BIAS);
                assert_eq!(coefficient, coef);
            }
            other => panic!("expected Finite, got {other:?}"),
        }
    }

    #[test]
    fn pack_unpack_roundtrip_max_canonical_coefficient() {
        // Largest canonical coefficient: 10^7 - 1 = 9_999_999. This sits in
        // Form B (since 10^7 - 1 > 2^23).
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
        // Build a Form B encoding by hand whose coefficient exceeds the
        // canonical limit 10^7. Per IEEE 754-2019 §3.5.2 the decoder must
        // canonicalise it to Zero with the encoded biased exponent and sign.
        //
        // Construct coefficient = 10_000_000 = 0b1001_1000_1001_0110_1000_0000
        // (24 bits). The top 3 bits "100" are the implicit Form B prefix; the
        // 4th bit (T[0]) is 1; the low 20 bits are 0x89680.
        let exp_high2 = 0u32; // top 2 bits of biased_exp
        let t_zero = 1u32; // T[0] = bit 20 of the coefficient
        let type_bits = (FORM_B_MARKER << 3) | (exp_high2 << 1) | t_zero;
        let bits = (type_bits << TYPE_SHIFT) | 0x89680u32;
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
        // Intel reference: MASK_INF for decimal32 = 0x78 << 24
        assert_eq!(pack_infinity(false), 0x7800_0000u32);
        assert_eq!(pack_infinity(true), 0xF800_0000u32);
    }

    #[test]
    fn quiet_nan_classification() {
        let bits = pack_quiet_nan(false, 0);
        match classify_bits(bits) {
            Class::QuietNaN { sign, payload } => {
                assert!(!sign);
                assert_eq!(payload, 0);
            }
            other => panic!("expected QuietNaN, got {other:?}"),
        }
        // Intel reference: MASK_NAN for decimal32 = 0x7C << 24
        assert_eq!(bits, 0x7C00_0000u32);
    }

    #[test]
    fn signaling_nan_classification() {
        let bits = pack_signaling_nan(false, 0);
        match classify_bits(bits) {
            Class::SignalingNaN { sign, payload } => {
                assert!(!sign);
                assert_eq!(payload, 0);
            }
            other => panic!("expected SignalingNaN, got {other:?}"),
        }
        // Intel reference: MASK_SNAN for decimal32 = 0x7E << 24
        assert_eq!(bits, 0x7E00_0000u32);
    }

    #[test]
    fn nan_payload_preserved() {
        let payload = 0x0001_2345u32 & T_MASK;
        let bits = pack_quiet_nan(true, payload);
        match classify_bits(bits) {
            Class::QuietNaN { sign, payload: p } => {
                assert!(sign);
                assert_eq!(p, payload);
            }
            other => panic!("expected QuietNaN, got {other:?}"),
        }
    }

    #[test]
    fn form_a_decodes_zero_when_coefficient_zero() {
        // Form A type field = exp_high2 (2 bits) || coef_high3 (3 bits).
        // Use exp_high2 = 0b01 and coef_high3 = 0b000 so type = 0b01000.
        // ec = 0x25 (37) gives biased_exp = (0b01 << 6) | 37 = 64 + 37 = 101 = BIAS.
        let exp_high2 = 0b01u32;
        let coef_high3 = 0b000u32;
        let type_bits = (exp_high2 << 3) | coef_high3;
        let bits = (type_bits << TYPE_SHIFT) | (0x25u32 << EC_SHIFT);
        match classify_bits(bits) {
            Class::Zero { sign, biased_exp } => {
                assert!(!sign);
                assert_eq!(biased_exp, BIAS);
            }
            other => panic!("expected Zero, got {other:?}"),
        }
    }

    #[test]
    fn pack_finite_roundtrip_sweep() {
        // Sweep a deterministic set of packed values to catch any shift
        // errors. Includes Form A, Form B, and the boundary between them.
        for sign_bit in [false, true] {
            for &biased_exp in &[0u32, 1, BIAS - 1, BIAS, BIAS + 1, BIASED_EXP_MAX] {
                for &coef in &[
                    0u32,
                    1,
                    1_000,
                    FORM_B_THRESHOLD - 1,
                    FORM_B_THRESHOLD,
                    FORM_B_THRESHOLD + 1,
                    9_000_000,
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
                            },
                            "sign={sign_bit} exp={biased_exp} coef=0 should canonicalise to Zero"
                        );
                    } else {
                        assert_eq!(
                            class,
                            Class::Finite {
                                sign: sign_bit,
                                biased_exp,
                                coefficient: coef
                            },
                            "sign={sign_bit} exp={biased_exp} coef={coef:#x}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn decimal_digit_count_basic() {
        assert_eq!(decimal_digit_count(0), 1);
        assert_eq!(decimal_digit_count(1), 1);
        assert_eq!(decimal_digit_count(9), 1);
        assert_eq!(decimal_digit_count(10), 2);
        assert_eq!(decimal_digit_count(99), 2);
        assert_eq!(decimal_digit_count(100), 3);
        assert_eq!(decimal_digit_count(9_999_999), 7);
    }

    #[test]
    fn pow10_basic() {
        assert_eq!(pow10(0), 1);
        assert_eq!(pow10(1), 10);
        assert_eq!(pow10(7), 10_000_000);
        assert_eq!(pow10(9), 1_000_000_000);
    }
}
