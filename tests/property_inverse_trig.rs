//! Faithful-rounding contract for atan / asin / acos / atan2 vs
//! astro-float, asserted for every IEEE 754 rounding direction
//! (ADR-0021, IEEE 754-2019 §9.2). See `tests/common/mod.rs`; this is
//! not a `± ULP` tolerance envelope.

#![cfg(feature = "trig")]

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

// atan -------------------------------------------------------------------

#[test]
fn atan_one() {
    check_unary("atan", "1", Decimal128::atan, astro_float::BigFloat::atan);
}
#[test]
fn atan_two() {
    check_unary("atan", "2", Decimal128::atan, astro_float::BigFloat::atan);
}
#[test]
fn atan_huge() {
    check_unary(
        "atan",
        "1e30",
        Decimal128::atan,
        astro_float::BigFloat::atan,
    );
}
#[test]
fn atan_tiny() {
    check_unary(
        "atan",
        "1e-30",
        Decimal128::atan,
        astro_float::BigFloat::atan,
    );
}
#[test]
fn atan_half() {
    check_unary("atan", "0.5", Decimal128::atan, astro_float::BigFloat::atan);
}
#[test]
fn atan_pi() {
    check_unary(
        "atan",
        "3.14159265358979323846264338327950288",
        Decimal128::atan,
        astro_float::BigFloat::atan,
    );
}

// asin -------------------------------------------------------------------

#[test]
fn asin_half() {
    check_unary("asin", "0.5", Decimal128::asin, astro_float::BigFloat::asin);
}
#[test]
fn asin_neg_half() {
    check_unary(
        "asin",
        "-0.5",
        Decimal128::asin,
        astro_float::BigFloat::asin,
    );
}
#[test]
fn asin_near_one() {
    check_unary(
        "asin",
        "0.999",
        Decimal128::asin,
        astro_float::BigFloat::asin,
    );
}
#[test]
fn asin_tiny() {
    check_unary(
        "asin",
        "1e-15",
        Decimal128::asin,
        astro_float::BigFloat::asin,
    );
}

// acos -------------------------------------------------------------------

#[test]
fn acos_half() {
    check_unary("acos", "0.5", Decimal128::acos, astro_float::BigFloat::acos);
}
#[test]
fn acos_quarter() {
    check_unary(
        "acos",
        "0.25",
        Decimal128::acos,
        astro_float::BigFloat::acos,
    );
}
#[test]
fn acos_neg_half() {
    check_unary(
        "acos",
        "-0.5",
        Decimal128::acos,
        astro_float::BigFloat::acos,
    );
}

// atan2 ------------------------------------------------------------------

fn check_atan2(y_str: &str, x_str: &str) {
    let y = parse(y_str);
    let x = parse(x_str);
    let exact_y = format!("{y:e}");
    let exact_x = format!("{x:e}");

    // astro-float has no atan2; synthesize via atan(y/x) + quadrant.
    let mut cc = Consts::new().expect("init consts");
    let yv = BigFloat::parse(&exact_y, Radix::Dec, P, AfRm::None, &mut cc);
    let xv = BigFloat::parse(&exact_x, Radix::Dec, P, AfRm::None, &mut cc);
    let pi_bf = cc.pi(P, AfRm::None);
    let q = yv.div(&xv, P, AfRm::None);
    let mut oracle = q.atan(P, AfRm::None, &mut cc);
    if x.is_sign_negative() {
        if y.is_sign_negative() {
            oracle = oracle.sub(&pi_bf, P, AfRm::None);
        } else {
            oracle = oracle.add(&pi_bf, P, AfRm::None);
        }
    }
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
