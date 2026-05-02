//! Kani harnesses for `Decimal128::div`.

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

#[kani::proof]
fn div_special_resolves_when_either_is_non_finite_or_zero() {
    let ai: u8 = kani::any();
    let bi: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    kani::assume(bi < NUM_OPERANDS);

    let a = operand(ai);
    let b = operand(bi);
    kani::assume(
        a.is_nan()
            || a.is_infinite()
            || a.is_zero()
            || b.is_nan()
            || b.is_infinite()
            || b.is_zero(),
    );

    assert!(a.div_special_only_for_kani(b).is_some());
}

#[kani::proof]
fn div_nan_propagates() {
    let ai: u8 = kani::any();
    let bi: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    kani::assume(bi < NUM_OPERANDS);

    let a = operand(ai);
    let b = operand(bi);
    kani::assume(a.is_nan() || b.is_nan());

    let (r, _) = a
        .div_special_only_for_kani(b)
        .expect("NaN path resolved by special_cases");
    assert!(r.is_nan());
}

#[kani::proof]
fn div_snan_raises_invalid() {
    let ai: u8 = kani::any();
    let bi: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    kani::assume(bi < NUM_OPERANDS);

    let a = operand(ai);
    let b = operand(bi);
    kani::assume(a.is_signaling_nan() || b.is_signaling_nan());

    let (_, status) = a
        .div_special_only_for_kani(b)
        .expect("sNaN path resolved by special_cases");
    assert!(status.invalid());
}

/// `0 / 0` and `∞ / ∞` are invalid: NaN + INVALID.
#[kani::proof]
fn zero_over_zero_and_inf_over_inf_invalid() {
    let za: bool = kani::any();
    let zb: bool = kani::any();
    let zero_a = if za { Decimal128::NEG_ZERO } else { Decimal128::ZERO };
    let zero_b = if zb { Decimal128::NEG_ZERO } else { Decimal128::ZERO };

    let (r, s) = zero_a
        .div_special_only_for_kani(zero_b)
        .expect("0/0 is special-cased");
    assert!(r.is_nan());
    assert!(s.invalid());

    let inf_a = if za { Decimal128::NEG_INFINITY } else { Decimal128::INFINITY };
    let inf_b = if zb { Decimal128::NEG_INFINITY } else { Decimal128::INFINITY };
    let (r, s) = inf_a
        .div_special_only_for_kani(inf_b)
        .expect("∞/∞ is special-cased");
    assert!(r.is_nan());
    assert!(s.invalid());
}

/// `finite_nonzero / 0` raises DIV_BY_ZERO and yields ±∞ with sign XOR.
#[kani::proof]
fn finite_over_zero_div_by_zero() {
    let sa: bool = kani::any();
    let sb: bool = kani::any();
    let a = if sa { Decimal128::NEG_ONE } else { Decimal128::ONE };
    let zero = if sb { Decimal128::NEG_ZERO } else { Decimal128::ZERO };

    let (r, s) = a
        .div_special_only_for_kani(zero)
        .expect("finite/0 is special-cased");
    assert!(r.is_infinite());
    assert!(s.div_by_zero());
    assert!(r.is_sign_negative() == (sa ^ sb));
}

/// `∞ / 0` is `±∞` but does NOT raise DIV_BY_ZERO (the infinity is genuine).
#[kani::proof]
fn inf_over_zero_no_div_by_zero() {
    let sa: bool = kani::any();
    let sb: bool = kani::any();
    let inf = if sa { Decimal128::NEG_INFINITY } else { Decimal128::INFINITY };
    let zero = if sb { Decimal128::NEG_ZERO } else { Decimal128::ZERO };

    let (r, s) = inf
        .div_special_only_for_kani(zero)
        .expect("∞/0 is special-cased");
    assert!(r.is_infinite());
    assert!(!s.div_by_zero());
    assert!(r.is_sign_negative() == (sa ^ sb));
}
