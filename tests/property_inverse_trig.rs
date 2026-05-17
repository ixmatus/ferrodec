//! Faithful-rounding contract for atan / asin / acos / atan2 vs
//! astro-float, asserted for every IEEE 754 rounding direction
//! (ADR-0021, IEEE 754-2019 §9.2). See `tests/common/mod.rs`; this is
//! not a `± ULP` tolerance envelope.

#![cfg(feature = "trig")]

use astro_float::{BigFloat, Consts};
use ferrodec::{Decimal128, RoundingMode, Status};
use ferrodec_test_support::transcend_oracle::oracle;

mod common;
use common::{assert_faithful, parse, MODES};

/// Build the shared 256-bit astro-float oracle for the named inverse
/// unary op. Centralising the dispatch here (instead of passing an
/// `astro_float::BigFloat::*` method per call site) keeps the oracle
/// value bit-identical to the pre-DRY helper while removing the local
/// astro-float plumbing, exactly as the exp-log P2-pre DRY did.
fn oracle_unary(name: &str, exact: &str, cc: &mut Consts) -> BigFloat {
    match name {
        "atan" => oracle::atan(exact, cc),
        "asin" => oracle::asin(exact, cc),
        "acos" => oracle::acos(exact, cc),
        other => panic!("unknown inverse-unary op {other}"),
    }
}

fn check_unary<F>(name: &str, x_str: &str, ferrodec_op: F)
where
    F: Fn(Decimal128, RoundingMode) -> (Decimal128, Status),
{
    let x = parse(x_str);
    let exact = format!("{x:e}");
    let mut cc = Consts::new().expect("init consts");
    let oracle = oracle_unary(name, &exact, &mut cc);
    for &rm in MODES {
        let (got, status) = ferrodec_op(x, rm);
        assert_faithful(
            got,
            status,
            &oracle,
            &mut cc,
            rm,
            &format!("{name}({exact})"),
        );
    }
}

// atan -------------------------------------------------------------------

#[test]
fn atan_one() {
    check_unary("atan", "1", Decimal128::atan);
}
#[test]
fn atan_two() {
    check_unary("atan", "2", Decimal128::atan);
}
#[test]
fn atan_huge() {
    check_unary("atan", "1e30", Decimal128::atan);
}
#[test]
fn atan_tiny() {
    check_unary("atan", "1e-30", Decimal128::atan);
}
#[test]
fn atan_half() {
    check_unary("atan", "0.5", Decimal128::atan);
}
#[test]
fn atan_pi() {
    check_unary(
        "atan",
        "3.14159265358979323846264338327950288",
        Decimal128::atan,
    );
}

// asin -------------------------------------------------------------------

#[test]
fn asin_half() {
    check_unary("asin", "0.5", Decimal128::asin);
}
#[test]
fn asin_neg_half() {
    check_unary("asin", "-0.5", Decimal128::asin);
}
#[test]
fn asin_near_one() {
    check_unary("asin", "0.999", Decimal128::asin);
}
#[test]
fn asin_tiny() {
    check_unary("asin", "1e-15", Decimal128::asin);
}

// acos -------------------------------------------------------------------

#[test]
fn acos_half() {
    check_unary("acos", "0.5", Decimal128::acos);
}
#[test]
fn acos_quarter() {
    check_unary("acos", "0.25", Decimal128::acos);
}
#[test]
fn acos_neg_half() {
    check_unary("acos", "-0.5", Decimal128::acos);
}

// atan2 ------------------------------------------------------------------

fn check_atan2(y_str: &str, x_str: &str) {
    let y = parse(y_str);
    let x = parse(x_str);
    let exact_y = format!("{y:e}");
    let exact_x = format!("{x:e}");

    // astro-float has no atan2; the shared builder synthesizes it via
    // atan(y/x) + quadrant, the exact construction this helper used
    // before the P3-pre DRY. The sign bits come from the parsed
    // `Decimal128` values so the quadrant decision is unchanged.
    let mut cc = Consts::new().expect("init consts");
    let oracle = oracle::atan2(
        &exact_y,
        &exact_x,
        y.is_sign_negative(),
        x.is_sign_negative(),
        &mut cc,
    );
    for &rm in MODES {
        let (got, status) = y.atan2(x, rm);
        assert_faithful(
            got,
            status,
            &oracle,
            &mut cc,
            rm,
            &format!("atan2({exact_y}, {exact_x})"),
        );
    }
}

#[test]
fn atan2_one_one() {
    check_atan2("1", "1");
}
#[test]
fn atan2_one_two() {
    check_atan2("1", "2");
}
#[test]
fn atan2_neg_one_neg_two() {
    check_atan2("-1", "-2");
}
#[test]
fn atan2_three_four() {
    check_atan2("3", "4");
}
#[test]
fn atan2_neg_one_one() {
    check_atan2("-1", "1");
}
