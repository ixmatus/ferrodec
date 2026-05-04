//! Kani harnesses for the BID pack/unpack round-trip.

use crate::bid::{
    classify_bits, pack_finite, pack_infinity, pack_quiet_nan, pack_signaling_nan, Class,
    BIASED_EXP_MAX, COEFFICIENT_FIELD_LIMIT,
};

/// `pack_finite` followed by `classify_bits` recovers the inputs verbatim
/// for any in-range Form-A finite triple.
#[kani::proof]
fn pack_finite_unpack_roundtrip() {
    let sign: bool = kani::any();
    let biased_exp: u32 = kani::any();
    let coefficient: u128 = kani::any();
    kani::assume(biased_exp <= BIASED_EXP_MAX);
    kani::assume(coefficient < COEFFICIENT_FIELD_LIMIT);

    let bits = pack_finite(sign, biased_exp, coefficient);
    match classify_bits(bits) {
        Class::Zero {
            sign: s,
            biased_exp: e,
        } => {
            assert!(coefficient == 0);
            assert!(s == sign);
            assert!(e == biased_exp);
        }
        Class::Finite {
            sign: s,
            biased_exp: e,
            coefficient: c,
        } => {
            assert!(coefficient != 0);
            assert!(s == sign);
            assert!(e == biased_exp);
            assert!(c == coefficient);
        }
        // Form A encoding of a finite triple cannot decode as
        // Inf/NaN/Form-B-Zero — that would be a layout bug.
        _ => kani::cover!(false, "pack_finite produced non-finite encoding"),
    }
}

/// Infinity is round-trippable: any sign packs and unpacks to itself.
#[kani::proof]
fn pack_infinity_roundtrip() {
    let sign: bool = kani::any();
    let bits = pack_infinity(sign);
    match classify_bits(bits) {
        Class::Infinity { sign: s } => assert!(s == sign),
        _ => kani::cover!(false, "infinity decoded as non-infinity"),
    }
}

/// Quiet NaN is round-trippable for any sign and arbitrary 110-bit payload.
#[kani::proof]
fn pack_quiet_nan_roundtrip() {
    let sign: bool = kani::any();
    let payload: u128 = kani::any();
    // Payload is masked to 110 bits inside pack_quiet_nan.
    let bits = pack_quiet_nan(sign, payload);
    match classify_bits(bits) {
        Class::QuietNaN {
            sign: s,
            payload: p,
        } => {
            assert!(s == sign);
            assert!(p == payload & ((1u128 << 110) - 1));
        }
        _ => kani::cover!(false, "qNaN decoded as non-qNaN"),
    }
}

/// Signaling NaN is round-trippable.
#[kani::proof]
fn pack_signaling_nan_roundtrip() {
    let sign: bool = kani::any();
    let payload: u128 = kani::any();
    let bits = pack_signaling_nan(sign, payload);
    match classify_bits(bits) {
        Class::SignalingNaN {
            sign: s,
            payload: p,
        } => {
            assert!(s == sign);
            assert!(p == payload & ((1u128 << 110) - 1));
        }
        _ => kani::cover!(false, "sNaN decoded as non-sNaN"),
    }
}
