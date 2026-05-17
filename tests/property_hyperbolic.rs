//! Faithful-rounding contract for sinh/cosh/tanh and inverses vs
//! astro-float, asserted for every IEEE 754 rounding direction
//! (ADR-0021, IEEE 754-2019 §9.2). See `tests/common/mod.rs`; this is
//! not a `± ULP` tolerance envelope.

#![cfg(feature = "hyperbolic")]

use astro_float::{BigFloat, Consts};
use ferrodec::{Decimal128, RoundingMode, Status};
use ferrodec_test_support::transcend_oracle::oracle;

mod common;
use common::{assert_faithful, parse, MODES};

/// Build the shared 256-bit astro-float oracle for the named
/// hyperbolic unary op. Centralising the dispatch here (instead of
/// passing an `astro_float::BigFloat::*` method per call site) keeps
/// the oracle value bit-identical to the pre-DRY helper while removing
/// the local astro-float plumbing, exactly as the trig P3-pre DRY did.
fn oracle_unary(name: &str, exact: &str, cc: &mut Consts) -> BigFloat {
    match name {
        "sinh" => oracle::sinh(exact, cc),
        "cosh" => oracle::cosh(exact, cc),
        "tanh" => oracle::tanh(exact, cc),
        "asinh" => oracle::asinh(exact, cc),
        "acosh" => oracle::acosh(exact, cc),
        "atanh" => oracle::atanh(exact, cc),
        other => panic!("unknown hyperbolic-unary op {other}"),
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

#[test]
fn sinh_one() {
    check_unary("sinh", "1", Decimal128::sinh);
}
#[test]
fn sinh_two() {
    check_unary("sinh", "2", Decimal128::sinh);
}
#[test]
fn sinh_tiny() {
    check_unary("sinh", "0.001", Decimal128::sinh);
}
#[test]
fn sinh_neg() {
    check_unary("sinh", "-1.5", Decimal128::sinh);
}

#[test]
fn cosh_one() {
    check_unary("cosh", "1", Decimal128::cosh);
}
#[test]
fn cosh_two() {
    check_unary("cosh", "2", Decimal128::cosh);
}
#[test]
fn cosh_tiny() {
    check_unary("cosh", "0.001", Decimal128::cosh);
}

#[test]
fn tanh_half() {
    check_unary("tanh", "0.5", Decimal128::tanh);
}
#[test]
fn tanh_one() {
    check_unary("tanh", "1", Decimal128::tanh);
}
#[test]
fn tanh_three() {
    check_unary("tanh", "3", Decimal128::tanh);
}

#[test]
fn asinh_one() {
    check_unary("asinh", "1", Decimal128::asinh);
}
#[test]
fn asinh_huge() {
    check_unary("asinh", "1e30", Decimal128::asinh);
}
#[test]
fn asinh_tiny() {
    check_unary("asinh", "1e-15", Decimal128::asinh);
}

#[test]
fn acosh_two() {
    check_unary("acosh", "2", Decimal128::acosh);
}
#[test]
fn acosh_huge() {
    check_unary("acosh", "1e30", Decimal128::acosh);
}

#[test]
fn atanh_half() {
    check_unary("atanh", "0.5", Decimal128::atanh);
}
#[test]
fn atanh_quarter() {
    check_unary("atanh", "0.25", Decimal128::atanh);
}
#[test]
fn atanh_neg_three_quarter() {
    check_unary("atanh", "-0.75", Decimal128::atanh);
}
