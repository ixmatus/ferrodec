//! Kani harnesses for the DPD interchange surface
//! (`Decimal32::to_dpd_bytes` / `from_dpd_bytes`) and the underlying
//! declet codec. Port of `ferrodec/src/verify/dpd.rs` to the 32-bit
//! format (two declets, `[u8; 4]`).
//!
//! `decimal64` has no DPD codec, so this port is decimal32-only.
//!
//! Three properties:
//!
//! 1. **Declet decode is total**: every 10-bit pattern decodes to a
//!    valid BCD triple (each nibble ≤ 9, no high bits set).
//! 2. **`from_dpd_bytes` is total**: every `[u8; 4]` decodes to a
//!    valid `Decimal32` — exactly one of NaN / Infinity / Finite
//!    holds. No panic, no debug-assert tripped.
//! 3. **Special-value round-trip**: each distinguished special value
//!    survives encode → decode bit-equal.
//!
//! (1) and (2) are totality proofs the proptest cannot give; (3) is a
//! structural check on the special-value paths. Round-trip over the
//! finite surface is covered by `tests/property_dpd.rs` (the same
//! stop-loss as the Decimal128 port: the arbitrary-operand round-trip
//! harness blew the CBMC budget and was dropped in favour of the
//! property test).

use crate::dpd::{decode_declet, DECLET_COUNT};
use crate::Decimal32;

/// Every 10-bit declet pattern decodes to three valid BCD digits
/// (IEEE 754-2008 §3.5.2; total over all 1024 patterns, including the
/// 24 non-canonical ones).
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

/// `from_dpd_bytes` is total: every 32-bit pattern decodes to a
/// well-classified `Decimal32` (exactly one of NaN / Infinity /
/// Finite). Also a panic-freedom proof for the decoder.
#[kani::proof]
fn from_dpd_bytes_total() {
    let bytes: [u8; 4] = kani::any();
    let d = Decimal32::from_dpd_bytes(bytes);

    let nan = d.is_nan();
    let inf = d.is_infinite();
    let fin = d.is_finite();
    let count = (nan as u32) + (inf as u32) + (fin as u32);
    assert!(count == 1);
}

/// Every distinguished IEEE 754 special value round-trips through DPD
/// bit-equal.
#[kani::proof]
fn dpd_roundtrip_specials() {
    let selector: u8 = kani::any();
    let d = match selector & 0b11 {
        0 => Decimal32::INFINITY,
        1 => Decimal32::NEG_INFINITY,
        2 => Decimal32::NAN,
        _ => Decimal32::SIGNALING_NAN,
    };
    let recovered = Decimal32::from_dpd_bytes(d.to_dpd_bytes());
    assert!(recovered.to_bits() == d.to_bits());

    // Structural sanity: two declets carry the 20-bit trailing
    // significand of decimal32.
    assert!(DECLET_COUNT * 10 == 20);
}
