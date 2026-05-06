//! Faithful-rounding cross-check for atan / asin / acos / atan2 vs astro-float.

#![cfg(feature = "trig")]

use astro_float::{BigFloat, Consts, Radix, RoundingMode as AfRm};
use ferrodec::{Decimal128, RoundingMode};

mod common;
use common::{bigfloat_to_decimal_string, parse, within_ulps};

fn check_unary<F, G>(name: &str, x_str: &str, ferrodec_op: F, oracle_op: G, ulps: u32)
where
    F: FnOnce(Decimal128) -> (Decimal128, ferrodec::Status),
    G: FnOnce(&BigFloat, usize, AfRm, &mut Consts) -> BigFloat,
{
    let x = parse(x_str);
    let exact = format!("{x}");
    let (got, _) = ferrodec_op(x);
    let p = 220;
    let rm = AfRm::None;
    let mut cc = Consts::new().expect("init consts");
    let xv = BigFloat::parse(&exact, Radix::Dec, p, rm, &mut cc);
    let want_bf = oracle_op(&xv, p, rm, &mut cc);
    let want_str = bigfloat_to_decimal_string(&want_bf, &mut cc, 50);
    let want = parse(&want_str);
    assert!(
        within_ulps(got, want, ulps),
        "{name}({exact}): got {got:?}, want {want:?} (oracle {want_str})"
    );
}

// atan -------------------------------------------------------------------

#[test]
fn atan_one() {
    check_unary(
        "atan",
        "1",
        |x| x.atan(RoundingMode::NearestEven),
        astro_float::BigFloat::atan,
        1,
    );
}
#[test]
fn atan_two() {
    check_unary(
        "atan",
        "2",
        |x| x.atan(RoundingMode::NearestEven),
        astro_float::BigFloat::atan,
        1,
    );
}
#[test]
fn atan_huge() {
    check_unary(
        "atan",
        "1e30",
        |x| x.atan(RoundingMode::NearestEven),
        astro_float::BigFloat::atan,
        1,
    );
}
#[test]
fn atan_tiny() {
    check_unary(
        "atan",
        "1e-30",
        |x| x.atan(RoundingMode::NearestEven),
        astro_float::BigFloat::atan,
        1,
    );
}
#[test]
fn atan_half() {
    check_unary(
        "atan",
        "0.5",
        |x| x.atan(RoundingMode::NearestEven),
        astro_float::BigFloat::atan,
        1,
    );
}
#[test]
fn atan_pi() {
    check_unary(
        "atan",
        "3.14159265358979323846264338327950288",
        |x| x.atan(RoundingMode::NearestEven),
        astro_float::BigFloat::atan,
        1,
    );
}

// asin -------------------------------------------------------------------

#[test]
fn asin_half() {
    check_unary(
        "asin",
        "0.5",
        |x| x.asin(RoundingMode::NearestEven),
        astro_float::BigFloat::asin,
        1,
    );
}
#[test]
fn asin_neg_half() {
    check_unary(
        "asin",
        "-0.5",
        |x| x.asin(RoundingMode::NearestEven),
        astro_float::BigFloat::asin,
        1,
    );
}
#[test]
fn asin_near_one() {
    check_unary(
        "asin",
        "0.999",
        |x| x.asin(RoundingMode::NearestEven),
        astro_float::BigFloat::asin,
        1,
    );
}
#[test]
fn asin_tiny() {
    check_unary(
        "asin",
        "1e-15",
        |x| x.asin(RoundingMode::NearestEven),
        astro_float::BigFloat::asin,
        1,
    );
}

// acos -------------------------------------------------------------------

#[test]
fn acos_half() {
    check_unary(
        "acos",
        "0.5",
        |x| x.acos(RoundingMode::NearestEven),
        astro_float::BigFloat::acos,
        1,
    );
}
#[test]
fn acos_quarter() {
    check_unary(
        "acos",
        "0.25",
        |x| x.acos(RoundingMode::NearestEven),
        astro_float::BigFloat::acos,
        1,
    );
}
#[test]
fn acos_neg_half() {
    check_unary(
        "acos",
        "-0.5",
        |x| x.acos(RoundingMode::NearestEven),
        astro_float::BigFloat::acos,
        1,
    );
}

// atan2 ------------------------------------------------------------------

fn check_atan2(y_str: &str, x_str: &str, ulps: u32) {
    let y = parse(y_str);
    let x = parse(x_str);
    let exact_y = format!("{y}");
    let exact_x = format!("{x}");
    let (got, _) = y.atan2(x, RoundingMode::NearestEven);

    // astro-float has no atan2; synthesize via atan(y/x) + quadrant.
    let p = 220;
    let rm = AfRm::None;
    let mut cc = Consts::new().expect("init consts");
    let yv = BigFloat::parse(&exact_y, Radix::Dec, p, rm, &mut cc);
    let xv = BigFloat::parse(&exact_x, Radix::Dec, p, rm, &mut cc);
    let pi_bf = cc.pi(p, rm);
    let q = yv.div(&xv, p, rm);
    let mut want_bf = q.atan(p, rm, &mut cc);
    if x.is_sign_negative() {
        if y.is_sign_negative() {
            want_bf = want_bf.sub(&pi_bf, p, rm);
        } else {
            want_bf = want_bf.add(&pi_bf, p, rm);
        }
    }
    let want_str = bigfloat_to_decimal_string(&want_bf, &mut cc, 50);
    let want = parse(&want_str);
    assert!(
        within_ulps(got, want, ulps),
        "atan2({exact_y}, {exact_x}): got {got:?}, want {want:?} (oracle {want_str})"
    );
}

#[test]
fn atan2_one_one() {
    check_atan2("1", "1", 1);
}
#[test]
fn atan2_one_two() {
    check_atan2("1", "2", 1);
}
#[test]
fn atan2_neg_one_neg_two() {
    check_atan2("-1", "-2", 1);
}
#[test]
fn atan2_three_four() {
    check_atan2("3", "4", 1);
}
#[test]
fn atan2_neg_one_one() {
    check_atan2("-1", "1", 1);
}
