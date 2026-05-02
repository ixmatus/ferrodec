//! Kani harnesses for `Decimal128::sqrt`.

use crate::Decimal128;

const NUM_OPERANDS: u8 = 10;

fn operand(idx: u8) -> Decimal128 {
    match idx {
        0 => Decimal128::NAN,
        1 => Decimal128::SIGNALING_NAN,
        2 => Decimal128::INFINITY,
        3 => Decimal128::NEG_INFINITY,
        4 => Decimal128::ZERO,
        5 => Decimal128::NEG_ZERO,
        6 => Decimal128::ONE,
        7 => Decimal128::NEG_ONE,
        8 => Decimal128::MAX,
        _ => Decimal128::MIN,
    }
}

/// The special-case path resolves for any non-finite-positive operand.
#[kani::proof]
fn sqrt_special_resolves_for_non_positive_finite() {
    let i: u8 = kani::any();
    kani::assume(i < NUM_OPERANDS);

    let a = operand(i);
    kani::assume(a.is_nan() || a.is_infinite() || a.is_zero() || a.is_sign_negative());

    assert!(a.sqrt_special_only_for_kani().is_some());
}

#[kani::proof]
fn sqrt_nan_propagates() {
    let i: u8 = kani::any();
    kani::assume(i < NUM_OPERANDS);
    let a = operand(i);
    kani::assume(a.is_nan());

    let (r, _) = a
        .sqrt_special_only_for_kani()
        .expect("NaN path resolved by special_cases");
    assert!(r.is_nan());
}

#[kani::proof]
fn sqrt_snan_raises_invalid() {
    let i: u8 = kani::any();
    kani::assume(i < NUM_OPERANDS);
    let a = operand(i);
    kani::assume(a.is_signaling_nan());
    let (_, s) = a
        .sqrt_special_only_for_kani()
        .expect("sNaN path resolved");
    assert!(s.invalid());
}

#[kani::proof]
fn sqrt_negative_finite_is_invalid_nan() {
    let i: u8 = kani::any();
    kani::assume(i < NUM_OPERANDS);
    let a = operand(i);
    kani::assume(a.is_finite() && !a.is_zero() && !a.is_nan() && a.is_sign_negative());

    let (r, s) = a
        .sqrt_special_only_for_kani()
        .expect("negative finite is special-cased");
    assert!(r.is_nan());
    assert!(s.invalid());
}

#[kani::proof]
fn sqrt_neg_inf_is_invalid_nan() {
    let (r, s) = Decimal128::NEG_INFINITY
        .sqrt_special_only_for_kani()
        .expect("-∞ is special-cased");
    assert!(r.is_nan());
    assert!(s.invalid());
}

#[kani::proof]
fn sqrt_pos_inf_is_pos_inf() {
    let (r, s) = Decimal128::INFINITY
        .sqrt_special_only_for_kani()
        .expect("+∞ is special-cased");
    assert!(r.is_infinite());
    assert!(!r.is_sign_negative());
    assert!(!s.invalid());
}

#[kani::proof]
fn sqrt_zero_preserves_sign() {
    let s: bool = kani::any();
    let z = if s { Decimal128::NEG_ZERO } else { Decimal128::ZERO };
    let (r, _) = z
        .sqrt_special_only_for_kani()
        .expect("0 is special-cased");
    assert!(r.is_zero());
    assert!(r.is_sign_negative() == s);
}
