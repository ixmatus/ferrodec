//! Kani harnesses for classification predicates. Port of
//! `ferrodec/src/verify/classify.rs` to the 64-bit format.

use crate::Decimal64;

/// Every 64-bit pattern falls into exactly one IEEE-754 class.
#[kani::proof]
fn classify_categories_disjoint() {
    let bits: u64 = kani::any();
    let d = Decimal64::from_bits(bits);

    let n_nan = d.is_nan() as u32;
    let n_inf = d.is_infinite() as u32;
    let n_zero = d.is_zero() as u32;
    let n_normal = d.is_normal() as u32;
    let n_subnormal = d.is_subnormal() as u32;

    // Exactly one bucket.
    assert!(n_nan + n_inf + n_zero + n_normal + n_subnormal == 1);
}

/// Quiet and signaling NaN partition the NaN class.
#[kani::proof]
fn nan_quiet_signaling_partition() {
    let bits: u64 = kani::any();
    let d = Decimal64::from_bits(bits);

    if d.is_nan() {
        // Exactly one of quiet / signaling.
        assert!(d.is_quiet_nan() ^ d.is_signaling_nan());
    } else {
        // Neither qNaN nor sNaN if not NaN at all.
        assert!(!d.is_quiet_nan());
        assert!(!d.is_signaling_nan());
    }
}

/// Signaling NaN implies NaN.
#[kani::proof]
fn signaling_nan_implies_nan() {
    let bits: u64 = kani::any();
    let d = Decimal64::from_bits(bits);
    if d.is_signaling_nan() {
        assert!(d.is_nan());
    }
}

/// `is_finite` ⇔ `!is_nan ∧ !is_infinite`.
#[kani::proof]
fn finite_complement_of_nan_or_inf() {
    let bits: u64 = kani::any();
    let d = Decimal64::from_bits(bits);
    assert!(d.is_finite() == !(d.is_nan() || d.is_infinite()));
}

/// `is_sign_negative` and `is_sign_positive` are exact complements.
#[kani::proof]
fn sign_predicates_complementary() {
    let bits: u64 = kani::any();
    let d = Decimal64::from_bits(bits);
    assert!(d.is_sign_negative() != d.is_sign_positive());
}

/// `abs(d)` is never sign-negative.
#[kani::proof]
fn abs_is_non_negative_sign() {
    let bits: u64 = kani::any();
    let d = Decimal64::from_bits(bits);
    assert!(!d.abs().is_sign_negative());
}

/// Negation flips the sign for every input, including NaN.
#[kani::proof]
fn neg_flips_sign() {
    let bits: u64 = kani::any();
    let d = Decimal64::from_bits(bits);
    assert!(d.neg().is_sign_negative() != d.is_sign_negative());
}

/// Double negation is identity at the bit level.
#[kani::proof]
fn neg_neg_is_identity() {
    let bits: u64 = kani::any();
    let d = Decimal64::from_bits(bits);
    assert!(d.neg().neg().to_bits() == d.to_bits());
}
