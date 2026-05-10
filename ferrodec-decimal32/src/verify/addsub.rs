//! Kani harnesses for `Decimal32::add` and `Decimal32::sub`.
//!
//! The harnesses bound operand inputs to a 10-constant set and
//! verify:
//!
//! 1. No panic on any combination of the bounded operands and any
//!    rounding mode.
//! 2. NaN propagation: a NaN input produces a NaN output.
//! 3. Signaling NaN raises `INVALID`.
//! 4. `(±∞) + (±∞)` of opposite sign → NaN + INVALID; same sign →
//!    same-signed infinity.
//! 5. IEEE 754-2019 §6.3 zero-sign rule: `(+0) + (−0) = +0` in all
//!    rounding modes except `TowardNegative`, which yields `−0`.

use super::{operand, rm_from_u8, NUM_OPERANDS};
use crate::decimal::Decimal32;
use ferrodec_ieee::RoundingMode;

#[kani::proof]
fn add_no_panic_special_inputs() {
    let ai: u8 = kani::any();
    let bi: u8 = kani::any();
    let rmi: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    kani::assume(bi < NUM_OPERANDS);
    kani::assume(rmi <= 4);

    let a = operand(ai);
    let b = operand(bi);
    let mode = rm_from_u8(rmi);

    let _ = a.add(b, mode);
    // Implicit: reaching this point means no panic.
}

#[kani::proof]
fn sub_no_panic_special_inputs() {
    let ai: u8 = kani::any();
    let bi: u8 = kani::any();
    let rmi: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    kani::assume(bi < NUM_OPERANDS);
    kani::assume(rmi <= 4);

    let a = operand(ai);
    let b = operand(bi);
    let mode = rm_from_u8(rmi);

    let _ = a.sub(b, mode);
}

#[kani::proof]
fn add_nan_propagates() {
    let ai: u8 = kani::any();
    let bi: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    kani::assume(bi < NUM_OPERANDS);

    let a = operand(ai);
    let b = operand(bi);
    kani::assume(a.is_nan() || b.is_nan());

    let (r, _) = a.add(b, RoundingMode::NearestEven);
    assert!(r.is_nan());
}

#[kani::proof]
fn add_snan_raises_invalid() {
    let ai: u8 = kani::any();
    let bi: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    kani::assume(bi < NUM_OPERANDS);

    let a = operand(ai);
    let b = operand(bi);
    kani::assume(a.is_signaling_nan() || b.is_signaling_nan());

    let (_, status) = a.add(b, RoundingMode::NearestEven);
    assert!(status.invalid());
}

#[kani::proof]
fn add_infinity_arithmetic() {
    let sa: bool = kani::any();
    let sb: bool = kani::any();
    let a = if sa {
        Decimal32::NEG_INFINITY
    } else {
        Decimal32::INFINITY
    };
    let b = if sb {
        Decimal32::NEG_INFINITY
    } else {
        Decimal32::INFINITY
    };

    let (r, status) = a.add(b, RoundingMode::NearestEven);
    if sa == sb {
        assert!(r.is_infinite());
        assert!(r.is_sign_negative() == sa);
        assert!(!status.invalid());
    } else {
        assert!(r.is_nan());
        assert!(status.invalid());
    }
}

#[kani::proof]
fn add_zero_zero_sign_rule() {
    let sa: bool = kani::any();
    let sb: bool = kani::any();
    let rmi: u8 = kani::any();
    kani::assume(rmi <= 4);

    let a = if sa {
        Decimal32::NEG_ZERO
    } else {
        Decimal32::ZERO
    };
    let b = if sb {
        Decimal32::NEG_ZERO
    } else {
        Decimal32::ZERO
    };
    let mode = rm_from_u8(rmi);

    let (r, _) = a.add(b, mode);
    assert!(r.is_zero());

    let expected_negative = if sa == sb {
        sa
    } else {
        matches!(mode, RoundingMode::TowardNegative)
    };
    assert!(r.is_sign_negative() == expected_negative);
}
