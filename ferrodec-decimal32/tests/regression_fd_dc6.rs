//! Regression pin for fd-dc6 — `Decimal32::fma` subnormal
//! double-rounding.
//!
//! Surfaced by the fd-dpg exact-oracle sweep
//! (`tests/property_fma_oracle.rs`). `round_and_pack_finite` rounded
//! the exact product to PRECISION first, then `finalise_finite`'s
//! `biased < 0` arm rounded a *second* time into the subnormal
//! quantum. A residue that landed strictly above the subnormal tie
//! but below the PRECISION tie was collapsed by the first rounding,
//! so the second saw an exact tie and rounded the wrong way:
//!
//! `fma(3.142290e-17, -2.033196e-78, 5.38890e-95)` `NearestEven`
//! gave `-9.99992e-96` where the correctly-rounded value is
//! `-9.99991e-96`.
//!
//! The fix is the sibling analogue of the parent `Decimal128` fd-42l
//! single-rounding restructure: drop to the wider of the PRECISION
//! and subnormal-quantum requirements in one rounding.
//!
//! Each case is asserted bit-for-bit and status-for-status against
//! the exact correctly-rounded oracle for every rounding mode.

#![cfg(feature = "fmt")]

use ferrodec_decimal32::{Decimal32, RoundingMode};
use ferrodec_test_support::conformance::status_conformance_eq;
use ferrodec_test_support::oracle::{self, decode_decimal32, parse_decimal, Expect, Format};

const MODES: &[RoundingMode] = &[
    RoundingMode::NearestEven,
    RoundingMode::NearestAway,
    RoundingMode::TowardZero,
    RoundingMode::TowardPositive,
    RoundingMode::TowardNegative,
];

fn matches(got: Decimal32, want: &Expect) -> bool {
    match want {
        Expect::Nan => got.is_nan(),
        Expect::Infinity { neg } => got.is_infinite() && got.is_sign_negative() == *neg,
        Expect::Finite { neg, coeff, exp } => {
            got.is_finite() && {
                let (n, c, e) = decode_decimal32(got.to_bits());
                n == *neg && c == *coeff && e == *exp
            }
        }
    }
}

fn check(a: Decimal32, b: Decimal32, c: Decimal32) {
    for &rm in MODES {
        let (got, gs) = a.fma(b, c, rm);
        let da = parse_decimal(&format!("{a:e}")).unwrap();
        let db = parse_decimal(&format!("{b:e}")).unwrap();
        let dc = parse_decimal(&format!("{c:e}")).unwrap();
        let r = oracle::fma(&da, &db, &dc, Format::DECIMAL32, rm);
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

fn dec(s: &str) -> Decimal32 {
    Decimal32::parse_str(s, RoundingMode::NearestEven)
        .unwrap()
        .0
}

#[test]
fn subnormal_product_residue_above_subnormal_tie() {
    // The fd-dpg minimal failing input. The opposite-sign product
    // dominates the addend; the result is subnormal and the exact
    // residue sits strictly above the subnormal tie. A single correct
    // rounding keeps `…91`; the old double rounding tied it to `…92`.
    check(dec("3142290E-23"), dec("-2033196E-84"), dec("538890E-100"));
}

#[test]
fn subnormal_product_residue_above_subnormal_tie_negated() {
    // Sign-mirror: the directed modes round the other way, still
    // single-rounded against the exact residue.
    check(dec("-3142290E-23"), dec("-2033196E-84"), dec("538890E-100"));
}
