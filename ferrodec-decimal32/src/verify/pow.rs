//! Kani harnesses for `Decimal32::pow` and `Decimal32::cbrt`.
//!
//! Routes every assertion through `pow_special_only_for_kani` /
//! `cbrt_special_only_for_kani` per ADR-0016. CBMC never encodes the
//! negative-base integer test or the `libm` + `from_f64` finite
//! pipeline; we prove no-panic and IEEE 754-2019 §9.2 special-case
//! propagation only.

use super::{operand, NUM_OPERANDS};
use crate::decimal::Decimal32;

/// `pow` resolves in the special-case path exactly when the exponent
/// is zero, the base numerically equals one (keyed on the
/// value-equality predicate `equals_one`, not the canonical `ONE`
/// bit pattern), or either operand is NaN; otherwise it falls
/// through to the f64 path.
#[kani::proof]
fn pow_special_resolution_set() {
    let ai: u8 = kani::any();
    let bi: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    kani::assume(bi < NUM_OPERANDS);
    let a = operand(ai);
    let b = operand(bi);
    let resolved = a.pow_special_only_for_kani(b).is_some();
    let one_base = a.equals_one_for_kani();
    assert!(resolved == (b.is_zero() || one_base || a.is_nan() || b.is_nan()));
}

/// `pow(x, 0) = 1` for every base, including `pow(NaN, 0) = 1`.
#[kani::proof]
fn pow_x_zero_is_one() {
    let ai: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    let a = operand(ai);
    let neg: bool = kani::any();
    let z = if neg {
        Decimal32::NEG_ZERO
    } else {
        Decimal32::ZERO
    };
    let (r, s) = a
        .pow_special_only_for_kani(z)
        .expect("zero exponent resolved by pow_special_cases");
    assert!(r.to_bits() == Decimal32::ONE.to_bits() && !s.invalid());
}

/// `pow(1, y) = 1` for any `y`; an `sNaN` exponent still raises
/// INVALID per §9.2, a quiet NaN or finite exponent does not.
#[kani::proof]
fn pow_one_base_is_one() {
    let (r, s) = Decimal32::ONE
        .pow_special_only_for_kani(Decimal32::SIGNALING_NAN)
        .expect("pow(1, sNaN) resolved by pow_special_cases");
    assert!(r.to_bits() == Decimal32::ONE.to_bits() && s.invalid());

    let (r, s) = Decimal32::ONE
        .pow_special_only_for_kani(Decimal32::NAN)
        .expect("pow(1, qNaN) resolved by pow_special_cases");
    assert!(r.to_bits() == Decimal32::ONE.to_bits() && !s.invalid());

    let (r, s) = Decimal32::ONE
        .pow_special_only_for_kani(Decimal32::MAX)
        .expect("pow(1, finite) resolved by pow_special_cases");
    assert!(r.to_bits() == Decimal32::ONE.to_bits() && !s.invalid());
}

/// §9.2 ties `pow(1, y) = 1` to the *value* `1`, not the canonical
/// cohort. A non-canonical cohort of one (`10 × 10⁻¹`, `100 × 10⁻²`,
/// ..., `10⁶ × 10⁻⁶`, the largest power-of-ten cohort that fits in
/// 7 digits) must short-circuit identically. The bounded `10u32.pow`
/// inside `equals_one` is guarded by `k > 6`, so CBMC unrolls it a
/// fixed number of times.
#[kani::proof]
fn pow_non_canonical_one_cohort_short_circuits() {
    let k: u8 = kani::any();
    kani::assume(k >= 1 && k <= 6);
    let pow10: i32 = match k {
        1 => 10,
        2 => 100,
        3 => 1_000,
        4 => 10_000,
        5 => 100_000,
        _ => 1_000_000,
    };
    let one_cohort = Decimal32::try_new(pow10, -(k as i32)).unwrap();
    // The cohort numerically equals one.
    assert!(one_cohort.equals_one_for_kani());

    // pow(this-cohort-of-1, sNaN) = 1 + INVALID per §9.2.
    let (r, s) = one_cohort
        .pow_special_only_for_kani(Decimal32::SIGNALING_NAN)
        .expect("non-canonical one cohort resolved by pow_special_cases");
    assert!(r.to_bits() == Decimal32::ONE.to_bits() && s.invalid());

    // pow(this-cohort-of-1, qNaN) = 1 + OK.
    let (r, s) = one_cohort
        .pow_special_only_for_kani(Decimal32::NAN)
        .expect("non-canonical one cohort resolved by pow_special_cases");
    assert!(r.to_bits() == Decimal32::ONE.to_bits() && !s.invalid());

    // pow(this-cohort-of-1, finite) = 1 + OK.
    let (r, s) = one_cohort
        .pow_special_only_for_kani(Decimal32::MAX)
        .expect("non-canonical one cohort resolved by pow_special_cases");
    assert!(r.to_bits() == Decimal32::ONE.to_bits() && !s.invalid());
}

/// NaN propagation order (§6.2.3): with a non-zero exponent and a
/// base that is not one, a signaling NaN in either operand raises
/// INVALID; a quiet NaN propagates with OK.
#[kani::proof]
fn pow_nan_propagation() {
    // sNaN base, finite non-one exponent → qNaN + INVALID.
    let (r, s) = Decimal32::SIGNALING_NAN
        .pow_special_only_for_kani(Decimal32::MAX)
        .expect("sNaN base resolved by pow_special_cases");
    assert!(r.is_nan() && s.invalid());

    // Finite non-one base, sNaN exponent → qNaN + INVALID.
    let (r, s) = Decimal32::MAX
        .pow_special_only_for_kani(Decimal32::SIGNALING_NAN)
        .expect("sNaN exponent resolved by pow_special_cases");
    assert!(r.is_nan() && s.invalid());

    // qNaN base, finite non-one exponent → qNaN + OK.
    let (r, s) = Decimal32::NAN
        .pow_special_only_for_kani(Decimal32::MAX)
        .expect("qNaN base resolved by pow_special_cases");
    assert!(r.is_nan() && !s.invalid());
}

/// A non-NaN, non-one base with a non-zero, non-NaN exponent falls
/// through (`None`): the negative-base integer test and the
/// `libm::pow` path are out of scope for the proof.
#[kani::proof]
fn pow_finite_falls_through() {
    // Positive finite base, positive finite exponent.
    assert!(Decimal32::MAX
        .pow_special_only_for_kani(Decimal32::MAX)
        .is_none());
    // Negative finite base: the non-integer-exponent INVALID is on
    // the f64 path, so this is also a fall-through.
    assert!(Decimal32::NEG_ONE
        .pow_special_only_for_kani(Decimal32::MAX)
        .is_none());
}

/// For every non-finite-non-zero operand, `cbrt` resolves in the
/// special-case path (finite non-zero is the only fall-through);
/// `cbrt(±∞) = ±∞`, `cbrt(±0) = ±0` (sign preserved).
#[kani::proof]
fn cbrt_special_resolves_and_signs() {
    let ai: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    let a = operand(ai);
    kani::assume(a.is_nan() || a.is_infinite() || a.is_zero());
    assert!(a.cbrt_special_only_for_kani().is_some());

    let neg: bool = kani::any();
    let (zi, ii) = if neg {
        (Decimal32::NEG_ZERO, Decimal32::NEG_INFINITY)
    } else {
        (Decimal32::ZERO, Decimal32::INFINITY)
    };
    let (rz, _) = zi
        .cbrt_special_only_for_kani()
        .expect("±0 resolved by cbrt_special_cases");
    assert!(rz.is_zero() && rz.is_sign_negative() == neg);
    let (ri, _) = ii
        .cbrt_special_only_for_kani()
        .expect("±∞ resolved by cbrt_special_cases");
    assert!(ri.is_infinite() && ri.is_sign_negative() == neg);
}

/// `cbrt(NaN)` propagates; `cbrt(sNaN)` raises INVALID.
#[kani::proof]
fn cbrt_nan_propagates() {
    let ai: u8 = kani::any();
    kani::assume(ai < NUM_OPERANDS);
    let a = operand(ai);
    kani::assume(a.is_nan());
    let (r, _) = a
        .cbrt_special_only_for_kani()
        .expect("NaN resolved by cbrt_special_cases");
    assert!(r.is_nan());

    let (_, s) = Decimal32::SIGNALING_NAN
        .cbrt_special_only_for_kani()
        .expect("sNaN resolved by cbrt_special_cases");
    assert!(s.invalid());
}
