// Phase 0 ships the codec primitive; Phase 1 wires it into
// `Decimal128::to_dpd_bytes` / `from_dpd_bytes`. Until then, the
// functions are within-crate-public but unused.
#![allow(dead_code)]
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
}
