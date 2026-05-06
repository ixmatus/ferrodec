//! Kani harnesses for the IEEE 754-2019 §5.3 / §5.10 quantum surface
//! that doesn't depend on symbolic-coefficient multiplication.
//!
//! Skipped from this module:
//!
//! * `quantize`, `scaleb`, `logb` — all three multiply a symbolic
//!   coefficient by `pow10(delta)` for a symbolic `delta`. SMT cannot
//!   dispatch this path within the project's tens-of-minutes envelope.
//!   Those functions are instead exercised by the conformance suite
//!   (`tests/conformance.rs`) and the property tests in
//!   `tests/property_quantum.rs`.

use core::cmp::Ordering;

use crate::Decimal128;

/// `same_quantum` is reflexive over arbitrary bit patterns. Covers
/// the documented "NaN-vs-NaN is true" and "Inf-vs-Inf is true" rules
/// in addition to the finite case.
#[kani::proof]
fn same_quantum_reflexive() {
    let bits: u128 = kani::any();
    let d = Decimal128::from_bits(bits);
    assert!(d.same_quantum(d));
}

/// `same_quantum` is symmetric: `x.same_quantum(y)` and
/// `y.same_quantum(x)` agree for any pair.
#[kani::proof]
fn same_quantum_symmetric() {
    let a_bits: u128 = kani::any();
    let b_bits: u128 = kani::any();
    let a = Decimal128::from_bits(a_bits);
    let b = Decimal128::from_bits(b_bits);
    assert!(a.same_quantum(b) == b.same_quantum(a));
}

/// `compare_total_magnitude` is reflexive: |x|.total_cmp(|x|) ==
/// Equal for any bit pattern. Follows trivially from `total_cmp`'s
/// reflexivity and the deterministic nature of `abs`, but worth
/// pinning since `compare_total_magnitude` is a separate public
/// method.
#[kani::proof]
fn compare_total_magnitude_reflexive() {
    let bits: u128 = kani::any();
    let d = Decimal128::from_bits(bits);
    assert!(d.compare_total_magnitude(d) == Ordering::Equal);
}

/// `compare_total_magnitude` is antisymmetric off the
/// finite-finite-same-sign domain. Same restriction as
/// [`crate::verify::cmp::total_cmp_antisymmetric_off_finite_finite`]
/// since `compare_total_magnitude(x, y)` reduces to
/// `x.abs().total_cmp(y.abs())`. Cross-cohort same-magnitude finites
/// remain proptest-only.
#[kani::proof]
fn compare_total_magnitude_antisymmetric_off_finite_finite() {
    let a_bits: u128 = kani::any();
    let b_bits: u128 = kani::any();
    let a = Decimal128::from_bits(a_bits);
    let b = Decimal128::from_bits(b_bits);
    kani::assume(!a.is_finite() || a.is_zero() || !b.is_finite() || b.is_zero());

    let ab = a.compare_total_magnitude(b);
    let ba = b.compare_total_magnitude(a);
    assert!(ab == ba.reverse());
}

/// `radix()` is `10`. Trivial, but pins the constant against
/// accidental drift; `Decimal128::radix` is a public `const fn`
/// callers may rely on at compile time.
#[kani::proof]
fn radix_is_ten() {
    assert!(Decimal128::radix() == 10);
}

/// `next_up` returns the documented value for each special-case
/// dispatch path. Avoids the cohort-normalisation branch (which uses
/// a `pow10(expand)` multiplication on a symbolic exponent).
#[kani::proof]
fn next_up_special_dispatch() {
    let bits: u128 = kani::any();
    let d = Decimal128::from_bits(bits);
    // Restrict to the special cases the next_up path dispatches at
    // the top: signaling NaN, quiet NaN, ±0, ±∞.
    kani::assume(d.is_signaling_nan() || d.is_nan() || d.is_zero() || d.is_infinite());

    let (r, _) = d.next_up();

    if d.is_signaling_nan() {
        assert!(r.is_nan());
    } else if d.is_nan() {
        assert!(r.is_nan());
    } else if d.is_zero() {
        assert!(r.to_bits() == Decimal128::MIN_POSITIVE.to_bits());
    } else if d == Decimal128::NEG_INFINITY {
        assert!(r.to_bits() == Decimal128::MIN.to_bits());
    } else if d == Decimal128::INFINITY {
        assert!(r.to_bits() == Decimal128::INFINITY.to_bits());
    }
}
