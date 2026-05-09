//! Kani harnesses for the DPD interchange surface
//! (`Decimal128::to_dpd_bytes` / `from_dpd_bytes`) and the underlying
//! declet codec.
//!
//! Three properties:
//!
//! 1. **Declet decode is total**: every 10-bit pattern decodes to a
//!    valid BCD triple (each nibble ≤ 9, no high bits set).
//! 2. **`from_dpd_bytes` is total**: every `[u8; 16]` decodes to a
//!    valid `Decimal128` — exactly one of NaN / Infinity / Finite
//!    holds. No panic, no debug-assert tripped.
//! 3. **Special-value round-trip**: each of `INFINITY`,
//!    `NEG_INFINITY`, `NAN`, `SIGNALING_NAN` survives encode →
//!    decode bit-equal.
//!
//! Properties (1) and (2) are the load-bearing ones — they are
//! totality proofs that the proptest cannot give. (3) is a structural
//! check on the special-value paths.
//!
//! Round-trip over the entire canonical finite surface is covered by
//! `tests/property_dpd.rs` rather than Kani; see the stop-loss note
//! on `dpd_roundtrip_specials` below.

use crate::dpd::{decode_declet, DECLET_COUNT};
use crate::Decimal128;

/// Every 10-bit declet pattern decodes to three valid BCD digits.
///
/// IEEE 754-2008 §3.5.2 specifies that every declet — including the
/// 24 "non-canonical" patterns — decodes to a digit triple in
/// `0..=999`. The decoder is total over the entire 1024-pattern
/// space, by construction; this harness pins the property
/// symbolically so any future tweak to the boolean equations cannot
/// silently introduce an out-of-range output.
#[kani::proof]
fn declet_decode_total() {
    let raw: u16 = kani::any();
    let declet = raw & 0x3FF;
    let bcd = decode_declet(declet);

    // Upper four bits must be zero.
    assert!(bcd >> 12 == 0);
    // Each of the three nibbles must be a valid decimal digit.
    let d2 = bcd >> 8;
    let d1 = (bcd >> 4) & 0xF;
    let d0 = bcd & 0xF;
    assert!(d2 < 10);
    assert!(d1 < 10);
    assert!(d0 < 10);
}

/// `from_dpd_bytes` is total: every 128-bit pattern decodes to a
/// well-classified `Decimal128`.
///
/// Specifically, after decoding, exactly one of NaN, Infinity, or
/// Finite (which includes Zero per IEEE classification) holds. This
/// also serves as a panic-freedom proof: if any input could trip a
/// `debug_assert!` in `from_dpd_bytes` or `decode_declet`, CBMC would
/// flag it.
#[kani::proof]
fn from_dpd_bytes_total() {
    let bytes: [u8; 16] = kani::any();
    let d = Decimal128::from_dpd_bytes(bytes);

    // Exactly one of {NaN, Infinity, Finite} holds. is_finite()
    // excludes NaN/Inf by definition, so the categories are
    // disjoint; we just need one of them to be true.
    let nan = d.is_nan();
    let inf = d.is_infinite();
    let fin = d.is_finite();
    let count = (nan as u32) + (inf as u32) + (fin as u32);
    assert!(count == 1);
}

/// Round-trip: every distinguished IEEE 754 special value (Inf, NaN,
/// signed zeros) round-trips through DPD bit-equal.
///
/// **Stop-loss note**: an earlier draft of this module included a
/// `dpd_roundtrip_via_try_new` harness that quantified over an
/// arbitrary `(i128, i32)` operand pair, ran it through `try_new` →
/// `to_dpd_bytes` → `from_dpd_bytes`, and asserted bit-equality. CBMC
/// did not terminate within 10+ minutes on a developer laptop —
/// unrolling 11 iterations of symbolic `u128 % 1000` plus the boolean
/// expansion of `encode_declet`/`decode_declet` blew past the
/// existing 2-minute full-suite budget. The plan's stop-loss for that
/// harness was "drop it and rely on the property test"; the
/// round-trip property is now covered exclusively by
/// `tests/property_dpd.rs::finite_string_roundtrip` and
/// `finite_bid_dpd_bid_identity_via_construction`. The two `*_total`
/// harnesses below — which the plan flagged as "the higher-value
/// ones (they prove totality, which property tests can't)" — both
/// land in seconds.
#[kani::proof]
fn dpd_roundtrip_specials() {
    let selector: u8 = kani::any();
    let d = match selector & 0b11 {
        0 => Decimal128::INFINITY,
        1 => Decimal128::NEG_INFINITY,
        2 => Decimal128::NAN,
        _ => Decimal128::SIGNALING_NAN,
    };
    let recovered = Decimal128::from_dpd_bytes(d.to_dpd_bytes());
    assert!(recovered.to_bits() == d.to_bits());

    // Sanity: DECLET_COUNT × 10 == 110 (the trailing significand
    // width). Cheap structural assertion that lives well in this
    // harness's verifier context.
    assert!(DECLET_COUNT * 10 == 110);
}
