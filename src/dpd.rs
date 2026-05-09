// The encode/decode equations are transcribed verbatim from Cowlishaw
// §3.5.2 so the codec can be audited line-by-line against the spec.
// Algebraic minimization (clippy's preferred form) breaks that
// correspondence — keep the literal form.
#![allow(clippy::nonminimal_bool)]

//! Densely Packed Decimal (DPD) declet codec for IEEE 754 decimal128
//! interchange.
//!
//! IEEE 754 defines two equivalent storage encodings for decimal128:
//! BID (the canonical ferrodec storage; the coefficient is a binary
//! integer in the trailing significand field) and DPD (Densely Packed
//! Decimal; the coefficient is encoded as 11 declets of 10 bits each,
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
//! interchange surface (`Decimal128::to_dpd_bytes` /
//! `from_dpd_bytes`) is built on top of these primitives in
//! `crate::decimal` and ships behind the `dpd` cargo feature.
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

/// Number of declets in the decimal128 trailing significand.
///
/// decimal128 has 34 digits of precision; the most-significant digit
/// is carried in the combination field, leaving 33 trailing digits =
/// 11 declets × 3 digits. The trailing significand width is
/// `DECLET_COUNT * 10 = 110` bits, matching `bid::T_BITS`.
pub(crate) const DECLET_COUNT: usize = 11;

/// Encode a 12-bit BCD triple into a 10-bit DPD declet.
///
/// Each nibble of `bcd` must be in `0..=9`; the upper four bits
/// (bits 15..12) must be zero. Calling with out-of-range nibbles
/// yields a deterministic but unspecified result.
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
// Decimal128 surface (Phase 1)

use crate::bid::{self, Class};
use crate::decimal::Decimal128;

/// 10^33 — splits a 34-digit canonical coefficient into a leading
/// decimal digit (`coef / TEN_POW_33`, value `0..=9`) and a 33-digit
/// trailing remainder (`coef % TEN_POW_33`, value `0..10^33`).
const TEN_POW_33: u128 = 10u128.pow(33);

/// Maximum canonical NaN payload — same threshold as
/// `classify::MAX_CANONICAL_NAN_PAYLOAD`. Non-canonical NaN payloads
/// (binary value ≥ `10^33`) cannot be represented as 11 decimal-digit
/// declets and are emitted as zero on the DPD side.
const MAX_CANONICAL_NAN_PAYLOAD: u128 = TEN_POW_33;

impl Decimal128 {
    /// Encode this `Decimal128` as 16 bytes in IEEE 754-2019 DPD layout,
    /// big-endian.
    ///
    /// The DPD encoding shares the sign bit, the 5-bit combination field,
    /// and the 12-bit exponent continuation with BID. The 110-bit
    /// trailing significand differs: BID stores it as a binary integer,
    /// DPD as 11 declets of 10 bits each, holding 33 decimal digits.
    /// The leading decimal digit (the 34th, most-significant digit of
    /// the coefficient) is carried in the combination field in both
    /// encodings, but BID stores its top 3 bits as a *binary* prefix of
    /// the coefficient while DPD stores it as the leading *decimal*
    /// digit value.
    ///
    /// Storage encoding for arithmetic stays BID; this is a byte-level
    /// interchange adapter only. See ADR-0001 (BID storage choice) and
    /// ADR-0009 (DPD interchange).
    ///
    /// **Round-trip contract.** The pair `from_dpd_bytes(to_dpd_bytes(x))`
    /// is bit-equal to `x.canonicalize()`, not necessarily to `x`. The
    /// two diverge for non-canonical inputs reachable through
    /// `Decimal128::from_bits`:
    ///
    /// * NaN payloads at or above `10^33` cannot be represented as
    ///   11 declets and canonicalize to a zero payload on the DPD
    ///   side.
    /// * Form A coefficients at or above `10^34` decode as zero per
    ///   IEEE 754-2019 §3.5.2 before the DPD encode step.
    /// * `Decimal128::INFINITY` with low-bit junk emits canonical
    ///   `±Infinity` bytes; `Decimal128::NAN` with bits 120..110 set
    ///   emits canonical NaN bytes.
    ///
    /// For any *canonical* `Decimal128` the round-trip is bit-equal.
    /// `tests/property_from_bits.rs::dpd_roundtrip_via_canonical`
    /// pins the projection property over arbitrary 128-bit inputs.
    ///
    /// # Examples
    ///
    /// ```
    /// use ferrodec::Decimal128;
    /// // -7.50 in DPD is `#A20780000000000000000000000003D0`,
    /// // matching `dqEncode.decTest` decq001.
    /// let d = Decimal128::try_new(-750, -2).unwrap();
    /// let bytes = d.to_dpd_bytes();
    /// assert_eq!(bytes[0], 0xA2);
    /// assert_eq!(bytes[15], 0xD0);
    /// ```
    #[must_use]
    pub fn to_dpd_bytes(self) -> [u8; 16] {
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
                // Canonical coefficients are < 10^34, so leading_digit
                // is 0..=9 and trailing_33 is < 10^33. `classify_bits`
                // already rejects coefficients ≥ 10^34 (they decode to
                // `Class::Zero`), so this branch is always canonical.
                let leading_digit = (coefficient / TEN_POW_33) as u32;
                let trailing_33 = coefficient % TEN_POW_33;
                pack_dpd_finite(sign, biased_exp, leading_digit, trailing_33)
            }
        };
        bits.to_be_bytes()
    }

    /// Decode 16 bytes in IEEE 754-2019 DPD layout (big-endian) into a
    /// `Decimal128`.
    ///
    /// Total: every 128-bit pattern decodes to *some* valid value.
    /// Non-canonical inputs (uncanonical declets, NaN payloads ≥
    /// `10^33`) are accepted and decoded under IEEE 754-2019 §3.5.2 —
    /// the value matches the canonical equivalent and no exception is
    /// raised on input. The returned `Decimal128` is BID-encoded, so
    /// arithmetic on it goes through the existing BID kernels.
    #[must_use]
    pub fn from_dpd_bytes(bytes: [u8; 16]) -> Self {
        let bits = u128::from_be_bytes(bytes);
        let sign = (bits >> 127) & 1 == 1;
        let t = ((bits >> 122) & 0b1_1111) as u32;
        let ec = ((bits >> 110) & 0xFFF) as u32;
        let dpd_trailing = bits & bid::T_MASK;

        if t == 0b1_1110 {
            return Decimal128::from_bits(bid::pack_infinity(sign));
        }
        if t == 0b1_1111 {
            let signaling = (bits >> 121) & 1 == 1;
            let payload = decode_trailing(dpd_trailing);
            let bid_bits = if signaling {
                bid::pack_signaling_nan(sign, payload)
            } else {
                bid::pack_quiet_nan(sign, payload)
            };
            return Decimal128::from_bits(bid_bits);
        }

        // Finite. Pull the leading decimal digit and exp top 2 bits
        // from the combination field. Form A: G[4:3] ∈ {00,01,10}
        // gives leading_digit = G[2:0] ∈ 0..=7. Form B: G[4:3] = 11,
        // G[2] = 0, leading_digit = 8 + G[0] ∈ {8,9}, exp_top_2 =
        // G[2:1].
        let (leading_digit, exp_top_2) = if (t >> 3) == 0b11 {
            // Form B (t & 0b11000 == 0b11000, and we already excluded
            // 0b1_1110 / 0b1_1111 above).
            (u128::from(8 + (t & 1)), (t >> 1) & 0b11)
        } else {
            // Form A.
            (u128::from(t & 0b111), (t >> 3) & 0b11)
        };
        let biased_exp = (exp_top_2 << 12) | ec;
        let trailing_33 = decode_trailing(dpd_trailing);
        let coefficient = leading_digit * TEN_POW_33 + trailing_33;

        Decimal128::from_bits(bid::pack_finite(sign, biased_exp, coefficient))
    }
}

/// Build the BID-style raw bits for a DPD-encoded finite value.
///
/// `leading_digit ∈ 0..=9`, `trailing_33 < 10^33`, `biased_exp` ≤
/// `BIASED_EXP_MAX`. The output bit pattern is laid out with the same
/// sign / combination / ec / trailing-significand fields as BID, but
/// the combination field uses the DPD interpretation (T[2:0] is the
/// leading *decimal* digit, not the top three binary bits) and the
/// trailing significand holds 11 declets.
fn pack_dpd_finite(sign: bool, biased_exp: u32, leading_digit: u32, trailing_33: u128) -> u128 {
    debug_assert!(leading_digit < 10);
    debug_assert!(trailing_33 < TEN_POW_33);
    debug_assert!(biased_exp <= bid::BIASED_EXP_MAX);

    let exp_top_2 = (biased_exp >> 12) & 0b11;
    let ec = biased_exp & 0xFFF;
    let combination = if leading_digit < 8 {
        // Form A: T[4:3] = exp_top_2, T[2:0] = leading_digit.
        (exp_top_2 << 3) | leading_digit
    } else {
        // Form B: T[4:3] = 11, T[2:1] = exp_top_2, T[0] = digit_low_bit.
        0b11000 | (exp_top_2 << 1) | (leading_digit & 1)
    };
    let dpd_trailing = encode_trailing(trailing_33);

    (u128::from(sign) << 127)
        | (u128::from(combination) << 122)
        | (u128::from(ec) << 110)
        | dpd_trailing
}

fn pack_dpd_infinity(sign: bool) -> u128 {
    (u128::from(sign) << 127) | (0b1_1110_u128 << 122)
}

/// Pack a DPD NaN. Non-canonical BID payloads (binary value ≥ 10^33)
/// can't be represented as 11 declets, so they canonicalize to a
/// zero payload on the DPD side — this matches IEEE 754-2019's
/// treatment of non-canonical NaN payloads on read (canonicalize on
/// emission rather than on consumption is the same effect at
/// round-trip granularity for the canonical-input case).
fn pack_dpd_nan(sign: bool, signaling: bool, bid_payload: u128) -> u128 {
    let dpd_payload = if bid_payload < MAX_CANONICAL_NAN_PAYLOAD {
        encode_trailing(bid_payload)
    } else {
        0
    };
    (u128::from(sign) << 127)
        | (0b1_1111_u128 << 122)
        | (u128::from(signaling) << bid::NAN_SIGNALING_SHIFT)
        | dpd_payload
}

/// Encode a 33-digit decimal value (`< 10^33`) as 11 declets packed
/// into the low 110 bits of a `u128`. Declet `i` (0 = least
/// significant triple, 10 = most significant) occupies bits
/// `10*i .. 10*i+9`.
fn encode_trailing(value: u128) -> u128 {
    debug_assert!(value < TEN_POW_33);
    let mut remaining = value;
    let mut result: u128 = 0;
    for i in 0..DECLET_COUNT {
        let triple = (remaining % 1000) as u16;
        remaining /= 1000;
        let bcd = ((triple / 100) << 8) | (((triple / 10) % 10) << 4) | (triple % 10);
        let declet = encode_declet(bcd);
        result |= u128::from(declet) << (10 * i);
    }
    debug_assert_eq!(remaining, 0);
    result
}

/// Decode 11 declets in the low 110 bits to a 33-digit decimal value.
/// Total: every input produces a value `< 10^33`.
fn decode_trailing(bits: u128) -> u128 {
    let mut value: u128 = 0;
    for i in (0..DECLET_COUNT).rev() {
        let declet = ((bits >> (10 * i)) & 0x3FF) as u16;
        let bcd = decode_declet(declet);
        let d2 = u128::from(bcd >> 8);
        let d1 = u128::from((bcd >> 4) & 0xF);
        let d0 = u128::from(bcd & 0xF);
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
    fn known_mappings() {
        // Identity for digits 0..=9 in the low position (these are
        // the all-small triples 000..009): DPD = BCD = digit value.
        for d in 0..10u16 {
            assert_eq!(encode_declet(bcd(0, 0, d)), d);
            assert_eq!(decode_declet(d), bcd(0, 0, d));
        }

        // Pure-mid digit: 010..090 encode to declets 16..144 (step
        // 16; bit 4 is `u = h`, the mid-digit LSB) for d in 0..=7,
        // and use the large-digit encoding above that. Spot-check
        // the small cases.
        for d in 0..8u16 {
            assert_eq!(encode_declet(bcd(0, d, 0)), d << 4);
        }

        // BCD 100 → declet 0x080 (bit 7 = high digit's LSB, all else 0).
        assert_eq!(encode_declet(bcd(1, 0, 0)), 0x080);
        assert_eq!(decode_declet(0x080), bcd(1, 0, 0));

        // BCD 750 → declet 0x3D0. Verified against
        // `dqEncode.decTest` decq001 (`#A2078...3D0 -> -7.50`).
        assert_eq!(encode_declet(bcd(7, 5, 0)), 0x3D0);
        assert_eq!(decode_declet(0x3D0), bcd(7, 5, 0));

        // BCD 999 — all-large case. Multiple non-canonical encodings
        // exist; the canonical one has declet bits v=w=x=1 (i.e.
        // bits 3, 2, 1 set in the output), with the high seven bits
        // carrying the digits' low bits. Round-trip is the
        // definitive check; exact bit pattern is per the equations.
        let nine_nine_nine = encode_declet(bcd(9, 9, 9));
        assert_eq!(decode_declet(nine_nine_nine), bcd(9, 9, 9));

        // BCD 089 — single-large at the low position.
        let zero_eight_nine = encode_declet(bcd(0, 8, 9));
        assert_eq!(decode_declet(zero_eight_nine), bcd(0, 8, 9));

        // BCD 880 — two-large at high and mid positions.
        let eight_eight_zero = encode_declet(bcd(8, 8, 0));
        assert_eq!(decode_declet(eight_eight_zero), bcd(8, 8, 0));
    }

    #[test]
    fn non_canonical_declets_canonicalize() {
        // IEEE 754-2008 §3.5.2: every 10-bit pattern decodes to a
        // valid triple, and the encoder produces the canonical
        // representative. So for every declet, decode-then-encode
        // yields a (possibly different) canonical declet that
        // decodes to the same value, and the canonical form is
        // idempotent under further decode/encode.
        for declet in 0u16..1024 {
            let value = decode_declet(declet);
            let canonical = encode_declet(value);
            assert_eq!(
                decode_declet(canonical),
                value,
                "canonicalization broke value: declet {declet:#x} -> bcd {value:#x} -> declet {canonical:#x}",
            );
            // Idempotence: re-canonicalizing the canonical form is a no-op.
            let recanonical = encode_declet(decode_declet(canonical));
            assert_eq!(
                canonical, recanonical,
                "encode not idempotent: {canonical:#x} != {recanonical:#x}",
            );
        }
    }

    #[test]
    fn non_canonical_declets_match_dq_canonical_pattern() {
        // dqCanonical.decTest comment: "Uncanonical declets are abc,
        // where a∈{1,2,3}, b∈{6,7,e,f}, c∈{e,f}". Those are the 24
        // non-canonical 10-bit patterns when grouped as a 12-bit
        // zero-extended value (the leading nibble is always zero, so
        // 'abc' here means the low 12 bits, with each letter a 4-bit
        // hex digit). In our 10-bit declet space this filters down
        // to specific patterns; we identify them by checking which
        // declets are not their own canonical form.
        let mut non_canonical_count = 0;
        for declet in 0u16..1024 {
            let canonical = encode_declet(decode_declet(declet));
            if canonical != declet {
                non_canonical_count += 1;
            }
        }
        // IEEE 754-2008: there are exactly 24 non-canonical 10-bit
        // patterns (from the 1024 patterns, 1000 are canonical for
        // the 1000 BCD triples + 24 redundant encodings).
        assert_eq!(
            non_canonical_count, 24,
            "expected 24 non-canonical declets, found {non_canonical_count}",
        );
    }

    #[test]
    fn declet_count_matches_t_bits() {
        // Sanity: 11 declets × 10 bits = 110 bits, the trailing
        // significand width.
        assert_eq!(DECLET_COUNT * 10, crate::bid::T_BITS as usize);
    }

    // -- Decimal128 surface tests (Phase 1) ----------------------------------

    #[test]
    fn dqencode_decq001_vector() {
        // Upstream `dqEncode.decTest`:
        //   decq001 apply #A20780000000000000000000000003D0 -> -7.50
        // Both directions must match.
        let bytes = [
            0xA2, 0x07, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x03, 0xD0,
        ];
        let d = Decimal128::from_dpd_bytes(bytes);
        let expected = Decimal128::try_new(-750, -2).unwrap();
        assert_eq!(d.to_bits(), expected.to_bits(), "from_dpd_bytes mismatch");
        assert_eq!(expected.to_dpd_bytes(), bytes, "to_dpd_bytes mismatch");
    }

    #[test]
    fn dqencode_form_b_max_vector() {
        // Upstream `dqCanonical.decTest`:
        //   dqcan001 apply 9.999999999999999999999999999999999E+6144
        //                              -> #77ffcff3fcff3fcff3fcff3fcff3fcff
        // Form B: leading digit 9, exp top 2 bits = 10, declets all 0x0FF (BCD 999).
        let bytes = [
            0x77, 0xFF, 0xCF, 0xF3, 0xFC, 0xFF, 0x3F, 0xCF, 0xF3, 0xFC, 0xFF, 0x3F, 0xCF, 0xF3,
            0xFC, 0xFF,
        ];
        let d = Decimal128::from_dpd_bytes(bytes);
        let expected = Decimal128::MAX;
        assert_eq!(
            d.to_bits(),
            expected.to_bits(),
            "Form B max round-trip from DPD"
        );
        assert_eq!(
            expected.to_dpd_bytes(),
            bytes,
            "Form B max round-trip to DPD"
        );
    }

    #[test]
    fn distinguished_constants_roundtrip() {
        // Every distinguished constant in `decimal.rs` must round-trip
        // BID → DPD → BID bit-equal.
        let consts = [
            Decimal128::ZERO,
            Decimal128::NEG_ZERO,
            Decimal128::ONE,
            Decimal128::NEG_ONE,
            Decimal128::TEN,
            Decimal128::MAX,
            Decimal128::MIN,
            Decimal128::MIN_POSITIVE,
            Decimal128::MIN_POSITIVE_NORMAL,
            Decimal128::NAN,
            Decimal128::SIGNALING_NAN,
            Decimal128::INFINITY,
            Decimal128::NEG_INFINITY,
        ];
        for d in consts {
            let bytes = d.to_dpd_bytes();
            let recovered = Decimal128::from_dpd_bytes(bytes);
            assert_eq!(
                recovered.to_bits(),
                d.to_bits(),
                "round-trip failed for {d:?}",
            );
        }
    }

    #[test]
    fn coefficient_boundary_values() {
        // Boundary cases that exercise specific code paths in
        // `pack_dpd_finite`: leading digit = 7 (Form A boundary),
        // leading digit = 8 (Form B start), leading digit = 9 (Form
        // B end), and powers of 10 across the 10^33 split.
        let cases: &[(i128, i32)] = &[
            (7_999_999_999_999_999_999_999_999_999_999_999, 0),
            (8_000_000_000_000_000_000_000_000_000_000_000, 0),
            (8_999_999_999_999_999_999_999_999_999_999_999, 0),
            (9_000_000_000_000_000_000_000_000_000_000_000, 0),
            (9_999_999_999_999_999_999_999_999_999_999_999, 0),
            (1_000_000_000_000_000_000_000_000_000_000_000, 0), // 10^33
            (i128::from(u32::MAX), 0),
            (1, 6111),  // emax exponent
            (1, -6176), // most negative exponent
        ];
        for &(coef, exp) in cases {
            let d = Decimal128::try_new(coef, exp).unwrap();
            let recovered = Decimal128::from_dpd_bytes(d.to_dpd_bytes());
            assert_eq!(
                recovered.to_bits(),
                d.to_bits(),
                "round-trip failed for ({coef}, {exp})",
            );
        }
    }

    #[test]
    fn nan_with_payload_roundtrip() {
        // Canonical NaN payloads (< 10^33) must round-trip through
        // DPD bit-equal — the codec handles the binary↔BCD payload
        // shape just like a finite trailing significand.
        let payloads = [0u128, 1, 999, 1_000_000_000, TEN_POW_33 - 1];
        for &p in &payloads {
            let bid_bits = bid::pack_quiet_nan(false, p);
            let d = Decimal128::from_bits(bid_bits);
            let recovered = Decimal128::from_dpd_bytes(d.to_dpd_bytes());
            assert_eq!(
                recovered.to_bits(),
                d.to_bits(),
                "NaN payload {p} round-trip failed",
            );
        }
    }

    #[test]
    fn nan_with_non_canonical_payload_round_trips_via_canonicalize() {
        // The M11 finding: NaN payloads ≥ 10^33 cannot be represented
        // as 11 declets (the BCD encoding has a hard 33-digit ceiling).
        // The contract therefore is "round-trip equals canonicalize",
        // not "round-trip equals input". Pin it for both NaN flavours
        // and both signs across the boundary, so a future refactor of
        // either pack_dpd_nan or canonicalize cannot diverge silently.
        let non_canonical = [
            TEN_POW_33,         // exactly the boundary
            TEN_POW_33 + 1,
            (1u128 << 110) - 1, // largest payload representable in BID
        ];
        for &p in &non_canonical {
            for sign in [false, true] {
                for signaling in [false, true] {
                    let bid_bits = if signaling {
                        bid::pack_signaling_nan(sign, p)
                    } else {
                        bid::pack_quiet_nan(sign, p)
                    };
                    let d = Decimal128::from_bits(bid_bits);
                    let recovered = Decimal128::from_dpd_bytes(d.to_dpd_bytes());
                    let canonical = d.canonicalize();
                    assert_eq!(
                        recovered.to_bits(),
                        canonical.to_bits(),
                        "non-canonical NaN payload {p:#x} sign={sign} sig={signaling}: \
                         dpd round-trip should equal canonicalize",
                    );
                    // Sanity: recovered NaN and canonical NaN agree on
                    // sign and signaling flavour.
                    assert!(recovered.is_nan());
                    assert_eq!(recovered.is_signaling_nan(), canonical.is_signaling_nan());
                    assert_eq!(recovered.is_sign_negative(), sign);
                }
            }
        }
    }

    #[test]
    fn from_dpd_bytes_is_total() {
        // Spot-check that `from_dpd_bytes` never panics on any input,
        // including patterns with non-canonical declets, ec values
        // beyond the canonical range, and combination fields that
        // imply Form B. The exhaustive guarantee comes from the
        // property test in `tests/property_dpd.rs`; this is a
        // smoke test against ten arbitrary patterns.
        let patterns: &[u128] = &[
            0,
            u128::MAX,
            0x12345678_9ABCDEF0_12345678_9ABCDEF0,
            0xFFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFE,
            0xC0FF_EEFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF, // Form B
            0x7800_0000_0000_0000_0000_0000_0000_0000, // Inf
            0xFC00_0000_0000_0000_0000_0000_0000_03FF, // NaN with non-canonical declet
            0x0000_0000_0000_0000_0000_0000_0000_03FF,
            0x0001_0000_0000_0000_0000_0000_0000_0000,
            0xDEAD_BEEF_DEAD_BEEF_DEAD_BEEF_DEAD_BEEF,
        ];
        for &bits in patterns {
            let _ = Decimal128::from_dpd_bytes(bits.to_be_bytes());
        }
    }

    #[test]
    fn coefficient_is_decimal_not_binary() {
        // Catch the "BID T[2:0] copied as DPD T[2:0]" bug class:
        // these are different for any coefficient whose top binary
        // bits don't equal its leading decimal digit. 10^33 is the
        // simplest case — leading decimal digit is 1, but the
        // 113-bit binary number's top 3 bits are 0.
        let d = Decimal128::try_new(10i128.pow(33), 0).unwrap();
        let bytes = d.to_dpd_bytes();
        // Combination field T[2:0] should encode decimal digit 1, so
        // T = (exp_top_2 << 3) | 1. exp = 0 → biased = 6176 → top 2
        // bits = 01 (since 6176 = 0b01_1000_0010_0000). T = 0b01001 = 9.
        let t = (bytes[0] >> 2) & 0b1_1111;
        assert_eq!(t, 0b01001, "T[2:0] should be decimal 1, not binary 0");
        // Round-trip preserved.
        let recovered = Decimal128::from_dpd_bytes(bytes);
        assert_eq!(recovered.to_bits(), d.to_bits());
    }
}
