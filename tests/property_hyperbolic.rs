//! Faithful-rounding contract for sinh/cosh/tanh and inverses vs
//! astro-float, asserted for every IEEE 754 rounding direction
//! (ADR-0021, IEEE 754-2019 §9.2). See `tests/common/mod.rs`; this is
//! not a `± ULP` tolerance envelope.

#![cfg(feature = "hyperbolic")]

use astro_float::{BigFloat, Consts, Radix, RoundingMode as AfRm};
use ferrodec::{Decimal128, RoundingMode, Status};

mod common;
use common::{assert_faithful, parse, MODES};

/// Working precision for the astro-float oracle: 256 bits ≈ 77 decimal
/// digits.
const P: usize = 256;

fn check_unary<F, G>(name: &str, x_str: &str, ferrodec_op: F, oracle_op: G)
where
    F: Fn(Decimal128, RoundingMode) -> (Decimal128, Status),
    G: FnOnce(&BigFloat, usize, AfRm, &mut Consts) -> BigFloat,
{
    let x = parse(x_str);
    let exact = format!("{x:e}");
    let mut cc = Consts::new().expect("init consts");
    let xv = BigFloat::parse(&exact, Radix::Dec, P, AfRm::None, &mut cc);
    let oracle = oracle_op(&xv, P, AfRm::None, &mut cc);
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
    check_unary("sinh", "1", Decimal128::sinh, astro_float::BigFloat::sinh);
}
#[test]
fn sinh_two() {
    check_unary("sinh", "2", Decimal128::sinh, astro_float::BigFloat::sinh);
}
#[test]
fn sinh_tiny() {
    check_unary(
        "sinh",
        "0.001",
        Decimal128::sinh,
        astro_float::BigFloat::sinh,
    );
}
#[test]
fn sinh_neg() {
    check_unary(
        "sinh",
        "-1.5",
        Decimal128::sinh,
        astro_float::BigFloat::sinh,
    );
}

#[test]
fn cosh_one() {
    check_unary("cosh", "1", Decimal128::cosh, astro_float::BigFloat::cosh);
}
#[test]
fn cosh_two() {
    check_unary("cosh", "2", Decimal128::cosh, astro_float::BigFloat::cosh);
}
#[test]
fn cosh_tiny() {
    check_unary(
        "cosh",
        "0.001",
        Decimal128::cosh,
        astro_float::BigFloat::cosh,
    );
}

#[test]
fn tanh_half() {
    check_unary("tanh", "0.5", Decimal128::tanh, astro_float::BigFloat::tanh);
}
#[test]
fn tanh_one() {
    check_unary("tanh", "1", Decimal128::tanh, astro_float::BigFloat::tanh);
}
#[test]
fn tanh_three() {
    check_unary("tanh", "3", Decimal128::tanh, astro_float::BigFloat::tanh);
}

#[test]
fn asinh_one() {
    check_unary(
        "asinh",
        "1",
        Decimal128::asinh,
        astro_float::BigFloat::asinh,
    );
}
#[test]
fn asinh_huge() {
    check_unary(
        "asinh",
        "1e30",
        Decimal128::asinh,
        astro_float::BigFloat::asinh,
    );
}
#[test]
fn asinh_tiny() {
    check_unary(
        "asinh",
        "1e-15",
        Decimal128::asinh,
        astro_float::BigFloat::asinh,
    );
}

#[test]
fn acosh_two() {
    check_unary(
        "acosh",
        "2",
        Decimal128::acosh,
        astro_float::BigFloat::acosh,
    );
}
#[test]
fn acosh_huge() {
    check_unary(
        "acosh",
        "1e30",
        Decimal128::acosh,
        astro_float::BigFloat::acosh,
    );
}

#[test]
fn atanh_half() {
    check_unary(
        "atanh",
        "0.5",
        Decimal128::atanh,
        astro_float::BigFloat::atanh,
    );
}
#[test]
fn atanh_quarter() {
    check_unary(
        "atanh",
        "0.25",
        Decimal128::atanh,
        astro_float::BigFloat::atanh,
    );
}
#[test]
fn atanh_neg_three_quarter() {
    check_unary(
        "atanh",
        "-0.75",
        Decimal128::atanh,
        astro_float::BigFloat::atanh,
    );
}
