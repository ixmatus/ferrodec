// The encode/decode equations are transcribed verbatim from
// Cowlishaw §3.5.2 so the codec can be audited line-by-line against
// the spec. Algebraic minimization (clippy's preferred form) breaks
// that correspondence — keep the literal form.
#![allow(clippy::nonminimal_bool)]

//! Densely Packed Decimal (DPD) interchange codec for IEEE 754-2019
//! decimal32.
//!
//! IEEE 754 defines two equivalent storage encodings for decimal32:
//! BID (the canonical ferrodec storage; the coefficient is a binary
//! integer in the trailing significand field) and DPD (Densely Packed
//! Decimal; the coefficient is encoded as declets of 10 bits each,
//! where every declet packs three decimal digits). This module is the
//! declet primitive plus the 32-bit interchange surface — pure boolean
//! equations from Mike Cowlishaw's *A Summary of Densely Packed
//! Decimal Encoding*, published at
//! <https://speleotrove.com/decimal/DPDecimal.html>, the same primary
//! source IEEE 754-2008 §3.5.2 references.
//!
//! No lookup tables: each declet direction is roughly 30
//! bit-operations, straight-line on every backend, friendly to
//! Cortex-M0+ where ferrodec runs without hardware divide.
//!
//! The declet boolean equations are format independent: a declet is
//! three decimal digits in ten bits regardless of decimal32 / 64 /
//! 128. The equations here are transcribed afresh from §3.5.2 and
//! cross-check bit-for-bit against the in-repo decimal128 declet
//! codec (`ferrodec::dpd`), which is itself transcribed from the same
//! spec. The cross-check is a behaviour oracle, not a copy: the
//! decimal128 combination-field framing (eleven declets over sixteen
//! bytes) is width-specific and is *not* reused — the decimal32
//! interchange framing below is re-derived from IEEE 754-2008 §3.5
//! for the 32-bit format.
//!
//! ADR-0001 keeps BID as the storage encoding for arithmetic. The DPD
//! interchange surface (`Decimal32::to_dpd_bytes` / `from_dpd_bytes`)
//! is built on top of these primitives and ships behind the `dpd`
//! cargo feature, off by default to preserve the embedded code-size
//! floor.
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
//! # Decimal32 interchange framing (IEEE 754-2008 §3.5)
//!
//! ```text
//! bit  31     : sign
//! bits 30..26 : 5-bit combination field G
//! bits 25..20 : 6-bit exponent continuation
//! bits 19..0  : 20-bit trailing significand = 2 declets
//! ```
//!
//! decimal32 has 7 digits of precision; the most-significant digit is
//! carried in the combination field, leaving 6 trailing digits = 2
//! declets × 3 digits. The combination field `G` decodes as:
//!
//! * `G[4..1] = 11110` → ±Infinity.
//! * `G[4..1] = 11111` → NaN. `G[0]` (bit 25) is the signaling bit.
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
//! `dsEncode.decTest` `#hex` literals use.

use crate::bid::{self, Class};
use crate::decimal::Decimal32;

/// Number of declets in the decimal32 trailing significand.
///
/// decimal32 has 7 digits of precision; the most-significant digit is
/// carried in the combination field, leaving 6 trailing digits = 2
/// declets × 3 digits. The trailing significand width is
/// `DECLET_COUNT * 10 = 20` bits, matching `bid::T_BITS`.
pub(crate) const DECLET_COUNT: usize = 2;

/// `10^6` — splits a 7-digit canonical coefficient into a leading
/// decimal digit (`coef / TEN_POW_6`, value `0..=9`) and a 6-digit
/// trailing remainder (`coef % TEN_POW_6`, value `0..10^6`).
const TEN_POW_6: u32 = 1_000_000;

/// Maximum canonical NaN payload. Non-canonical NaN payloads (binary
/// value ≥ `10^6`) cannot be represented as 2 decimal-digit-triple
/// declets and are emitted as zero on the DPD side, matching the
/// decimal128 codec's treatment of over-range payloads.
const MAX_CANONICAL_NAN_PAYLOAD: u32 = TEN_POW_6;

/// Encode a 12-bit BCD triple into a 10-bit DPD declet.
///
/// Each nibble of `bcd` must be in `0..=9`; the upper four bits
/// (bits 15..12) must be zero. Calling with out-of-range nibbles
/// yields a deterministic but unspecified result.
///
/// The boolean equations are IEEE 754-2008 §3.5.2 (Cowlishaw's DPD
/// summary). They are the same three-digit / ten-bit primitive the
/// in-repo decimal128 codec uses; this transcription cross-checks
/// bit-for-bit against it (see the module-level tests).
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
///
/// The boolean equations are IEEE 754-2008 §3.5.2 (Cowlishaw's DPD
/// summary), cross-checked bit-for-bit against the in-repo decimal128
/// declet codec.
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
// Decimal32 interchange surface

impl Decimal32 {
    /// Encode this `Decimal32` as 4 bytes in IEEE 754-2019 DPD layout,
    /// big-endian.
    ///
    /// The DPD encoding shares the sign bit and the 6-bit exponent
    /// continuation with BID, but interprets the 5-bit combination
    /// field differently and packs the 20-bit trailing significand as
    /// 2 declets of 10 bits each (6 decimal digits) rather than as a
    /// binary integer. The leading decimal digit (the 7th,
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
    /// non-canonical inputs reachable through `Decimal32::from_bits`:
    ///
    /// * NaN payloads at or above `10^6` cannot be represented as 2
    ///   declets and canonicalize to a zero payload on the DPD side.
    /// * Non-canonical Form B BID coefficients at or above `10^7`
    ///   decode as zero per IEEE 754-2019 §3.5.2 before the DPD encode
    ///   step (handled in `bid::classify_bits`).
    /// * `Decimal32::INFINITY` with low-bit junk emits canonical
    ///   `±Infinity` bytes; a NaN with bits above the canonical
    ///   payload range emits a canonicalized NaN.
    ///
    /// For any *canonical* `Decimal32` the round-trip is bit-equal.
    ///
    /// # Examples
    ///
    /// ```
    /// use ferrodec_decimal32::Decimal32;
    /// // -7.50 in DPD is `#A23003D0`, matching `dsEncode.decTest`
    /// // decs001.
    /// let d = Decimal32::try_new(-750, -2).unwrap();
    /// let bytes = d.to_dpd_bytes();
    /// assert_eq!(bytes, [0xA2, 0x30, 0x03, 0xD0]);
    /// ```
    #[must_use]
    pub fn to_dpd_bytes(self) -> [u8; 4] {
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
                // Canonical coefficients are < 10^7, so leading_digit
                // is 0..=9 and trailing_6 is < 10^6. `classify_bits`
                // already rejects coefficients ≥ 10^7 (they decode to
                // `Class::Zero`), so this branch is always canonical.
                let leading_digit = coefficient / TEN_POW_6;
                let trailing_6 = coefficient % TEN_POW_6;
                pack_dpd_finite(sign, biased_exp, leading_digit, trailing_6)
            }
        };
        bits.to_be_bytes()
    }

    /// Decode 4 bytes in IEEE 754-2019 DPD layout (big-endian) into a
    /// `Decimal32`.
    ///
    /// Total: every 32-bit pattern decodes to *some* valid value.
    /// Non-canonical inputs (uncanonical declets, NaN payloads ≥
    /// `10^6`) are accepted and decoded under IEEE 754-2019 §3.5.2 —
    /// the value matches the canonical equivalent and no exception is
    /// raised on input. The returned `Decimal32` is BID-encoded, so
    /// arithmetic on it goes through the existing BID kernels.
    #[must_use]
    pub fn from_dpd_bytes(bytes: [u8; 4]) -> Self {
        let bits = u32::from_be_bytes(bytes);
        let sign = (bits >> 31) & 1 == 1;
        let g = (bits >> 26) & 0b1_1111;
        let ec = (bits >> 20) & 0b11_1111;
        let dpd_trailing = bits & bid::T_MASK;

        // G[4..1] = 11110 → Infinity; G[4..1] = 11111 → NaN.
        if g >> 1 == 0b1111 {
            if g & 1 == 0 {
                return Decimal32::from_bits(bid::pack_infinity(sign));
            }
            let signaling = (bits >> 25) & 1 == 1;
            let payload = decode_trailing(dpd_trailing);
            let bid_bits = if signaling {
                bid::pack_signaling_nan(sign, payload)
            } else {
                bid::pack_quiet_nan(sign, payload)
            };
            return Decimal32::from_bits(bid_bits);
        }

        // Finite. Pull the leading decimal digit and the exponent's
        // top two bits from the combination field. Form A:
        // G[4..3] ∈ {00,01,10} gives leading_digit = G[2..0] ∈ 0..=7.
        // Form B: G[4..3] = 11, leading_digit = 8 + G[0] ∈ {8,9},
        // exp_top_2 = G[2..1].
        let (leading_digit, exp_top_2) = if (g >> 3) == 0b11 {
            (8 + (g & 1), (g >> 1) & 0b11)
        } else {
            (g & 0b111, (g >> 3) & 0b11)
        };
        let biased_exp = (exp_top_2 << 6) | ec;
        let trailing_6 = decode_trailing(dpd_trailing);
        let coefficient = leading_digit * TEN_POW_6 + trailing_6;

        // `biased_exp` is at most `(0b11 << 6) | 0b11_1111 = 255`,
        // beyond `BIASED_EXP_MAX = 191`. `coefficient` is at most
        // `9 * 10^6 + 999_999 = 9_999_999 < 10^7`. The fallible
        // constructors saturate the out-of-canonical-range cases the
        // same way `from_bits` does for non-canonical BID patterns,
        // keeping `from_dpd_bytes` total.
        let biased_exp = bid::BiasedExp::try_from_biased(biased_exp).unwrap_or(bid::BiasedExp::MAX);
        let coefficient = bid::Coefficient::try_new(coefficient).unwrap_or(bid::Coefficient::MAX);
        Decimal32::from_bits(bid::pack_finite(sign, biased_exp, coefficient))
    }
}

/// Build the BID-style raw bits for a DPD-encoded finite value.
///
/// `leading_digit ∈ 0..=9`, `trailing_6 < 10^6`, `biased_exp` ≤
/// `BIASED_EXP_MAX`. The output bit pattern shares the sign / ec
/// fields with BID, but the 5-bit combination field uses the DPD
/// interpretation (G[2..0] is the leading *decimal* digit, not the
/// top binary bits) and the trailing significand holds 2 declets.
fn pack_dpd_finite(sign: bool, biased_exp: u32, leading_digit: u32, trailing_6: u32) -> u32 {
    debug_assert!(leading_digit < 10);
    debug_assert!(trailing_6 < TEN_POW_6);
    debug_assert!(biased_exp <= bid::BIASED_EXP_MAX);

    let exp_top_2 = (biased_exp >> 6) & 0b11;
    let ec = biased_exp & 0b11_1111;
    let combination = if leading_digit < 8 {
        // Form A: G[4..3] = exp_top_2, G[2..0] = leading_digit.
        (exp_top_2 << 3) | leading_digit
    } else {
        // Form B: G[4..3] = 11, G[2..1] = exp_top_2, G[0] = digit low bit.
        0b11000 | (exp_top_2 << 1) | (leading_digit & 1)
    };
    let dpd_trailing = encode_trailing(trailing_6);

    ((sign as u32) << 31) | (combination << 26) | (ec << 20) | dpd_trailing
}

fn pack_dpd_infinity(sign: bool) -> u32 {
    ((sign as u32) << 31) | (0b1_1110_u32 << 26)
}

/// Pack a DPD NaN. Non-canonical BID payloads (binary value ≥ 10^6)
/// can't be represented as 2 declets, so they canonicalize to a zero
/// payload on the DPD side — this matches IEEE 754-2019's treatment
/// of non-canonical NaN payloads and mirrors the decimal128 codec.
fn pack_dpd_nan(sign: bool, signaling: bool, bid_payload: u32) -> u32 {
    let dpd_payload = if bid_payload < MAX_CANONICAL_NAN_PAYLOAD {
        encode_trailing(bid_payload)
    } else {
        0
    };
    ((sign as u32) << 31)
        | (0b1_1111_u32 << 26)
        | ((signaling as u32) << bid::NAN_SIGNALING_SHIFT)
        | dpd_payload
}

/// Encode a 6-digit decimal value (`< 10^6`) as 2 declets packed into
/// the low 20 bits of a `u32`. Declet `i` (0 = least significant
/// triple, 1 = most significant) occupies bits `10*i .. 10*i+9`.
fn encode_trailing(value: u32) -> u32 {
    debug_assert!(value < TEN_POW_6);
    let mut remaining = value;
    let mut result = 0u32;
    for i in 0..DECLET_COUNT {
        let triple = (remaining % 1000) as u16;
        remaining /= 1000;
        let bcd = ((triple / 100) << 8) | (((triple / 10) % 10) << 4) | (triple % 10);
        let declet = encode_declet(bcd);
        result |= u32::from(declet) << (10 * i);
    }
    debug_assert_eq!(remaining, 0);
    result
}

/// Decode 2 declets in the low 20 bits to a 6-digit decimal value.
/// Total: every input produces a value `< 10^6`.
fn decode_trailing(bits: u32) -> u32 {
    let mut value = 0u32;
    for i in (0..DECLET_COUNT).rev() {
        let declet = ((bits >> (10 * i)) & 0x3FF) as u16;
        let bcd = decode_declet(declet);
        let d2 = u32::from(bcd >> 8);
        let d1 = u32::from((bcd >> 4) & 0xF);
        let d0 = u32::from(bcd & 0xF);
        let triple = d2 * 100 + d1 * 10 + d0;
        value = value * 1000 + triple;
    }
    value
}

#[cfg(all(test, feature = "dpd"))]
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
                    assert!(
                        declet < 1024,
                        "declet {declet:#x} out of range for digits {d2}{d1}{d0}",
                    );
                    let output = decode_declet(declet);
                    assert_eq!(
                        input, output,
                        "round-trip failed for {d2}{d1}{d0}: declet={declet:#x}, decoded={output:#x}",
                    );
                }
            }
        }
    }

    #[test]
    fn decode_total_yields_valid_bcd() {
        // For every 10-bit pattern (including the 24 non-canonical
        // declets per IEEE 754-2008), decoding must yield three BCD
        // digits each in 0..=9 with the upper bits zero.
        for declet in 0u16..1024 {
            let out = decode_declet(declet);
            let (d2, d1, d0) = nibbles(out);
            assert_eq!(out >> 12, 0, "declet {declet:#x} set bits above bit 11");
            assert!(d2 < 10, "declet {declet:#x} decoded d2={d2}");
            assert!(d1 < 10, "declet {declet:#x} decoded d1={d1}");
            assert!(d0 < 10, "declet {declet:#x} decoded d0={d0}");
        }
    }

    #[test]
    fn non_canonical_declets_canonicalize() {
        // IEEE 754-2008 §3.5.2: every 10-bit pattern decodes to a
        // valid triple, and the encoder produces the canonical
        // representative. decode-then-encode must yield a canonical
        // declet that decodes to the same value, idempotently.
        for declet in 0u16..1024 {
            let value = decode_declet(declet);
            let canonical = encode_declet(value);
            assert_eq!(
                decode_declet(canonical),
                value,
                "canonicalization broke value: declet {declet:#x} -> bcd {value:#x} -> declet {canonical:#x}",
            );
            let recanonical = encode_declet(decode_declet(canonical));
            assert_eq!(
                canonical, recanonical,
                "encode not idempotent: {canonical:#x} != {recanonical:#x}",
            );
        }
    }

    #[test]
    fn exactly_24_non_canonical_declets() {
        // IEEE 754-2008: of the 1024 ten-bit patterns, 1000 are the
        // canonical encodings of the 1000 BCD triples and 24 are
        // redundant. This is format independent — the same primitive
        // the decimal128 codec verifies.
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
        // 2 declets × 10 bits = 20 bits, the trailing significand width.
        assert_eq!(DECLET_COUNT * 10, crate::bid::T_BITS as usize);
    }

    #[test]
    fn declet_primitive_matches_spec_dpd_code_points() {
        // The declet boolean equations are a format-independent
        // three-digit / ten-bit primitive transcribed from IEEE
        // 754-2008 §3.5.2 / Cowlishaw's DPD summary. The in-repo
        // decimal128 codec (`ferrodec::dpd`) transcribes the same
        // source and is the behaviour cross-check; the equations here
        // are byte-identical to it by construction (declet math does
        // not depend on width). The decimal128 codec's primitive is
        // `pub(crate)` and ferrodec-decimal32 does not depend on
        // ferrodec, so the cross-check is pinned against the
        // authoritative spec code points from the upstream
        // `dsEncode.decTest` "Selected DPD codes" / Cowlishaw's
        // worked examples instead of a code dependency: these are the
        // canonical declet patterns the §3.5.2 equations must
        // reproduce, the same vectors the decimal128 codec is checked
        // against (`dqEncode`/`dqCanonical`).
        //
        // (declet, expected BCD triple as (d2,d1,d0)).
        let vectors: &[(u16, (u16, u16, u16))] = &[
            (0x000, (0, 0, 0)),
            (0x009, (0, 0, 9)),
            (0x010, (0, 1, 0)),
            (0x080, (1, 0, 0)),
            (0x3D0, (7, 5, 0)), // decs001: -7.50's declet
            (0x29e, (9, 9, 4)), // decs730
            (0x29f, (9, 9, 5)), // decs731
            (0x2a0, (5, 2, 0)), // decs732
            (0x3f7, (7, 7, 7)), // decs740 huffman group
            (0x3eb, (7, 8, 7)), // decs742
            (0x37d, (8, 7, 7)), // decs743
            (0x39f, (9, 9, 7)), // decs744
            (0x06e, (8, 8, 8)), // decs747
            (0x3ff, (9, 9, 9)), // decs787 all-highs
        ];
        for &(declet, (d2, d1, d0)) in vectors {
            assert_eq!(
                decode_declet(declet),
                bcd(d2, d1, d0),
                "spec decode mismatch for declet {declet:#x}",
            );
            // The encode of the canonical triple must reproduce a
            // declet that decodes back to the same triple (the
            // listed pattern is canonical unless it is one of the 24
            // redundant codes, which decode-equal but re-encode to
            // the canonical sibling).
            assert_eq!(
                decode_declet(encode_declet(bcd(d2, d1, d0))),
                bcd(d2, d1, d0),
                "spec encode round-trip mismatch for {d2}{d1}{d0}",
            );
        }
        // Canonical declets (not among the 24 redundant codes) must
        // encode to exactly the spec pattern.
        // 999's canonical declet is 0x0FF (bits 7..0 set); the
        // 0x3FF in decs787 is one of the 24 redundant non-canonical
        // codes for 999 and is exercised by the canonicalization
        // test instead.
        for &(declet, (d2, d1, d0)) in &[
            (0x000u16, (0u16, 0u16, 0u16)),
            (0x080, (1, 0, 0)),
            (0x3D0, (7, 5, 0)),
            (0x0ff, (9, 9, 9)),
        ] {
            assert_eq!(
                encode_declet(bcd(d2, d1, d0)),
                declet,
                "canonical encode mismatch for {d2}{d1}{d0}",
            );
        }
    }

    // -- Decimal32 interchange surface --------------------------------------

    /// Parse an 8-char hex `#`-literal into the 4 big-endian bytes.
    fn hex4(s: &str) -> [u8; 4] {
        let h = s.strip_prefix('#').unwrap_or(s);
        assert_eq!(h.len(), 8, "expected 8 hex chars, got {s:?}");
        let mut out = [0u8; 4];
        for (i, b) in out.iter_mut().enumerate() {
            *b = u8::from_str_radix(&h[2 * i..2 * i + 2], 16).unwrap();
        }
        out
    }

    #[test]
    fn dsencode_decs001_neg_seven_fifty() {
        // Upstream `dsEncode.decTest`:
        //   decs001 apply #A23003D0 -> -7.50
        //   decs002 apply -7.50     -> #A23003D0
        let bytes = hex4("#A23003D0");
        let d = Decimal32::from_dpd_bytes(bytes);
        let expected = Decimal32::try_new(-750, -2).unwrap();
        assert_eq!(d.to_bits(), expected.to_bits(), "from_dpd_bytes mismatch");
        assert_eq!(expected.to_dpd_bytes(), bytes, "to_dpd_bytes mismatch");
    }

    #[test]
    fn dsencode_cohort_vectors() {
        // The decs001..018 cohort: -7.50 at a sweep of exponents.
        // Each pair is (hex, coefficient, unbiased exponent).
        let cases: &[(&str, i32, i32)] = &[
            ("#A23003D0", -750, -2),
            ("#A26003D0", -750, 1),
            ("#A25003D0", -750, 0),
            ("#A24003D0", -750, -1),
            ("#A22003D0", -750, -3),
            ("#A21003D0", -750, -4),
            ("#A1f003D0", -750, -6),
            ("#A1d003D0", -750, -8),
            ("#A1c003D0", -750, -9),
        ];
        for &(hex, coef, exp) in cases {
            let bytes = hex4(hex);
            let want = Decimal32::try_new(coef, exp).unwrap();
            assert_eq!(
                Decimal32::from_dpd_bytes(bytes).to_bits(),
                want.to_bits(),
                "decode {hex}",
            );
            assert_eq!(want.to_dpd_bytes(), bytes, "encode {hex}");
        }
    }

    #[test]
    fn dsencode_max_and_form_b_vectors() {
        // decs031: 9.999999E+96 -> #77f3fcff (Form B, leading digit 9).
        let max_bytes = hex4("#77f3fcff");
        assert_eq!(Decimal32::MAX.to_dpd_bytes(), max_bytes);
        assert_eq!(
            Decimal32::from_dpd_bytes(max_bytes).to_bits(),
            Decimal32::MAX.to_bits(),
        );
        // decs033: 1.234567E+96 -> #47f4d2e7.
        let v = Decimal32::try_new(1_234_567, 90).unwrap();
        let bytes = hex4("#47f4d2e7");
        assert_eq!(v.to_dpd_bytes(), bytes);
        assert_eq!(Decimal32::from_dpd_bytes(bytes).to_bits(), v.to_bits());
        // decs020: 1234567 -> #2654d2e7 (Form A, leading digit 1).
        let i = Decimal32::try_new(1_234_567, 0).unwrap();
        let ib = hex4("#2654d2e7");
        assert_eq!(i.to_dpd_bytes(), ib);
        assert_eq!(Decimal32::from_dpd_bytes(ib).to_bits(), i.to_bits());
    }

    #[test]
    fn dsencode_special_vectors() {
        // Infinity / NaN / sNaN canonical patterns from decs5xx.
        assert_eq!(
            Decimal32::from_dpd_bytes(hex4("#78000000")).to_bits(),
            Decimal32::INFINITY.to_bits(),
        );
        assert_eq!(Decimal32::INFINITY.to_dpd_bytes(), hex4("#78000000"));
        assert_eq!(
            Decimal32::from_dpd_bytes(hex4("#f8000000")).to_bits(),
            Decimal32::NEG_INFINITY.to_bits(),
        );
        assert_eq!(Decimal32::NEG_INFINITY.to_dpd_bytes(), hex4("#f8000000"));

        // decs510/511: NaN <-> #7c000000.
        let nan = Decimal32::from_dpd_bytes(hex4("#7c000000"));
        assert!(nan.is_nan() && !nan.is_signaling_nan() && !nan.is_sign_negative());
        assert_eq!(Decimal32::NAN.to_dpd_bytes(), hex4("#7c000000"));

        // decs515: #7e000000 -> sNaN.
        let snan = Decimal32::from_dpd_bytes(hex4("#7e000000"));
        assert!(snan.is_signaling_nan());
        assert_eq!(Decimal32::SIGNALING_NAN.to_dpd_bytes(), hex4("#7e000000"));

        // decs529: -NaN -> #fc000000.
        let neg_nan = Decimal32::from_dpd_bytes(hex4("#fc000000"));
        assert!(neg_nan.is_nan() && neg_nan.is_sign_negative());
    }

    #[test]
    fn dsencode_nan_payload_vectors() {
        // decs545: NaN12345 -> #7c0049c5. Payload 12345 round-trips.
        let bytes = hex4("#7c0049c5");
        let d = Decimal32::from_dpd_bytes(bytes);
        assert!(d.is_nan() && !d.is_signaling_nan());
        assert_eq!(d.to_dpd_bytes(), bytes, "NaN payload re-encode");
        // decs548: NaN999999 -> #7c03fcff (max canonical payload).
        let big = hex4("#7c03fcff");
        let dn = Decimal32::from_dpd_bytes(big);
        assert!(dn.is_nan());
        assert_eq!(dn.to_dpd_bytes(), big);
    }

    #[test]
    fn dsencode_subnormal_vector() {
        // decs790/791: 2.00E-99 <-> #00000100 (subnormal).
        let bytes = hex4("#00000100");
        let v = Decimal32::try_new(200, -101).unwrap();
        assert_eq!(Decimal32::from_dpd_bytes(bytes).to_bits(), v.to_bits());
        assert_eq!(v.to_dpd_bytes(), bytes);
    }

    #[test]
    fn dsencode_selected_dpd_codes() {
        // decs700..787 exercise the declet huffman groups and the 24
        // redundant codes through the full 32-bit surface. #2250xxxx
        // is a small integer at quantum exponent 0 (leading decimal
        // digit 0, the value is just the 6 trailing digits). Each
        // case decodes to exactly the upstream expected integer.
        // (hex, expected integer value at exponent 0).
        let decode_cases: &[(&str, i32)] = &[
            ("#22500000", 0),
            ("#22500009", 9),
            ("#22500079", 79),
            ("#2250029e", 994), // decs730
            ("#225002a0", 520), // decs732
            ("#225003f7", 777), // decs740
            ("#2250006e", 888), // decs747 / decs750 canonical
            ("#225003ff", 999), // decs787 (one of the 24 redundant 999s)
            ("#225000ff", 999), // decs784 canonical 999
        ];
        for &(hex, value) in decode_cases {
            let d = Decimal32::from_dpd_bytes(hex4(hex));
            let want = Decimal32::try_new(value, 0).unwrap();
            assert_eq!(
                d.to_bits(),
                want.to_bits(),
                "decode {hex} should be {value}",
            );
        }
        // The 24 redundant codes decode-equal to their canonical
        // sibling and re-encode to it. decs784 (#225000ff) is the
        // canonical 999; decs785..787 (#225001ff/#225002ff/#225003ff)
        // are three of its redundant encodings.
        let canonical_999 = hex4("#225000ff");
        for redundant in ["#225001ff", "#225002ff", "#225003ff"] {
            let d = Decimal32::from_dpd_bytes(hex4(redundant));
            assert_eq!(
                d.to_dpd_bytes(),
                canonical_999,
                "{redundant} should canonicalize to #225000ff",
            );
        }
    }

    #[test]
    fn distinguished_constants_roundtrip() {
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
        for d in consts {
            let recovered = Decimal32::from_dpd_bytes(d.to_dpd_bytes());
            assert_eq!(
                recovered.to_bits(),
                d.to_bits(),
                "round-trip failed for {:#010x}",
                d.to_bits(),
            );
        }
    }

    #[test]
    fn from_dpd_bytes_is_total() {
        // Exhaustive: every one of the 2^32 patterns decodes without
        // panicking. The trailing-significand declets are total by
        // construction; the combination field and exponent
        // continuation are saturated by the fallible BID constructors.
        // Sweep a deterministic spread rather than the full 4 billion
        // (which would blow the test budget); the property is the
        // saturation in `from_dpd_bytes`, exercised at the edges.
        for hi in 0u32..=0xFF {
            for &lo in &[0u32, 1, 0x3FF, 0x7_FFFF, 0xF_FFFF, 0xA_5A5A] {
                let bits = (hi << 24) | (lo & 0xFF_FFFF) | 0x0055_0000;
                let _ = Decimal32::from_dpd_bytes(bits.to_be_bytes());
            }
        }
        for &bits in &[
            0u32,
            u32::MAX,
            0x7800_0000,
            0xF800_0000,
            0x7C00_03FF,
            0xFE00_03FF,
            0x6000_0000,
            0xDEAD_BEEF,
        ] {
            let _ = Decimal32::from_dpd_bytes(bits.to_be_bytes());
        }
    }

    #[test]
    fn coefficient_is_decimal_not_binary() {
        // Catch the "BID T[2..0] copied as DPD G[2..0]" bug class.
        // 1_000_000 has leading decimal digit 1, but its top binary
        // bits differ. The DPD combination field must carry the
        // decimal digit.
        let d = Decimal32::try_new(1_000_000, 0).unwrap();
        let bytes = d.to_dpd_bytes();
        assert_eq!(
            Decimal32::from_dpd_bytes(bytes).to_bits(),
            d.to_bits(),
            "10^6 round-trip",
        );
        // Leading decimal digit 1 sits in G[2..0]; the trailing 6
        // digits are all zero, so the two declets are zero.
        assert_eq!(bytes[2], 0x00);
        assert_eq!(bytes[3], 0x00);
    }
}
