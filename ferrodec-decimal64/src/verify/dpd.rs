//! Kani harnesses for the DPD interchange surface
//! (`Decimal64::to_dpd_bytes` / `from_dpd_bytes`) and the underlying
//! declet codec.
//!
//! Three properties, mirroring the decimal128 codec (ADR-0009):
//!
//! 1. **Declet decode is total**: every 10-bit pattern decodes to a
//!    valid BCD triple (each nibble ≤ 9, no high bits set).
//! 2. **`from_dpd_bytes` is total**: every `[u8; 8]` decodes to a
//!    valid `Decimal64` — exactly one of NaN / Infinity / Finite
//!    holds. No panic, no debug-assert tripped.
//! 3. **Special-value round-trip**: each of `INFINITY`,
//!    `NEG_INFINITY`, `NAN`, `SIGNALING_NAN` survives encode →
//!    decode bit-equal.
//!
//! Properties (1) and (2) are the load-bearing totality proofs the
//! proptest cannot give. Round-trip over the canonical finite surface
//! is covered by `tests/property_dpd.rs`, not Kani — the decimal128
//! codec's stop-loss (ADR-0009) found that a symbolic full round-trip
//! over `% 1000` × declet expansion does not terminate in budget; the
//! decimal64 codec has fewer declets but the same shape, so the same
//! split applies.

use crate::dpd::{decode_declet, DECLET_COUNT};
use crate::Decimal64;

/// Every 10-bit declet pattern decodes to three valid BCD digits.
///
/// The declet primitive is precision-independent (it is the same
/// boolean transcription as the decimal128 / decimal32 codecs), so
/// this is identical to the parent harness; it pins the property in
/// this crate's verifier context so a future tweak cannot silently
/// introduce an out-of-range output.
#[kani::proof]
fn declet_decode_total() {
    let raw: u16 = kani::any();
    let declet = raw & 0x3FF;
    let bcd = decode_declet(declet);

    assert!(bcd >> 12 == 0);
    let d2 = bcd >> 8;
    let d1 = (bcd >> 4) & 0xF;
    let d0 = bcd & 0xF;
    assert!(d2 < 10);
    assert!(d1 < 10);
    assert!(d0 < 10);
}

/// `from_dpd_bytes` is total: every 64-bit pattern decodes to a
/// well-classified `Decimal64` (exactly one of NaN / Infinity / Finite
/// holds). Also a panic-freedom proof: any `debug_assert!` in
/// `from_dpd_bytes` or `decode_declet` reachable by a 64-bit input
/// would be flagged by CBMC.
#[kani::proof]
fn from_dpd_bytes_total() {
    let bytes: [u8; 8] = kani::any();
    let d = Decimal64::from_dpd_bytes(bytes);

    let nan = d.is_nan();
    let inf = d.is_infinite();
    let fin = d.is_finite();
    let count = (nan as u32) + (inf as u32) + (fin as u32);
    assert!(count == 1);
}

/// Round-trip: every distinguished IEEE 754 special value round-trips
/// through DPD bit-equal. See the module note on why the full canonical
/// round-trip lives in the property test rather than here.
#[kani::proof]
fn dpd_roundtrip_specials() {
    let selector: u8 = kani::any();
    let d = match selector & 0b11 {
        0 => Decimal64::INFINITY,
        1 => Decimal64::NEG_INFINITY,
        2 => Decimal64::NAN,
        _ => Decimal64::SIGNALING_NAN,
    };
    let recovered = Decimal64::from_dpd_bytes(d.to_dpd_bytes());
    assert!(recovered.to_bits() == d.to_bits());

    // Sanity: DECLET_COUNT × 10 == 50 (the trailing significand width).
    assert!(DECLET_COUNT * 10 == 50);
}
