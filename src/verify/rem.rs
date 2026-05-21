//! Kani harnesses for `Decimal128::rem_near` (the IEEE 754-2019 §5.3.1
//! nearest-even remainder; in 1.x this was named bare `rem`).

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

/// The special-case path resolves whenever any operand is non-finite,
/// the divisor is zero, or the dividend is zero.
#[kani::proof]
fn rem_special_resolves() {
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

    assert!(a.rem_special_only_for_kani(b).is_some());
}

#[kani::proof]
fn rem_nan_propagates() {
    let ai: u8 = kani::any();
    let bi: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    kani::assume(bi < NUM_OPERANDS);
    let a = operand(ai);
    let b = operand(bi);
    kani::assume(a.is_nan() || b.is_nan());

    let (r, _) = a
        .rem_special_only_for_kani(b)
        .expect("NaN path special-cased");
    assert!(r.is_nan());
}

#[kani::proof]
fn rem_snan_raises_invalid() {
    let ai: u8 = kani::any();
    let bi: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    kani::assume(bi < NUM_OPERANDS);
    let a = operand(ai);
    let b = operand(bi);
    kani::assume(a.is_signaling_nan() || b.is_signaling_nan());
    let (_, s) = a
        .rem_special_only_for_kani(b)
        .expect("sNaN path special-cased");
    assert!(s.invalid());
}

/// `x / 0` is invalid — NaN + INVALID.
#[kani::proof]
fn rem_x_over_zero_invalid() {
    let xi: u8 = kani::any();
    kani::assume(xi >= 6 && xi < NUM_OPERANDS); // ONE / NEG_ONE / MAX / MIN
    let zb: bool = kani::any();
    let zero = if zb {
        Decimal128::NEG_ZERO
    } else {
        Decimal128::ZERO
    };
    let (r, s) = operand(xi)
        .rem_special_only_for_kani(zero)
        .expect("x/0 special-cased");
    assert!(r.is_nan());
    assert!(s.invalid());
}

/// `±∞ / y` is invalid — NaN + INVALID for any non-NaN y.
#[kani::proof]
fn rem_inf_over_y_invalid() {
    let sa: bool = kani::any();
    let inf = if sa {
        Decimal128::NEG_INFINITY
    } else {
        Decimal128::INFINITY
    };
    let bi: u8 = kani::any();
    kani::assume(bi >= 2 && bi < NUM_OPERANDS); // not NaN/sNaN
    let (r, s) = inf
        .rem_special_only_for_kani(operand(bi))
        .expect("∞/y special-cased");
    assert!(r.is_nan());
    assert!(s.invalid());
}

/// `x / ±∞` returns `x` exactly (with whatever sign and cohort `x` has).
#[kani::proof]
fn rem_x_over_inf_returns_x() {
    let xi: u8 = kani::any();
    kani::assume(xi == 4 || xi == 5 || xi == 6 || xi == 7 || xi == 8 || xi == 9);
    let sb: bool = kani::any();
    let inf = if sb {
        Decimal128::NEG_INFINITY
    } else {
        Decimal128::INFINITY
    };

    let x = operand(xi);
    let (r, s) = x.rem_special_only_for_kani(inf).expect("x/∞ special-cased");
    assert!(r.to_bits() == x.to_bits());
    assert!(!s.invalid());
}
