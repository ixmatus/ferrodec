// The encode/decode equations are transcribed verbatim from Cowlishaw
// §3.5.2 so the codec can be audited line-by-line against the spec.
// Algebraic minimization (clippy's preferred form) breaks that
// correspondence — keep the literal form.
#![allow(clippy::nonminimal_bool)]

//! Densely Packed Decimal (DPD) declet codec for IEEE 754 decimal64
//! interchange.
//!
//! IEEE 754 defines two equivalent storage encodings for decimal64:
//! BID (the canonical ferrodec storage; the coefficient is a binary
//! integer in the trailing significand field) and DPD (Densely Packed
//! Decimal; the coefficient is encoded as 5 declets of 10 bits each,
//! where every declet packs three decimal digits). This module is the
//! declet primitive — pure boolean equations from Mike Cowlishaw's
//! *A Summary of Densely Packed Decimal Encoding*, published at
//! <https://speleotrove.com/decimal/DPDecimal.html>.
//!
//! No lookup tables: each direction is roughly 30 bit-operations,
//! straight-line on every backend, friendly to Cortex-M0+ where
//! ferrodec runs without hardware divide.
//!
//! ADR-0001 keeps BID as the storage encoding for arithmetic. The DPD
//! interchange surface (`Decimal64::to_dpd_bytes` / `from_dpd_bytes`)
//! is built on top of these primitives and ships behind the `dpd`
//! cargo feature. The same declet boolean primitive is transcribed in
//! the decimal128 and decimal32 codecs; per ADR-0031's
//! precision-local carve-out it stays per crate (each format encodes
//! only its own coefficient's declets). See ADR-0009 (the decimal128
//! DPD interchange this extends to decimal64).
//!
//! # Bit conventions
//!
//! Encoding input is a 12-bit BCD triple, three nibbles packed
//! most-significant-digit first:
//!
//! ```text
//! bits 11..8  : digit d2 (most significant), value 0..=9
//! bits  7..4  : digit d1
//! bits  3..0  : digit d0 (least significant)
//! ```
//!
//! Encoding output is a 10-bit declet, where bit 9 is the MSB.
//! Cowlishaw's notation labels the input bits `a..m` (skipping `l`)
//! and the output bits `p..y`; the mapping is
//!
//! ```text
//! d2 = a b c d   d1 = e f g h   d0 = i j k m
//! declet bits = p q r s t u v w x y   (p = MSB, y = LSB)
//! ```
//!
//! Decoding is total: every 10-bit pattern produces a valid BCD
//! triple. Non-canonical declets (multiple encodings of the same
//! value) decode to the same value as their canonical equivalent;
//! re-encoding the decoded value yields the canonical declet.
//!
//! # Decimal64 interchange framing (IEEE 754-2008 §3.5)
//!
//! ```text
//! bit  63     : sign
//! bits 62..58 : 5-bit combination field G
//! bits 57..50 : 8-bit exponent continuation
//! bits 49..0  : 50-bit trailing significand = 5 declets
//! ```
//!
//! decimal64 has 16 digits of precision; the most-significant digit is
//! carried in the combination field, leaving 15 trailing digits = 5
//! declets × 3 digits. The combination field `G` decodes as:
//!
//! * `G[4..1] = 11110` → ±Infinity.
//! * `G[4..1] = 11111` → NaN. `G[0]` (bit 57) is the signaling bit.
//! * `G[4..3] ∈ {00,01,10}` → finite, Form A: leading decimal digit =
//!   `G[2..0]` ∈ `0..=7`, exponent top two bits = `G[4..3]`.
//! * `G[4..3] = 11` (and not Inf/NaN) → finite, Form B: leading
//!   decimal digit = `8 + G[0]` ∈ `{8,9}`, exponent top two bits =
//!   `G[2..1]`.
//!
//! This DPD combination-field interpretation differs from ferrodec's
//! internal BID type field (`crate::bid`), where the same five bits
//! carry the top binary bits of the coefficient rather than the
//! leading decimal digit. The DPD layout here is the one the upstream
//! `ddEncode.decTest` `#hex` literals use.

use crate::bid::{self, Class};
use crate::decimal::Decimal64;

/// Number of declets in the decimal64 trailing significand.
///
/// decimal64 has 16 digits of precision; the most-significant digit is
/// carried in the combination field, leaving 15 trailing digits = 5
/// declets × 3 digits. The trailing significand width is
/// `DECLET_COUNT * 10 = 50` bits, matching `bid::T_BITS`.
pub(crate) const DECLET_COUNT: usize = 5;

/// `10^15` — splits a 16-digit canonical coefficient into a leading
/// decimal digit (`coef / TEN_POW_15`, value `0..=9`) and a 15-digit
/// trailing remainder (`coef % TEN_POW_15`, value `0..10^15`).
const TEN_POW_15: u64 = 1_000_000_000_000_000;

/// Maximum canonical NaN payload. Non-canonical NaN payloads (binary
/// value ≥ `10^15`) cannot be represented as 5 decimal-digit-triple
/// declets and are emitted as zero on the DPD side, matching the
/// decimal128 codec's treatment of over-range payloads.
const MAX_CANONICAL_NAN_PAYLOAD: u64 = TEN_POW_15;

/// Encode a 12-bit BCD triple into a 10-bit DPD declet.
///
/// Each nibble of `bcd` must be in `0..=9`; the upper four bits
/// (bits 15..12) must be zero. Calling with out-of-range nibbles
/// yields a deterministic but unspecified result.
///
/// The boolean equations are IEEE 754-2008 §3.5.2 (Cowlishaw's DPD
/// summary). They are the same three-digit / ten-bit primitive the
/// in-repo decimal128 and decimal32 codecs use; this transcription
/// cross-checks bit-for-bit against them (see the module-level tests).
pub(crate) fn encode_declet(bcd: u16) -> u16 {
    debug_assert!(bcd >> 12 == 0, "bcd has bits set above bit 11");
    debug_assert!((bcd >> 8) & 0xF < 10, "d2 nibble out of range");
    debug_assert!((bcd >> 4) & 0xF < 10, "d1 nibble out of range");
    debug_assert!(bcd & 0xF < 10, "d0 nibble out of range");

    let bit = |n: u32| (bcd & (1 << n)) != 0;
    let a = bit(11);
    let b = bit(10);
    let c = bit(9);
    let d = bit(8);
    let e = bit(7);
    let f = bit(6);
    let g = bit(5);
    let h = bit(4);
    let i = bit(3);
    let j = bit(2);
    let k = bit(1);
    let m = bit(0);

    let p = b || (a && j) || (a && f && i);
    let q = c || (a && k) || (a && g && i);
    let r = d;
    let s = (f && (!a || !i)) || (!a && e && j) || (e && i);
    let t = g || (!a && e && k) || (a && i);
    let u = h;
    let v = a || e || i;
    let w = a || (e && i) || (!e && j);
    let x = e || (a && i) || (!a && k);
    let y = m;

    pack(p, 9)
        | pack(q, 8)
        | pack(r, 7)
        | pack(s, 6)
        | pack(t, 5)
        | pack(u, 4)
        | pack(v, 3)
        | pack(w, 2)
        | pack(x, 1)
        | pack(y, 0)
}

/// Decode a 10-bit DPD declet into a 12-bit BCD triple.
///
/// Input must satisfy `declet < 1024`. Every such input produces a
/// valid output: each output nibble is in `0..=9`, and the upper four
/// bits of the result are zero.
pub(crate) fn decode_declet(declet: u16) -> u16 {
    debug_assert!(declet < 1024, "declet has bits set above bit 9");

    let bit = |n: u32| (declet & (1 << n)) != 0;
    let p = bit(9);
    let q = bit(8);
    let r = bit(7);
    let s = bit(6);
    let t = bit(5);
    let u = bit(4);
    let v = bit(3);
    let w = bit(2);
    let x = bit(1);
    let y = bit(0);

    let a = (v && w) && (!s || t || !x);
    let b = p && (!v || !w || (s && !t && x));
    let c = q && (!v || !w || (s && !t && x));
    let d = r;
    let e = v && ((!w && x) || (!t && x) || (s && x));
    let f = (s && (!v || !x)) || (p && !s && t && v && w && x);
    let g = (t && (!v || !x)) || (q && !s && t && w);
    let h = u;
    let i = v && ((!w && !x) || (w && x && (s || t)));
    let j = (!v && w) || (s && v && !w && x) || (p && w && (!x || (!s && !t)));
    let k = (!v && x) || (t && !w && x) || (q && v && w && (!x || (!s && !t)));
    let m = y;

    pack(a, 11)
        | pack(b, 10)
        | pack(c, 9)
        | pack(d, 8)
        | pack(e, 7)
        | pack(f, 6)
        | pack(g, 5)
        | pack(h, 4)
        | pack(i, 3)
        | pack(j, 2)
        | pack(k, 1)
        | pack(m, 0)
}

#[inline]
fn pack(bit: bool, shift: u32) -> u16 {
    (bit as u16) << shift
}

// ---------------------------------------------------------------------------
// Decimal64 surface

impl Decimal64 {
    /// Encode this `Decimal64` as 8 bytes in IEEE 754-2019 DPD layout,
    /// big-endian.
    ///
    /// The DPD encoding shares the sign bit and the 8-bit exponent
    /// continuation with BID, but interprets the 5-bit combination
    /// field differently and packs the 50-bit trailing significand as
    /// 5 declets of 10 bits each (15 decimal digits) rather than as a
    /// binary integer. The leading decimal digit (the 16th,
    /// most-significant digit of the coefficient) is carried in the
    /// combination field as a decimal-digit value, not as a binary
    /// prefix.
    ///
    /// Storage encoding for arithmetic stays BID; this is a byte-level
    /// interchange adapter only. See ADR-0001 (BID storage choice) and
    /// ADR-0009 (DPD interchange).
    ///
    /// **Round-trip contract.** The pair
    /// `from_dpd_bytes(to_dpd_bytes(x))` is bit-equal to
    /// `x.canonicalize()`, not necessarily to `x`. The two diverge for
    /// non-canonical inputs reachable through `Decimal64::from_bits`:
    ///
    /// * NaN payloads at or above `10^15` cannot be represented as 5
    ///   declets and canonicalize to a zero payload on the DPD side.
    /// * Non-canonical Form B BID coefficients at or above `10^16`
    ///   decode as zero per IEEE 754-2019 §3.5.2 before the DPD encode
    ///   step (handled in `bid::classify_bits`).
    /// * `Decimal64::INFINITY` with low-bit junk emits canonical
    ///   `±Infinity` bytes; a NaN with bits above the canonical
    ///   payload range emits a canonicalized NaN.
    ///
    /// For any *canonical* `Decimal64` the round-trip is bit-equal.
    ///
    /// # Examples
    ///
    /// ```
    /// use ferrodec_decimal64::Decimal64;
    /// // -7.50 in DPD is `#A2300000000003D0`, matching
    /// // `ddEncode.decTest` dece001.
    /// let d = Decimal64::try_new(-750, -2).unwrap();
    /// let bytes = d.to_dpd_bytes();
    /// assert_eq!(
    ///     bytes,
    ///     [0xA2, 0x30, 0x00, 0x00, 0x00, 0x00, 0x03, 0xD0],
    /// );
    /// ```
    #[must_use]
    pub fn to_dpd_bytes(self) -> [u8; 8] {
        let bits = match bid::classify_bits(self.0) {
            Class::Infinity { sign } => pack_dpd_infinity(sign),
            Class::QuietNaN { sign, payload } => pack_dpd_nan(sign, false, payload),
            Class::SignalingNaN { sign, payload } => pack_dpd_nan(sign, true, payload),
            Class::Zero { sign, biased_exp } => pack_dpd_finite(sign, biased_exp, 0, 0),
            Class::Finite {
                sign,
                biased_exp,
                coefficient,
            } => {
                // Canonical coefficients are < 10^16, so leading_digit
                // is 0..=9 and trailing_15 is < 10^15. `classify_bits`
                // already rejects coefficients ≥ 10^16 (they decode to
                // `Class::Zero`), so this branch is always canonical.
                let leading_digit = coefficient / TEN_POW_15;
                let trailing_15 = coefficient % TEN_POW_15;
                pack_dpd_finite(sign, biased_exp, leading_digit, trailing_15)
            }
        };
        bits.to_be_bytes()
    }

    /// Decode 8 bytes in IEEE 754-2019 DPD layout (big-endian) into a
    /// `Decimal64`.
    ///
    /// Total: every 64-bit pattern decodes to *some* valid value.
    /// Non-canonical inputs (uncanonical declets, NaN payloads ≥
    /// `10^15`) are accepted and decoded under IEEE 754-2019 §3.5.2 —
    /// the value matches the canonical equivalent and no exception is
    /// raised on input. The returned `Decimal64` is BID-encoded, so
    /// arithmetic on it goes through the existing BID kernels.
    #[must_use]
    pub fn from_dpd_bytes(bytes: [u8; 8]) -> Self {
        let bits = u64::from_be_bytes(bytes);
        let sign = (bits >> 63) & 1 == 1;
        let g = ((bits >> 58) & 0b1_1111) as u32;
        let ec = ((bits >> 50) & 0xFF) as u32;
        let dpd_trailing = bits & bid::T_MASK;

        // G[4..1] = 11110 → Infinity; G[4..1] = 11111 → NaN.
        if g >> 1 == 0b1111 {
            if g & 1 == 0 {
                return Decimal64::from_bits(bid::pack_infinity(sign));
            }
            let signaling = (bits >> bid::NAN_SIGNALING_SHIFT) & 1 == 1;
            let payload = decode_trailing(dpd_trailing);
            let bid_bits = if signaling {
                bid::pack_signaling_nan(sign, payload)
            } else {
                bid::pack_quiet_nan(sign, payload)
            };
            return Decimal64::from_bits(bid_bits);
        }

        // Finite. Pull the leading decimal digit and the exponent's top
        // two bits from the combination field. Form A: G[4..3] ∈
        // {00,01,10} gives leading_digit = G[2..0] ∈ 0..=7. Form B:
        // G[4..3] = 11, leading_digit = 8 + G[0] ∈ {8,9}, exp_top_2 =
        // G[2..1].
        let (leading_digit, exp_top_2) = if (g >> 3) == 0b11 {
            (8 + (g & 1), (g >> 1) & 0b11)
        } else {
            (g & 0b111, (g >> 3) & 0b11)
        };
        let biased_exp = (exp_top_2 << 8) | ec;
        let trailing_15 = decode_trailing(dpd_trailing);
        let coefficient = u64::from(leading_digit) * TEN_POW_15 + trailing_15;

        // `biased_exp` is at most `(0b11 << 8) | 0xFF = 1023`, beyond
        // `BIASED_EXP_MAX = 767`. `coefficient` is at most
        // `9 * 10^15 + (10^15 - 1) < 10^16`. The fallible constructors
        // saturate the out-of-canonical-range cases the same way
        // `from_bits` does for non-canonical BID patterns, keeping
        // `from_dpd_bytes` total.
        let biased_exp = bid::BiasedExp::try_from_biased(biased_exp).unwrap_or(bid::BiasedExp::MAX);
        let coefficient = bid::Coefficient::try_new(coefficient).unwrap_or(bid::Coefficient::MAX);
        Decimal64::from_bits(bid::pack_finite(sign, biased_exp, coefficient))
    }
}

/// Build the BID-style raw bits for a DPD-encoded finite value.
///
/// `leading_digit ∈ 0..=9`, `trailing_15 < 10^15`, `biased_exp` ≤
/// `BIASED_EXP_MAX`. The output bit pattern shares the sign / ec
/// fields with BID, but the 5-bit combination field uses the DPD
/// interpretation (G[2..0] is the leading *decimal* digit, not the top
/// binary bits) and the trailing significand holds 5 declets.
fn pack_dpd_finite(sign: bool, biased_exp: u32, leading_digit: u64, trailing_15: u64) -> u64 {
    debug_assert!(leading_digit < 10);
    debug_assert!(trailing_15 < TEN_POW_15);
    debug_assert!(biased_exp <= bid::BIASED_EXP_MAX);

    let exp_top_2 = u64::from((biased_exp >> 8) & 0b11);
    let ec = u64::from(biased_exp & 0xFF);
    let combination = if leading_digit < 8 {
        // Form A: G[4..3] = exp_top_2, G[2..0] = leading_digit.
        (exp_top_2 << 3) | leading_digit
    } else {
        // Form B: G[4..3] = 11, G[2..1] = exp_top_2, G[0] = digit low bit.
        0b11000 | (exp_top_2 << 1) | (leading_digit & 1)
    };
    let dpd_trailing = encode_trailing(trailing_15);

    (u64::from(sign) << 63) | (combination << 58) | (ec << 50) | dpd_trailing
}

fn pack_dpd_infinity(sign: bool) -> u64 {
    (u64::from(sign) << 63) | (0b1_1110_u64 << 58)
}

/// Pack a DPD NaN. Non-canonical BID payloads (binary value ≥ 10^15)
/// can't be represented as 5 declets, so they canonicalize to a zero
/// payload on the DPD side — this matches IEEE 754-2019's treatment of
/// non-canonical NaN payloads and mirrors the decimal128 codec.
fn pack_dpd_nan(sign: bool, signaling: bool, bid_payload: u64) -> u64 {
    let dpd_payload = if bid_payload < MAX_CANONICAL_NAN_PAYLOAD {
        encode_trailing(bid_payload)
    } else {
        0
    };
    (u64::from(sign) << 63)
        | (0b1_1111_u64 << 58)
        | (u64::from(signaling) << bid::NAN_SIGNALING_SHIFT)
        | dpd_payload
}

/// Encode a 15-digit decimal value (`< 10^15`) as 5 declets packed
/// into the low 50 bits of a `u64`. Declet `i` (0 = least significant
/// triple, 4 = most significant) occupies bits `10*i .. 10*i+9`.
fn encode_trailing(value: u64) -> u64 {
    debug_assert!(value < TEN_POW_15);
    let mut remaining = value;
    let mut result: u64 = 0;
    for i in 0..DECLET_COUNT {
        let triple = (remaining % 1000) as u16;
        remaining /= 1000;
        let bcd = ((triple / 100) << 8) | (((triple / 10) % 10) << 4) | (triple % 10);
        let declet = encode_declet(bcd);
        result |= u64::from(declet) << (10 * i);
    }
    debug_assert_eq!(remaining, 0);
    result
}

/// Decode 5 declets in the low 50 bits to a 15-digit decimal value.
/// Total: every input produces a value `< 10^15`.
fn decode_trailing(bits: u64) -> u64 {
    let mut value: u64 = 0;
    for i in (0..DECLET_COUNT).rev() {
        let declet = ((bits >> (10 * i)) & 0x3FF) as u16;
        let bcd = decode_declet(declet);
        let d2 = u64::from(bcd >> 8);
        let d1 = u64::from((bcd >> 4) & 0xF);
        let d0 = u64::from(bcd & 0xF);
        let triple = d2 * 100 + d1 * 10 + d0;
        value = value * 1000 + triple;
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bcd(d2: u16, d1: u16, d0: u16) -> u16 {
        debug_assert!(d2 < 10 && d1 < 10 && d0 < 10);
        (d2 << 8) | (d1 << 4) | d0
    }

    fn nibbles(packed: u16) -> (u16, u16, u16) {
        ((packed >> 8) & 0xF, (packed >> 4) & 0xF, packed & 0xF)
    }

    #[test]
    fn roundtrip_all_canonical_triples() {
        // 1000 canonical (d2, d1, d0) triples — every BCD value with
        // each nibble in 0..=9 must round-trip exactly.
        for d2 in 0..10 {
            for d1 in 0..10 {
                for d0 in 0..10 {
                    let input = bcd(d2, d1, d0);
                    let declet = encode_declet(input);
                    assert!(declet < 1024, "declet {declet:#x} out of range");
                    assert_eq!(decode_declet(declet), input, "round-trip {d2}{d1}{d0}");
                }
            }
        }
    }

    #[test]
    fn decode_total_yields_valid_bcd() {
        // Every 10-bit pattern (including the 24 non-canonical declets)
        // decodes to three BCD digits each in 0..=9, upper bits zero.
        for declet in 0u16..1024 {
            let out = decode_declet(declet);
            let (d2, d1, d0) = nibbles(out);
            assert_eq!(out >> 12, 0, "declet {declet:#x} set bits above bit 11");
            assert!(d2 < 10 && d1 < 10 && d0 < 10, "declet {declet:#x} bad bcd");
        }
    }

    #[test]
    fn known_declet_mappings() {
        // Identity for digits 0..=9 in the low position.
        for d in 0..10u16 {
            assert_eq!(encode_declet(bcd(0, 0, d)), d);
            assert_eq!(decode_declet(d), bcd(0, 0, d));
        }
        // BCD 750 → declet 0x3D0 (the -7.50 vector's least declet).
        assert_eq!(encode_declet(bcd(7, 5, 0)), 0x3D0);
        assert_eq!(decode_declet(0x3D0), bcd(7, 5, 0));
        // BCD 999 — all-large; round-trip is the definitive check.
        assert_eq!(decode_declet(encode_declet(bcd(9, 9, 9))), bcd(9, 9, 9));
    }

    #[test]
    fn exactly_24_non_canonical_declets() {
        // IEEE 754-2008: 1024 patterns = 1000 canonical + 24 redundant.
        let mut non_canonical = 0;
        for declet in 0u16..1024 {
            if encode_declet(decode_declet(declet)) != declet {
                non_canonical += 1;
            }
        }
        assert_eq!(non_canonical, 24);
    }

    #[test]
    fn declet_count_matches_t_bits() {
        assert_eq!(DECLET_COUNT * 10, crate::bid::T_BITS as usize);
    }

    #[test]
    fn ddencode_dece001_vector() {
        // Upstream `ddEncode.decTest`:
        //   dece001 apply #A2300000000003D0 -> -7.50
        // Both directions must match.
        let bytes = [0xA2, 0x30, 0x00, 0x00, 0x00, 0x00, 0x03, 0xD0];
        let d = Decimal64::from_dpd_bytes(bytes);
        let expected = Decimal64::try_new(-750, -2).unwrap();
        assert_eq!(d.to_bits(), expected.to_bits(), "from_dpd_bytes mismatch");
        assert_eq!(expected.to_dpd_bytes(), bytes, "to_dpd_bytes mismatch");
    }

    #[test]
    fn distinguished_constants_roundtrip() {
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
        for d in consts {
            let recovered = Decimal64::from_dpd_bytes(d.to_dpd_bytes());
            assert_eq!(recovered.to_bits(), d.to_bits(), "round-trip {d:?}");
        }
    }

    #[test]
    fn coefficient_boundary_values() {
        // Leading digit 7 (Form A boundary), 8 (Form B start), 9 (Form
        // B end), the 10^15 split, and extreme exponents.
        let cases: &[(i64, i32)] = &[
            (7_999_999_999_999_999, 0),
            (8_000_000_000_000_000, 0),
            (8_999_999_999_999_999, 0),
            (9_000_000_000_000_000, 0),
            (9_999_999_999_999_999, 0), // MAX coefficient, all nines
            (1_000_000_000_000_000, 0), // 10^15
            (i64::from(u32::MAX), 0),
            (1, 369),  // largest quantum exponent (E_MAX - precision + 1)
            (1, -398), // smallest quantum exponent (Etiny)
        ];
        for &(coef, exp) in cases {
            let d = Decimal64::try_new(coef, exp).unwrap();
            let recovered = Decimal64::from_dpd_bytes(d.to_dpd_bytes());
            assert_eq!(
                recovered.to_bits(),
                d.to_bits(),
                "round-trip ({coef}, {exp})"
            );
        }
    }

    #[test]
    fn nan_with_canonical_payload_roundtrip() {
        // Canonical NaN payloads (< 10^15) round-trip bit-equal.
        let payloads = [0u64, 1, 999, 1_000_000_000, TEN_POW_15 - 1];
        for &p in &payloads {
            let d = Decimal64::from_bits(bid::pack_quiet_nan(false, p));
            let recovered = Decimal64::from_dpd_bytes(d.to_dpd_bytes());
            assert_eq!(recovered.to_bits(), d.to_bits(), "NaN payload {p}");
        }
    }

    #[test]
    fn nan_with_non_canonical_payload_round_trips_via_canonicalize() {
        // NaN payloads ≥ 10^15 cannot be represented as 5 declets; the
        // contract is "round-trip equals canonicalize", pinned for both
        // flavours and signs across the boundary.
        let non_canonical = [TEN_POW_15, TEN_POW_15 + 1, bid::T_MASK];
        for &p in &non_canonical {
            for sign in [false, true] {
                for signaling in [false, true] {
                    let bid_bits = if signaling {
                        bid::pack_signaling_nan(sign, p)
                    } else {
                        bid::pack_quiet_nan(sign, p)
                    };
                    let d = Decimal64::from_bits(bid_bits);
                    let recovered = Decimal64::from_dpd_bytes(d.to_dpd_bytes());
                    let canonical = d.canonicalize();
                    assert_eq!(
                        recovered.to_bits(),
                        canonical.to_bits(),
                        "non-canonical NaN {p:#x} sign={sign} sig={signaling}",
                    );
                    assert!(recovered.is_nan());
                    assert_eq!(recovered.is_signaling_nan(), canonical.is_signaling_nan());
                    assert_eq!(recovered.is_sign_negative(), sign);
                }
            }
        }
    }

    #[test]
    fn from_dpd_bytes_is_total() {
        // Spot-check totality: no input panics. The exhaustive guarantee
        // is the Kani harness `from_dpd_bytes_total` and the property
        // test in `tests/property_dpd.rs`.
        let patterns: &[u64] = &[
            0,
            u64::MAX,
            0x1234_5678_9ABC_DEF0,
            0xFFFF_FFFF_FFFF_FFFE,
            0xC0FF_EEFF_FFFF_FFFF, // Form B
            0x7800_0000_0000_0000, // Inf
            0xFC00_0000_0000_03FF, // NaN with non-canonical declet
            0x0000_0000_0000_03FF,
            0xDEAD_BEEF_DEAD_BEEF,
        ];
        for &bits in patterns {
            let _ = Decimal64::from_dpd_bytes(bits.to_be_bytes());
        }
    }

    #[test]
    fn coefficient_is_decimal_not_binary() {
        // Catch the "BID T[2:0] copied as DPD T[2:0]" bug class: 10^15
        // has leading decimal digit 1, but its top binary bits are 0.
        let d = Decimal64::try_new(1_000_000_000_000_000, 0).unwrap();
        let bytes = d.to_dpd_bytes();
        // exp 0 → biased 398 → exp_top_2 = 1, leading digit 1, Form A:
        // combination = (1 << 3) | 1 = 0b01001 = 9.
        let combination = (bytes[0] >> 2) & 0b1_1111;
        assert_eq!(
            combination, 0b01001,
            "G should encode decimal 1, not binary 0"
        );
        let recovered = Decimal64::from_dpd_bytes(bytes);
        assert_eq!(recovered.to_bits(), d.to_bits());
    }
}
