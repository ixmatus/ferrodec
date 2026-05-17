//! Regression pins for fd-9fi — `Decimal32::fma` carried the
//! `fd-7nf` opposite-sign sub-ULP defect family, plus a distinct
//! pre-existing overlap defect the same exact-oracle sweep surfaced.
//!
//! Surfaced by the fd-dpg sibling exact-oracle sweep
//! (`tests/property_fma_oracle.rs`):
//!
//! 1. Tiny opposite-sign product with a dominant same-sign addend
//!    under a directed mode produced a gross magnitude error
//!    (`fma(-1e-101, 1e-101, -1e+27)` `TowardNegative` → `-2e27`
//!    instead of the correctly-rounded `-1.000001e+27`): the dominant
//!    operand was not re-cohorted before the funnel's directed
//!    round-up, so the round landed at the operand's coarse quantum.
//! 2. The early-return dominance test (`shift > safe_shift`) only
//!    checked u128-alignability, not precision overlap, so a product
//!    that overlapped the addend's kept 7-digit window was discarded
//!    into a single sticky bit.
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
fn tiny_opposite_product_dominant_same_sign_addend() {
    // a·b = -1e-202 (tiny, opposite sign to its own factors); c =
    // -1e+27 dominates. The true value is c plus a same-sign sub-ULP
    // residue: `-1.000001e+27` under TowardNegative, never `-2e27`.
    check(dec("-1e-101"), dec("1e-101"), dec("-1e+27"));
}

#[test]
fn tiny_opposite_product_dominant_same_sign_addend_mirrored() {
    // Sign-mirror: positive dominant addend.
    check(dec("-1e-101"), dec("1e-101"), dec("1e+27"));
}

#[test]
fn additive_control_directed_round_lands_at_precision_lsb() {
    // `fma(1E+45, 1E+45, 1E-101)`: exact value `10^90 + 10^-101`,
    // strictly above `10^90` by a ~`10^-185`-ULP same-sign residue.
    // The pre-fix kernel rounded the bare 1-digit coefficient up at
    // its own `10^90` quantum, so TowardPositive doubled to
    // `2.000000E+90`; the correctly-rounded value is `1.000001E+90`
    // (the ULP at 7 significant digits is `10^84`). The unit test
    // `fma_h4_same_sign_additive_control_no_regression` had pinned the
    // buggy `2.000000E+90`; the exact oracle is the arbiter.
    check(dec("1e+45"), dec("1e+45"), dec("1e-101"));
}

#[test]
fn zero_addend_subnormal_product_signals_underflow() {
    // `fma(-5.738903e-42, 5.487024e-55, -0e-101)`: c is zero, so the
    // result is the rebased product `≈ -3.149e-96`, a representable
    // subnormal (adjusted exponent −96 < E_MIN −95). The 14→7 digit
    // precision rounding is inexact; the deeply-subnormal `biased < 0`
    // arm then took an exact 1-digit shift and (pre-fix) failed to
    // carry the incoming INEXACT into UNDERFLOW. IEEE 754-2019 §7.5
    // requires Underflow Inexact Subnormal. Port of the decimal64
    // fd-99f / M1 rule decimal32 lacked.
    check(dec("-5.738903e-42"), dec("5.487024e-55"), dec("-0e-101"));
}
