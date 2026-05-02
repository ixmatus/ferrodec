//! Kani harnesses for `Decimal128::mul`.
//!
//! Same strategy as the addsub harnesses: target the loop-free
//! [`Decimal128::mul_special_only_for_kani`] entry point with operands
//! drawn from a small representative set, so CBMC never has to unroll
//! the 226-bit product / rounding pipeline.
//!
//! Finite-finite multiplication correctness is delegated to the proptest
//! harness in `tests/property_mul.rs`.

use crate::status::RoundingMode;
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

/// Whenever at least one operand is non-finite or zero, the multiply
/// special-case path resolves to `Some` — `mul_finite_finite` is never
/// invoked for those classes.
#[kani::proof]
fn mul_special_resolves_when_either_is_non_finite_or_zero() {
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

    assert!(a.mul_special_only_for_kani(b).is_some());
}

/// NaN propagates through `mul`'s special path for any operand class.
#[kani::proof]
fn mul_nan_propagates() {
    let ai: u8 = kani::any();
    let bi: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    kani::assume(bi < NUM_OPERANDS);

    let a = operand(ai);
    let b = operand(bi);
    kani::assume(a.is_nan() || b.is_nan());

    let (r, _) = a
        .mul_special_only_for_kani(b)
        .expect("NaN path resolved by special_cases");
    assert!(r.is_nan());
}

/// Signaling NaN raises `INVALID`.
#[kani::proof]
fn mul_snan_raises_invalid() {
    let ai: u8 = kani::any();
    let bi: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    kani::assume(bi < NUM_OPERANDS);

    let a = operand(ai);
    let b = operand(bi);
    kani::assume(a.is_signaling_nan() || b.is_signaling_nan());

    let (_, status) = a
        .mul_special_only_for_kani(b)
        .expect("sNaN path resolved by special_cases");
    assert!(status.invalid());
}

/// `0 × ∞` and `∞ × 0` (in either sign combination) are NaN with
/// `INVALID` raised.
#[kani::proof]
fn zero_times_inf_is_nan_invalid() {
    let zero_sign: bool = kani::any();
    let inf_sign: bool = kani::any();
    let zero_first: bool = kani::any();

    let zero = if zero_sign { Decimal128::NEG_ZERO } else { Decimal128::ZERO };
    let inf = if inf_sign { Decimal128::NEG_INFINITY } else { Decimal128::INFINITY };

    let (a, b) = if zero_first { (zero, inf) } else { (inf, zero) };

    let (r, status) = a
        .mul_special_only_for_kani(b)
        .expect("0 × ∞ is special-cased");
    assert!(r.is_nan());
    assert!(status.invalid());
}

/// `∞ × ∞` and `∞ × finite_non_zero`: result is `±∞` with sign
/// `sign(a) ⊕ sign(b)`.
#[kani::proof]
fn inf_times_anything_nonzero_is_inf_signed() {
    let sa: bool = kani::any();
    let inf = if sa { Decimal128::NEG_INFINITY } else { Decimal128::INFINITY };

    // Other operand: any non-NaN, non-zero. Restrict to representatives that
    // are not zero / NaN / sNaN.
    let oi: u8 = kani::any();
    kani::assume(oi == 2 || oi == 3 || oi == 6 || oi == 7 || oi == 8 || oi == 9);
    let other = operand(oi);

    let sb = other.is_sign_negative();

    let (r, status) = inf
        .mul_special_only_for_kani(other)
        .expect("∞ × non-zero is special-cased");
    assert!(r.is_infinite());
    assert!(r.is_sign_negative() == (sa ^ sb));
    assert!(!status.invalid());
}

/// `0 × 0`, `0 × finite_nonzero`, `finite_nonzero × 0` all give `±0`
/// with `sign(a) ⊕ sign(b)`.
#[kani::proof]
fn zero_times_finite_is_signed_zero() {
    let sa: bool = kani::any();
    let zero = if sa { Decimal128::NEG_ZERO } else { Decimal128::ZERO };

    // Other operand: zero or finite-non-zero (not NaN, not Inf).
    let oi: u8 = kani::any();
    kani::assume(oi == 4 || oi == 5 || oi == 6 || oi == 7 || oi == 8 || oi == 9);
    let other = operand(oi);

    let sb = other.is_sign_negative();

    let (r, status) = zero
        .mul_special_only_for_kani(other)
        .expect("0 × finite is special-cased");
    assert!(r.is_zero());
    assert!(r.is_sign_negative() == (sa ^ sb));
    assert!(!status.invalid());

    // Symmetric.
    let (r, status) = other
        .mul_special_only_for_kani(zero)
        .expect("finite × 0 is special-cased");
    assert!(r.is_zero());
    assert!(r.is_sign_negative() == (sa ^ sb));
    assert!(!status.invalid());
}

/// Status from the special path never raises flags other than `INVALID`.
#[kani::proof]
fn mul_special_status_only_invalid() {
    let ai: u8 = kani::any();
    let bi: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    kani::assume(bi < NUM_OPERANDS);

    let a = operand(ai);
    let b = operand(bi);

    if let Some((_, status)) = a.mul_special_only_for_kani(b) {
        assert!(!status.div_by_zero());
        assert!(!status.overflow());
        assert!(!status.underflow());
        assert!(!status.inexact());
        // INVALID is allowed: sNaN inputs OR 0 × ∞.
        let _ = RoundingMode::NearestEven; // silence warning if unused
    }
}
