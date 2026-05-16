//! Faithful-rounding contract for the derived transcendentals
//! (`log2`, `exp2`, `cbrt`, `tan`) vs astro-float, asserted for every
//! IEEE 754 rounding direction (ADR-0021, IEEE 754-2019 §9.2). See
//! `tests/common/mod.rs`; this is not a `± ULP` tolerance envelope.

#![cfg(feature = "transcendentals")]

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

// log2 -------------------------------------------------------------------

#[test]
fn log2_two() {
    check_unary("log2", "2", Decimal128::log2, astro_float::BigFloat::log2);
}
#[test]
fn log2_eight() {
    check_unary("log2", "8", Decimal128::log2, astro_float::BigFloat::log2);
}
#[test]
fn log2_half() {
    check_unary("log2", "0.5", Decimal128::log2, astro_float::BigFloat::log2);
}
#[test]
fn log2_pi() {
    check_unary(
        "log2",
        "3.14159265358979323846264338327950288",
        Decimal128::log2,
        astro_float::BigFloat::log2,
    );
}
#[test]
fn log2_huge() {
    check_unary(
        "log2",
        "1e1000",
        Decimal128::log2,
        astro_float::BigFloat::log2,
    );
}

// exp2 -------------------------------------------------------------------
//
// astro-float has no `exp2`; compute the oracle via `pow(2, x)`.

fn check_exp2(x_str: &str) {
    let x = parse(x_str);
    let exact = format!("{x:e}");
    let mut cc = Consts::new().expect("init consts");
    let two = BigFloat::from_word(2, P);
    let xv = BigFloat::parse(&exact, Radix::Dec, P, AfRm::None, &mut cc);
    let oracle = two.pow(&xv, P, AfRm::None, &mut cc);
    for &rm in MODES {
        let (got, status) = x.exp2(rm);
        assert_faithful(got, status, &oracle, &mut cc, rm, &format!("exp2({exact})"));
    }
}

#[test]
fn exp2_zero() {
    check_exp2("0");
}
#[test]
fn exp2_one() {
    check_exp2("1");
}
#[test]
fn exp2_ten() {
    check_exp2("10");
}
#[test]
fn exp2_neg_one() {
    check_exp2("-1");
}
#[test]
fn exp2_pi() {
    check_exp2("3.14159265358979323846264338327950288");
}

// cbrt -------------------------------------------------------------------

fn check_cbrt(x_str: &str) {
    let x = parse(x_str);
    let exact = format!("{x:e}");
    let mut cc = Consts::new().expect("init consts");
    let xv = BigFloat::parse(&exact, Radix::Dec, P, AfRm::None, &mut cc);
    let oracle = xv.cbrt(P, AfRm::None);
    for &rm in MODES {
        let (got, status) = x.cbrt(rm);
        assert_faithful(got, status, &oracle, &mut cc, rm, &format!("cbrt({exact})"));
    }
}

#[test]
fn cbrt_two() {
    check_cbrt("2");
}
#[test]
fn cbrt_minus_two() {
    check_cbrt("-2");
}
#[test]
fn cbrt_huge() {
    check_cbrt("1e90");
}
#[test]
fn cbrt_tiny() {
    check_cbrt("1e-90");
}

// tan --------------------------------------------------------------------

#[test]
fn tan_zero() {
    // tan(0) = 0 exactly, for every rounding direction.
    for &rm in MODES {
        let (r, status) = parse("0").tan(rm);
        assert!(r.is_zero(), "tan(0) rm={rm:?}: got {r:?}");
        assert!(!status.invalid(), "tan(0) rm={rm:?}: raised INVALID");
    }
}
#[test]
fn tan_pi_over_four() {
    check_unary(
        "tan",
        "0.7853981633974483096156608458198757",
        Decimal128::tan,
        astro_float::BigFloat::tan,
    );
}
#[test]
fn tan_one() {
    check_unary("tan", "1", Decimal128::tan, astro_float::BigFloat::tan);
}
#[test]
fn tan_minus_one() {
    check_unary("tan", "-1", Decimal128::tan, astro_float::BigFloat::tan);
}
#[test]
fn tan_pi() {
    // tan(π) ≈ 0; the rounded π input gives a tiny non-zero residual.
    check_unary(
        "tan",
        "3.141592653589793238462643383279503",
        Decimal128::tan,
        astro_float::BigFloat::tan,
    );
}
