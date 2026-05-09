//! BID (Binary Integer Decimal) encoding for IEEE 754-2019 decimal128.
//!
//! Layout of the 128-bit encoding (`bits[127]` = MSB, `bits[0]` = LSB):
//!
//! ```text
//! bit 127       : sign
//! bits 126..122 : 5-bit "type" field (sometimes called combination top)
//! bits 121..110 : 12-bit exponent continuation
//! bits 109..0   : 110-bit trailing significand
//! ```
//!
//! Decoding the type field `T = bits[126..122]`:
//!
//! * `T = 11110` → ±Infinity (sign from bit 127)
//! * `T = 11111` → NaN. Bit 121 = 1 ⇒ signaling NaN, 0 ⇒ quiet NaN.
//!   Trailing 110 bits are the payload.
//! * `T[4..3] ∈ {00,01,10}` → Form A:
//!     * `biased_exp = T[4..3] || ec`  (14 bits)
//!     * `coefficient = 0 || T[2..0] || trailing_significand` (113 bits)
//! * `T[4..3] = 11` (and not Inf/NaN) → Form B:
//!     * `biased_exp = T[2..1] || ec`  (14 bits)
//!     * `coefficient = 100 || T[0] || trailing_significand` (114 bits, ≥ 2^113)
//!     * For BID-128 every Form B coefficient is ≥ 2^113 > 10^34 − 1, so
//!       Form B encodings are *non-canonical* and represent ±0 with the
//!       given biased exponent.
//!
//! IEEE 754 parameters for decimal128: precision p = 34 digits, emax = 6144,
//! emin = −6143, bias = 6176, biased exponent range 0 .. 12287.

// A handful of constants below are unused in the foundations layer but will
// be consumed by the arithmetic modules (add/sub/mul/...). Declaring them
// here keeps the BID layout in one place.

// Bit-position constants -----------------------------------------------------

pub(crate) const SIGN_SHIFT: u32 = 127;

/// Top of the 5-bit type field.
pub(crate) const TYPE_SHIFT: u32 = 122;
/// Mask for the 5-bit type field, in place.
#[allow(dead_code)] // consumed by arithmetic modules
pub(crate) const TYPE_MASK: u128 = 0b1_1111u128 << TYPE_SHIFT;

/// Position of the signaling-bit within a NaN encoding.
pub(crate) const NAN_SIGNALING_SHIFT: u32 = 121;

/// Top of the 12-bit exponent continuation.
pub(crate) const EC_SHIFT: u32 = 110;
#[allow(dead_code)]
pub(crate) const EC_BITS: u32 = 12;
#[allow(dead_code)]
pub(crate) const EC_MASK: u128 = ((1u128 << EC_BITS) - 1) << EC_SHIFT;

/// Width of the trailing significand, in bits.
pub(crate) const T_BITS: u32 = 110;
pub(crate) const T_MASK: u128 = (1u128 << T_BITS) - 1;

// IEEE 754 decimal128 parameters --------------------------------------------

/// Decimal digits of precision.
pub(crate) const PRECISION: u32 = 34;
/// Maximum unbiased exponent.
#[allow(dead_code)]
pub(crate) const E_MAX: i32 = 6144;
/// Minimum unbiased exponent.
#[allow(dead_code)]
pub(crate) const E_MIN: i32 = 1 - E_MAX;
/// Bias added to the unbiased quantum exponent for storage.
pub(crate) const BIAS: u32 = 6176;
/// Largest valid biased exponent (`Q_MAX` - `Q_MIN` where `Q_MIN` = -BIAS).
pub(crate) const BIASED_EXP_MAX: u32 = 12287;
/// `10^34` — the strict upper bound on a canonical coefficient.
pub(crate) const COEFFICIENT_LIMIT: u128 = 10u128.pow(34);
/// `2^113` — width of the binary coefficient field.
pub(crate) const COEFFICIENT_FIELD_LIMIT: u128 = 1u128 << 113;

// Decoded form of an encoding ------------------------------------------------

/// Result of decoding the bit pattern of a [`Decimal128`].
///
/// The decode never fails: every 128-bit input maps to exactly one variant.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Class {
    /// Finite, non-zero value with `coefficient ∈ [1, 10^34 − 1]`.
    Finite {
        sign: bool,
        biased_exp: u32,
        coefficient: u128,
    },
    /// Numerical zero. `biased_exp` is preserved so `total_cmp` can
    /// distinguish cohorts (`+0E+0`, `+0E+1`, …).
    Zero { sign: bool, biased_exp: u32 },
    /// ±Infinity.
    Infinity { sign: bool },
    /// Quiet NaN with the given trailing-significand payload.
    QuietNaN { sign: bool, payload: u128 },
    /// Signaling NaN with the given trailing-significand payload.
    SignalingNaN { sign: bool, payload: u128 },
}

// Decoding -------------------------------------------------------------------

/// Read the sign bit.
#[inline]
pub(crate) const fn sign_of(bits: u128) -> bool {
    (bits >> SIGN_SHIFT) & 1 == 1
}

/// Read the 5-bit type field.
#[inline]
pub(crate) const fn type_field(bits: u128) -> u32 {
    ((bits >> TYPE_SHIFT) & 0b1_1111) as u32
}

/// Decompose `bits` into its [`Class`].
#[inline]
pub(crate) const fn classify_bits(bits: u128) -> Class {
    let sign = sign_of(bits);
    let t = type_field(bits);

    if t == 0b1_1110 {
        return Class::Infinity { sign };
    }
    if t == 0b1_1111 {
        let signaling = ((bits >> NAN_SIGNALING_SHIFT) & 1) == 1;
        let payload = bits & T_MASK;
        return if signaling {
            Class::SignalingNaN { sign, payload }
        } else {
            Class::QuietNaN { sign, payload }
        };
    }

    let ec = ((bits >> EC_SHIFT) & 0xFFF) as u32;
    let top2 = t >> 3;

    if top2 == 0b11 {
        // Form B — non-canonical for BID-128. Treat as ±0 with this biased exp.
        let exp_high2 = (t >> 1) & 0b11; // T[2..1]
        let biased_exp = (exp_high2 << 12) | ec;
        return Class::Zero { sign, biased_exp };
    }

    // Form A
    let exp_high2 = top2; // T[4..3]
    let coef_high3 = (t & 0b111) as u128; // T[2..0]
    let biased_exp = (exp_high2 << 12) | ec;
    let coefficient = (coef_high3 << T_BITS) | (bits & T_MASK);

    // Per IEEE 754-2019 §3.5.2, a Form A coefficient that exceeds 10^p − 1 is
    // non-canonical: the value of the floating-point datum is zero with the
    // encoded sign and biased exponent. Canonicalising at decode time keeps
    // every downstream consumer (arithmetic, DPD encode, format) safe by
    // construction.
    if coefficient == 0 || coefficient >= COEFFICIENT_LIMIT {
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

/// Pack a finite (or zero) value as Form A.
///
/// Caller guarantees `coefficient < 2^113` (which trivially includes the
/// canonical range `< 10^34`) and `biased_exp <= BIASED_EXP_MAX`.
#[inline]
pub(crate) const fn pack_finite(sign: bool, biased_exp: u32, coefficient: u128) -> u128 {
    debug_assert!(coefficient < COEFFICIENT_FIELD_LIMIT);
    debug_assert!(biased_exp <= BIASED_EXP_MAX);
    let s = (sign as u128) << SIGN_SHIFT;
    let exp_high2 = ((biased_exp >> 12) & 0b11) as u128; // 2 bits → T[4..3]
    let coef_high3 = (coefficient >> T_BITS) & 0b111; // 3 bits → T[2..0]
    let type_bits = (exp_high2 << 3) | coef_high3; // 5 bits
    let ec = (biased_exp & 0xFFF) as u128;
    let t = coefficient & T_MASK;
    s | (type_bits << TYPE_SHIFT) | (ec << EC_SHIFT) | t
}

#[inline]
pub(crate) const fn pack_infinity(sign: bool) -> u128 {
    let s = (sign as u128) << SIGN_SHIFT;
    s | (0b1_1110u128 << TYPE_SHIFT)
}

#[inline]
pub(crate) const fn pack_quiet_nan(sign: bool, payload: u128) -> u128 {
    let s = (sign as u128) << SIGN_SHIFT;
    s | (0b1_1111u128 << TYPE_SHIFT) | (payload & T_MASK)
}

#[inline]
pub(crate) const fn pack_signaling_nan(sign: bool, payload: u128) -> u128 {
    let s = (sign as u128) << SIGN_SHIFT;
    s | (0b1_1111u128 << TYPE_SHIFT) | (1u128 << NAN_SIGNALING_SHIFT) | (payload & T_MASK)
}

// Helpers --------------------------------------------------------------------

/// Number of significant decimal digits in `n`. Returns `1` when `n == 0`,
/// matching the IEEE 754 convention for "digits of zero".
#[inline]
pub(crate) const fn decimal_digit_count(n: u128) -> u32 {
    if n == 0 {
        1
    } else {
        n.ilog10() + 1
    }
}

/// `10^k` for `k <= 38` (the largest power of ten that fits in `u128`).
///
/// `k = 39` overflows `u128`. Caller is responsible for staying within range;
/// in `const` contexts this is enforced by `debug_assert!`.
#[inline]
pub(crate) const fn pow10(k: u32) -> u128 {
    debug_assert!(k <= 38);
    10u128.pow(k)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn precision_consistent() {
        assert_eq!(PRECISION, 34);
        assert!(COEFFICIENT_LIMIT < COEFFICIENT_FIELD_LIMIT);
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
        let bits = pack_finite(false, BIAS, 0);
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
        let bits = pack_finite(false, BIAS, 1);
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
    fn pack_unpack_roundtrip_max_coefficient() {
        // max canonical coefficient = 10^34 - 1
        let coef = COEFFICIENT_LIMIT - 1;
        let bits = pack_finite(true, BIASED_EXP_MAX, coef);
        match classify_bits(bits) {
            Class::Finite {
                sign,
                biased_exp,
                coefficient,
            } => {
                assert!(sign);
                assert_eq!(biased_exp, BIASED_EXP_MAX);
                assert_eq!(coefficient, coef);
            }
            other => panic!("expected Finite, got {other:?}"),
        }
    }

    #[test]
    fn non_canonical_form_a_decodes_as_zero() {
        // Coefficient = 2^113 − 1 is encodable in Form A but exceeds the
        // canonical limit 10^34 − 1. Per IEEE 754-2019 §3.5.2 the decoder
        // must canonicalise it to Zero with the encoded biased exponent
        // and sign; otherwise downstream paths (arithmetic, DPD encode,
        // format) inherit a poisoned coefficient.
        for &biased_exp in &[0u32, BIAS, BIASED_EXP_MAX] {
            for sign in [false, true] {
                let bits = pack_finite(sign, biased_exp, COEFFICIENT_FIELD_LIMIT - 1);
                assert_eq!(
                    classify_bits(bits),
                    Class::Zero { sign, biased_exp },
                    "non-canonical Form A coef={:#x} biased_exp={biased_exp} sign={sign}",
                    COEFFICIENT_FIELD_LIMIT - 1
                );
            }
        }
        // Boundary: exactly 10^34 is the first non-canonical value.
        let bits = pack_finite(false, BIAS, COEFFICIENT_LIMIT);
        assert_eq!(
            classify_bits(bits),
            Class::Zero {
                sign: false,
                biased_exp: BIAS
            },
        );
        // One below the boundary stays Finite.
        let bits = pack_finite(false, BIAS, COEFFICIENT_LIMIT - 1);
        assert!(matches!(classify_bits(bits), Class::Finite { .. }));
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
        // Intel reference: MASK_INF = 0x78<<120
        assert_eq!(
            pack_infinity(false),
            0x7800_0000_0000_0000_0000_0000_0000_0000u128
        );
        assert_eq!(
            pack_infinity(true),
            0xF800_0000_0000_0000_0000_0000_0000_0000u128
        );
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
        // Intel reference: MASK_NAN = 0x7C<<120
        assert_eq!(bits, 0x7C00_0000_0000_0000_0000_0000_0000_0000u128);
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
        // Intel reference: MASK_SNAN = 0x7E<<120
        assert_eq!(bits, 0x7E00_0000_0000_0000_0000_0000_0000_0000u128);
    }

    #[test]
    fn nan_payload_preserved() {
        let payload = 0x0123_4567_89AB_CDEFu128;
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
    fn form_b_decodes_as_zero() {
        // Build a Form-B encoding by hand: type = 11000, ec = 0x123,
        // trailing significand = 0xABCD.
        let bits = (0b1_1000u128 << TYPE_SHIFT) | (0x123u128 << EC_SHIFT) | 0xABCDu128;
        match classify_bits(bits) {
            Class::Zero { sign, biased_exp } => {
                assert!(!sign);
                // Form B: biased_exp = T[2..1] || ec = 00 || 0x123 = 0x123
                assert_eq!(biased_exp, 0x123);
            }
            other => panic!("expected Zero from Form B, got {other:?}"),
        }
    }

    #[test]
    fn form_a_decodes_zero_when_coefficient_zero() {
        // type = 01000 (exp_high2 = 01, coef_high3 = 000), ec = 0x820 → biased = 6176.
        let bits = (0b0_1000u128 << TYPE_SHIFT) | (0x820u128 << EC_SHIFT);
        match classify_bits(bits) {
            Class::Zero { sign, biased_exp } => {
                assert!(!sign);
                assert_eq!(biased_exp, BIAS);
            }
            other => panic!("expected Zero, got {other:?}"),
        }
    }

    #[test]
    fn pack_finite_roundtrip_sweep_random_pattern() {
        // Sweep a deterministic set of packed values to catch any shift errors.
        for sign_bit in [false, true] {
            for &biased_exp in &[0u32, 1, BIAS - 1, BIAS, BIAS + 1, BIASED_EXP_MAX] {
                for &coef in &[
                    0u128,
                    1,
                    1 << 60,
                    (1 << 110) - 1,
                    1 << 110,
                    (1 << 113) - 1,
                    COEFFICIENT_LIMIT - 1,
                    COEFFICIENT_LIMIT,
                ] {
                    let bits = pack_finite(sign_bit, biased_exp, coef);
                    let class = classify_bits(bits);
                    if coef == 0 || coef >= COEFFICIENT_LIMIT {
                        assert_eq!(
                            class,
                            Class::Zero {
                                sign: sign_bit,
                                biased_exp
                            },
                            "sign={sign_bit} exp={biased_exp} coef={coef:#x} should canonicalise to Zero"
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
}
