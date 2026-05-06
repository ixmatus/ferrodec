//! Kani harnesses for `total_cmp` and `partial_cmp`.
//!
//! `total_cmp` and `partial_cmp` route same-sign finite-vs-finite comparisons
//! through `magnitude_cmp`, which performs `10u128.pow(diff)` scaling. That
//! multiplication is hard for SMT-based solvers when both operands are
//! 128-bit symbolic AND `ea != eb`. The heavy "any-vs-any" antisymmetry over
//! finite-finite different-cohort same-sign pairs is therefore exercised
//! through proptest only.
//!
//! Phase 1 proved the cheap two-input properties — those whose proof does
//! not depend on `magnitude_cmp` — by constraining inputs to classes the
//! SMT layer can dispatch quickly (NaN, Inf, Zero, mixed-sign).
//!
//! Phase 2 (1.7.0) closes the *same-cohort* finite-finite case. When the
//! biased exponents match, `magnitude_cmp` skips the `pow10` scaling
//! entirely (line 327 of `src/cmp.rs`: `Ordering::Equal => ca.cmp(&cb)`).
//! That path is SMT-friendly. The remaining gap — finite-finite,
//! same-sign, *different* `biased_exp` — still needs proptest.

use core::cmp::Ordering;

use crate::bid::{classify_bits, Class};
use crate::Decimal128;

/// `total_cmp` is reflexive: `total_cmp(a, a) == Equal` for every bit pattern.
#[kani::proof]
fn total_cmp_reflexive() {
    let bits: u128 = kani::any();
    let d = Decimal128::from_bits(bits);
    assert!(d.total_cmp(d) == Ordering::Equal);
}

/// `total_cmp` is antisymmetric on the *non-finite* portion of the domain
/// (at least one operand is NaN, ±Inf, or ±0). This avoids the
/// `magnitude_cmp` path, which would explode SMT runtime.
#[kani::proof]
fn total_cmp_antisymmetric_off_finite_finite() {
    let bits_a: u128 = kani::any();
    let bits_b: u128 = kani::any();
    let a = Decimal128::from_bits(bits_a);
    let b = Decimal128::from_bits(bits_b);
    kani::assume(!a.is_finite() || a.is_zero() || !b.is_finite() || b.is_zero());

    let ab = a.total_cmp(b);
    let ba = b.total_cmp(a);
    assert!(ab == ba.reverse());
}

/// `partial_cmp` returns `None` if and only if at least one input is NaN.
/// Restricted to non-magnitude paths (mixed signs, infinities, zeros, NaNs);
/// the same-sign finite branch is covered by proptest.
#[kani::proof]
fn partial_cmp_none_iff_nan_off_finite_finite() {
    let bits_a: u128 = kani::any();
    let bits_b: u128 = kani::any();
    let a = Decimal128::from_bits(bits_a);
    let b = Decimal128::from_bits(bits_b);
    kani::assume(!a.is_finite() || a.is_zero() || !b.is_finite() || b.is_zero());

    let (ord, _status) = a.partial_cmp(b);
    let any_nan = a.is_nan() || b.is_nan();
    assert!(ord.is_none() == any_nan);
}

/// `total_cmp` is antisymmetric on the finite-finite same-cohort
/// same-sign domain — i.e. when both operands are finite, non-zero,
/// non-NaN, share a sign, and have identical `biased_exp`. In this
/// regime `magnitude_cmp`'s scale check is short-circuited at
/// `ea.cmp(&eb) == Equal`, so the comparison reduces to a direct
/// `ca.cmp(&cb)` with no `pow10` multiplication; SMT can dispatch the
/// whole case.
///
/// The remaining domain — finite-finite, same-sign, *different*
/// `biased_exp` — exercises the symbolic-multiplication path that
/// blocks SMT termination, and stays proptest-only.
#[kani::proof]
fn total_cmp_antisymmetric_finite_same_cohort_same_sign() {
    let a_bits: u128 = kani::any();
    let b_bits: u128 = kani::any();
    let a = Decimal128::from_bits(a_bits);
    let b = Decimal128::from_bits(b_bits);
    kani::assume(a.is_finite() && !a.is_zero() && !a.is_nan());
    kani::assume(b.is_finite() && !b.is_zero() && !b.is_nan());

    let a_bexp = match classify_bits(a.to_bits()) {
        Class::Finite { biased_exp, .. } => biased_exp,
        _ => return,
    };
    let b_bexp = match classify_bits(b.to_bits()) {
        Class::Finite { biased_exp, .. } => biased_exp,
        _ => return,
    };
    kani::assume(a_bexp == b_bexp);
    kani::assume(a.is_sign_negative() == b.is_sign_negative());

    let ab = a.total_cmp(b);
    let ba = b.total_cmp(a);
    assert!(ab == ba.reverse());
}

/// `partial_cmp` raises `INVALID` if and only if at least one input is a
/// signaling NaN. This property doesn't depend on `magnitude_cmp` even on
/// finite-finite inputs, but the early-exit logic is identical.
#[kani::proof]
fn partial_cmp_invalid_iff_signaling() {
    let bits_a: u128 = kani::any();
    let bits_b: u128 = kani::any();
    let a = Decimal128::from_bits(bits_a);
    let b = Decimal128::from_bits(bits_b);
    kani::assume(!a.is_finite() || a.is_zero() || !b.is_finite() || b.is_zero());

    let (_ord, status) = a.partial_cmp(b);
    let any_snan = a.is_signaling_nan() || b.is_signaling_nan();
    assert!(status.invalid() == any_snan);
}
