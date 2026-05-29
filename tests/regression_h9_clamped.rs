#![cfg(feature = "fmt")]
//! H9 regression (Decimal128): genuine in-operation quantum clamps
//! raise the IEEE 754-2019 §7.4 informational Clamped flag, matching
//! the ferrodec-decimal64 / ferrodec-decimal32 siblings (their
//! `regression_h9_clamped`). The value is exact in every case; only
//! the flag is asserted. The conformance harness masks Clamped
//! (informational; the deferred ideal-exponent cases are tracked by
//! fd-61r), so these live as unit-level regressions rather than
//! conformance vectors.

use ferrodec::{Decimal128, RoundingMode};

#[test]
fn div_finite_over_inf_is_clamped() {
    // dqdiv / dddiv788 shape: -1000 / Inf -> -0E-398 Clamped. Infinity
    // has no quantum, so the ideal exponent is unboundedly negative and
    // clamps to Etiny.
    let (r, s) = Decimal128::try_new(-1000, 0)
        .unwrap()
        .div(Decimal128::INFINITY, RoundingMode::NearestEven);
    assert!(r.is_zero() && r.is_sign_negative(), "got {r}");
    assert!(s.clamped(), "x / Inf should raise Clamped, got {s:?}");
}

#[test]
fn zero_with_out_of_range_preferred_quantum_is_clamped() {
    // A zero whose preferred quantum exceeds the format quantum range
    // is clamped into range; the zero is exact at every exponent.
    let (r, s) = Decimal128::parse_str("0E+6200", RoundingMode::NearestEven).unwrap();
    assert!(r.is_zero(), "got {r}");
    assert!(s.clamped(), "0E+6200 should clamp the quantum, got {s:?}");
}

#[test]
fn fma_zero_product_clamped_quantum() {
    // Zero product (a = 0) with addend zero, where the preferred
    // quantum min(q_ab, q_c) falls below Etiny and is clamped.
    let a = Decimal128::parse_str("0E-6000", RoundingMode::NearestEven)
        .unwrap()
        .0;
    let b = Decimal128::parse_str("1E-6000", RoundingMode::NearestEven)
        .unwrap()
        .0;
    let c = Decimal128::parse_str("0E-6000", RoundingMode::NearestEven)
        .unwrap()
        .0;
    let (r, s) = a.fma(b, c, RoundingMode::NearestEven);
    assert!(r.is_zero(), "got {r}");
    assert!(
        s.clamped(),
        "fma zero-product clamp should raise Clamped, got {s:?}"
    );
}
