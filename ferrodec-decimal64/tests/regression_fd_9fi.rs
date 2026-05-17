//! Regression pin for fd-9fi — `Decimal64::fma` carried the
//! `fd-7nf` opposite-sign sub-ULP defect family.
//!
//! Surfaced by the fd-dpg sibling exact-oracle sweep
//! (`tests/property_fma_oracle.rs`). A tiny opposite-sign product
//! with a dominant same-sign addend under a directed rounding mode
//! produced a gross magnitude error (≈ doubling), not the 1-ULP
//! astro-float envelope the old test would have masked:
//!
//! `fma(1e-398, -1e-398, -1e+114)` `TowardNegative` → `-2e114`
//! instead of the correctly-rounded `-1.000000000000001e+114`.
//!
//! Each case is asserted bit-for-bit and status-for-status against
//! the exact correctly-rounded oracle for every rounding mode.

#![cfg(feature = "fmt")]

use ferrodec_decimal64::{Decimal64, RoundingMode};
use ferrodec_test_support::conformance::status_conformance_eq;
use ferrodec_test_support::oracle::{self, decode_decimal64, parse_decimal, Expect, Format};

const MODES: &[RoundingMode] = &[
    RoundingMode::NearestEven,
    RoundingMode::NearestAway,
    RoundingMode::TowardZero,
    RoundingMode::TowardPositive,
    RoundingMode::TowardNegative,
];

fn matches(got: Decimal64, want: &Expect) -> bool {
    match want {
        Expect::Nan => got.is_nan(),
        Expect::Infinity { neg } => got.is_infinite() && got.is_sign_negative() == *neg,
        Expect::Finite { neg, coeff, exp } => {
            got.is_finite() && {
                let (n, c, e) = decode_decimal64(got.to_bits());
                n == *neg && c == *coeff && e == *exp
            }
        }
    }
}

fn check(a: Decimal64, b: Decimal64, c: Decimal64) {
    for &rm in MODES {
        let (got, gs) = a.fma(b, c, rm);
        let da = parse_decimal(&format!("{a:e}")).unwrap();
        let db = parse_decimal(&format!("{b:e}")).unwrap();
        let dc = parse_decimal(&format!("{c:e}")).unwrap();
        let r = oracle::fma(&da, &db, &dc, Format::DECIMAL64, rm);
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

fn dec(s: &str) -> Decimal64 {
    Decimal64::parse_str(s, RoundingMode::NearestEven)
        .unwrap()
        .0
}

#[test]
fn tiny_opposite_product_dominant_same_sign_addend() {
    // a·b = -1e-796 (tiny, opposite sign to its own factors); c =
    // -1e+114 dominates. The true value is c plus a sub-ULP same-sign
    // residue: `-1.000000000000001e+114` under TowardNegative, never
    // `-2e114`.
    check(dec("1e-398"), dec("-1e-398"), dec("-1e+114"));
}

#[test]
fn tiny_opposite_product_dominant_same_sign_addend_mirrored() {
    // Sign-mirror of the reproducer: positive dominant addend.
    check(dec("1e-398"), dec("-1e-398"), dec("1e+114"));
}
