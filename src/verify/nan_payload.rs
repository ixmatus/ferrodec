//! Kani harnesses for NaN payload propagation through arithmetic.
//!
//! ### Strategy
//!
//! IEEE 754-2019 §6.2.3 allows the implementation to choose which
//! operand's payload propagates when both inputs are NaN; ferrodec
//! documents "first NaN wins" (see `src/ops/nan_propagate.rs`). The
//! audit-leg of the May 2026 6-agent correctness review noted that
//! this rule has no Kani coverage — every harness exercises NaN
//! *category* propagation (NaN-in → NaN-out) but none asserts the
//! payload bits actually survive.
//!
//! Symbolic execution over the full 110-bit payload field would
//! blow the CBMC budget (`feedback_kani_strategy.md`). We bound the
//! payload to 8 bits — sufficient because the propagation path
//! (`pack_quiet_nan(sign, payload & T_MASK)`) is uniform on payload
//! width: a width-8 bug would also be a width-110 bug.

use crate::status::RoundingMode;
use crate::Decimal128;

/// Build a quiet NaN with a small symbolic payload.
fn build_qnan(sign: bool, payload8: u8) -> Decimal128 {
    Decimal128::from_bits(crate::bid::pack_quiet_nan(sign, payload8 as u128))
}

/// Build a signaling NaN with a small symbolic payload.
fn build_snan(sign: bool, payload8: u8) -> Decimal128 {
    Decimal128::from_bits(crate::bid::pack_signaling_nan(sign, payload8 as u128))
}

fn rm_from_u8(x: u8) -> RoundingMode {
    match x {
        0 => RoundingMode::NearestEven,
        1 => RoundingMode::NearestAway,
        2 => RoundingMode::TowardZero,
        3 => RoundingMode::TowardPositive,
        _ => RoundingMode::TowardNegative,
    }
}

/// `add(qNaN_a, x)` returns a qNaN whose low 8 payload bits match
/// `qNaN_a`'s. Pinned over symbolic sign + payload + rhs class.
#[kani::proof]
fn add_propagates_first_qnan_payload() {
    let sign: bool = kani::any();
    let payload8: u8 = kani::any();
    let rmi: u8 = kani::any();
    kani::assume(rmi <= 4);

    let a = build_qnan(sign, payload8);
    let b = Decimal128::ONE; // any non-NaN
    let (r, _) = a.add(b, rm_from_u8(rmi));

    assert!(r.is_nan());
    let r_payload = r.to_bits() & ((1u128 << 110) - 1);
    let r_payload_low = (r_payload as u8) as u128;
    assert!(r_payload_low == payload8 as u128);
}

/// `add(sNaN_a, x)` raises INVALID *and* returns a quiet NaN whose
/// low 8 payload bits match `sNaN_a`'s. The sNaN signal is consumed;
/// the payload is preserved.
#[kani::proof]
fn add_snan_raises_invalid_and_preserves_payload() {
    let sign: bool = kani::any();
    let payload8: u8 = kani::any();
    let rmi: u8 = kani::any();
    kani::assume(rmi <= 4);

    let a = build_snan(sign, payload8);
    let b = Decimal128::ONE;
    let (r, s) = a.add(b, rm_from_u8(rmi));

    assert!(r.is_nan());
    assert!(r.is_quiet_nan(), "sNaN must be quieted on output");
    assert!(s.invalid(), "sNaN must raise INVALID");
    let r_payload_low = ((r.to_bits() & ((1u128 << 110) - 1)) as u8) as u128;
    assert!(r_payload_low == payload8 as u128);
}

/// `mul` mirrors `add`'s payload propagation. Same shape, separate
/// proof so a regression in one doesn't go unnoticed because the
/// other still passes.
#[kani::proof]
fn mul_propagates_first_qnan_payload() {
    let sign: bool = kani::any();
    let payload8: u8 = kani::any();
    let rmi: u8 = kani::any();
    kani::assume(rmi <= 4);

    let a = build_qnan(sign, payload8);
    let b = Decimal128::from_i32(2);
    let (r, _) = a.mul(b, rm_from_u8(rmi));

    assert!(r.is_nan());
    let r_payload_low = ((r.to_bits() & ((1u128 << 110) - 1)) as u8) as u128;
    assert!(r_payload_low == payload8 as u128);
}

/// `mul(sNaN, x)` raises INVALID and quiets to a NaN with the same
/// low payload bits. Symmetric to the add proof.
#[kani::proof]
fn mul_snan_raises_invalid_and_preserves_payload() {
    let sign: bool = kani::any();
    let payload8: u8 = kani::any();
    let rmi: u8 = kani::any();
    kani::assume(rmi <= 4);

    let a = build_snan(sign, payload8);
    let b = Decimal128::from_i32(3);
    let (r, s) = a.mul(b, rm_from_u8(rmi));

    assert!(r.is_nan());
    assert!(r.is_quiet_nan());
    assert!(s.invalid());
    let r_payload_low = ((r.to_bits() & ((1u128 << 110) - 1)) as u8) as u128;
    assert!(r_payload_low == payload8 as u128);
}
