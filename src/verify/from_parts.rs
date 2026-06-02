//! Kani harnesses for [`Decimal128::from_parts`], the inverse of
//! [`Decimal128::decode`].
//!
//! `from_parts` is a thin wrapper over `bid::pack_finite` plus two range
//! checks. It forms a bijection with `decode` on canonical finite values,
//! proved here in both directions. SMT dispatches the `(bool, u128, i16)`
//! domain in seconds.

use crate::bid::{BIAS, BIASED_EXP_MAX, COEFFICIENT_LIMIT};
use crate::decimal::{Decimal128, Decimal128Parts};

/// `from_parts` returns `Some` exactly when the coefficient is below the
/// limit and the biased exponent is in `[0, BIASED_EXP_MAX]`, and the
/// result then decodes back to the same parts verbatim. This is the
/// `from_parts` then `decode` direction of the bijection.
#[kani::proof]
fn from_parts_in_range_roundtrips() {
    let negative: bool = kani::any();
    let coefficient: u128 = kani::any();
    let exponent: i16 = kani::any();
    let parts = Decimal128Parts {
        negative,
        coefficient,
        exponent,
    };

    let biased = exponent as i32 + BIAS as i32;
    let in_range =
        coefficient < COEFFICIENT_LIMIT && biased >= 0 && biased <= BIASED_EXP_MAX as i32;

    match Decimal128::from_parts(parts) {
        Some(d) => {
            assert!(in_range);
            // `from_parts` produced a finite value, so `decode` is `Some`.
            let p = d.decode().expect("from_parts result is finite, so decodes");
            assert!(p.negative == negative);
            assert!(p.coefficient == coefficient);
            assert!(p.exponent == exponent);
        }
        None => assert!(!in_range),
    }
}

/// For any canonical finite value, `decode` then `from_parts` reproduces
/// the original bit pattern. This is the `decode` then `from_parts`
/// direction of the bijection. Non-canonical encodings are excluded: they
/// decode to a canonical cohort member whose re-encoding differs by design.
#[kani::proof]
fn decode_then_from_parts_bit_identity() {
    let bits: u128 = kani::any();
    let d = Decimal128::from_bits(bits);
    kani::assume(d.is_finite());
    kani::assume(d.is_canonical());

    let p = d.decode().expect("finite values decode");
    let r = Decimal128::from_parts(p).expect("decoded canonical parts re-encode");
    assert!(r.to_bits() == d.to_bits());
}

/// Out-of-range parts (coefficient at or above the limit, or a biased
/// exponent outside `[0, BIASED_EXP_MAX]`) return `None`.
#[kani::proof]
fn from_parts_out_of_range_is_none() {
    let negative: bool = kani::any();
    let coefficient: u128 = kani::any();
    let exponent: i16 = kani::any();

    let biased = exponent as i32 + BIAS as i32;
    kani::assume(coefficient >= COEFFICIENT_LIMIT || biased < 0 || biased > BIASED_EXP_MAX as i32);

    let parts = Decimal128Parts {
        negative,
        coefficient,
        exponent,
    };
    assert!(Decimal128::from_parts(parts).is_none());
}
