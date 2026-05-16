//! Regression pins for fd-7nf — FMA `fma_ab_dom_in_range_eff_sub`
//! correctness on the opposite-sign sub-ULP residue shape.
//!
//! Surfaced by the S3 exact-oracle migration (the prior 1-ULP
//! astro-float test used central-band operands and never checked
//! status, so it could not see any of these). Three facets, all in
//! `src/ops/fma.rs`:
//!
//! 1. `digits(cab) ≤ PRECISION`: the dominant product was packed via
//!    raw `pack_finite` with no exponent clamp, so a representable
//!    value whose quantum exceeds `qmax` (e.g. `1×10^6112`)
//!    debug-asserted / mis-encoded. Now clamped (pad trailing zeros,
//!    lower the exponent) like `round_and_pack_finite`.
//! 2. `digits(cab) > PRECISION`, product divides evenly: the result
//!    was reported exact (no `INEXACT`) although the non-zero
//!    opposite-sign residue makes the true value never exact.
//! 3. Same shape, directional modes: the value was off by one ULP —
//!    the true magnitude is `kept − epsilon`, which truncating /
//!    floor modes must round to the lower neighbour.
//!
//! Each case is asserted bit-for-bit against the exact correctly-
//! rounded oracle.

#![cfg(feature = "fmt")]

use ferrodec::{Decimal128, RoundingMode};
use ferrodec_test_support::conformance::status_conformance_eq;
use ferrodec_test_support::oracle::{self, parse_decimal, Expect, Format};

const MODES: &[RoundingMode] = &[
    RoundingMode::NearestEven,
    RoundingMode::NearestAway,
    RoundingMode::TowardZero,
    RoundingMode::TowardPositive,
    RoundingMode::TowardNegative,
];

fn matches(got: Decimal128, want: &Expect) -> bool {
    match want {
        Expect::Nan => got.is_nan(),
        Expect::Infinity { neg } => got.is_infinite() && got.is_sign_negative() == *neg,
        Expect::Finite { neg, coeff, exp } => {
            got.is_finite() && {
                let (n, c, e) = oracle::decode_decimal128(got.to_bits());
                n == *neg && c == *coeff && e == *exp
            }
        }
    }
}

/// `a.fma(b, c, rm)` equals the exact correctly-rounded result,
/// bit-for-bit and status-for-status, for every rounding mode.
fn check(a: Decimal128, b: Decimal128, c: Decimal128) {
    for &rm in MODES {
        let (got, gs) = a.fma(b, c, rm);
        let da = parse_decimal(&format!("{a:e}")).unwrap();
        let db = parse_decimal(&format!("{b:e}")).unwrap();
        let dc = parse_decimal(&format!("{c:e}")).unwrap();
        let r = oracle::fma(&da, &db, &dc, Format::DECIMAL128, rm);
        assert!(
            matches(got, &r.value),
            "value fma({a:e}, {b:e}, {c:e}) rm={rm:?}: got {got:e}, oracle {}",
            r.decimal_string()
        );
        assert!(
            status_conformance_eq(gs, r.status),
            "status fma({a:e}, {b:e}, {c:e}) rm={rm:?}: got {gs:?}, oracle {:?}",
            r.status
        );
    }
}

#[test]
fn facet1_dominant_product_quantum_past_qmax_is_clamped_not_panicked() {
    // a·b = -1×10^6112 (representable; quantum 6112 > qmax 6111 so it
    // must be re-encoded as 10^33 × 10^6079, not packed raw).
    let a = Decimal128::from_bits(0x22084000000000000000000000000001); // 1e1
    let b = Decimal128::from_bits(0xC3FFC000000000000000000000000001); // -1e6111
    let c = Decimal128::from_bits(0x00000000000000000000000000000001); // 1e-6176
    check(a, b, c);
}

#[test]
fn facet23_evenly_dividing_product_with_opposite_sign_subulp_residue() {
    // a·b has a 36-digit coefficient that divides evenly to 34 digits;
    // c is a non-zero, opposite-sign, far sub-ULP residue. INEXACT
    // must be raised, and TowardZero must pick the lower neighbour.
    let a = Decimal128::from_bits(0xA1EF000000000000000000000000028A); // -6.50E-98
    let b = Decimal128::from_bits(0x21EF1606154A9F2A7D907DD6FB19B442); // 4.466…E-68
    let c = Decimal128::from_bits(0x00000000000000000000000000000001); // 1e-6176
    check(a, b, c);
}
