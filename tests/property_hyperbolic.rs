//! Faithful-rounding cross-check for sinh/cosh/tanh and inverses.
//!
//! sinh / cosh / tanh evaluate `(e^x ± e^{-x}) / 2` end to end at
//! `Extended` (50-digit) precision and round once at the Decimal128
//! boundary; the |x| < 0.5 branches use direct Taylor series (no
//! cancellation). Inverse hyperbolics keep the `ln(x + sqrt(...))`
//! argument at `Extended` precision through the `ln` call. Both
//! deliver ≤ 1 ULP at 34 digits across the supported domain.

#![cfg(feature = "transcendentals")]

use astro_float::{BigFloat, Consts, Radix, RoundingMode as AfRm, Sign};
use ferrodec::{Decimal128, RoundingMode};

fn parse(s: &str) -> Decimal128 {
    Decimal128::parse_str(s, RoundingMode::NearestEven)
        .unwrap()
        .0
}

fn bigfloat_to_decimal_string(v: &BigFloat, cc: &mut Consts, digits: usize) -> String {
    let (sign, mantissa, exp) = v
        .convert_to_radix(Radix::Dec, AfRm::ToEven, cc)
        .expect("convert to decimal");
    if mantissa.is_empty() || mantissa.iter().all(|&d| d == 0) {
        return "0".to_string();
    }
    let take = digits.min(mantissa.len());
    let digit_str: String = mantissa[..take]
        .iter()
        .map(|&d| char::from(b'0' + d))
        .collect();
    let scale = exp - take as i32;
    let sign_str = if matches!(sign, Sign::Neg) { "-" } else { "" };
    format!("{sign_str}{digit_str}e{scale}")
}

fn within_ulps(got: Decimal128, want: Decimal128, ulps: u32) -> bool {
    let (diff, _) = got.sub(want, RoundingMode::NearestEven);
    let diff = diff.abs();
    let abs_want = want.abs();
    if abs_want.is_zero() {
        let bound = parse(&format!("{ulps}e-30"));
        let (cmp, _) = diff.partial_cmp(bound);
        return matches!(
            cmp,
            Some(core::cmp::Ordering::Less | core::cmp::Ordering::Equal)
        );
    }
    let (rel, _) = diff.div(abs_want, RoundingMode::NearestEven);
    let bound = parse(&format!("{ulps}e-33"));
    let (cmp, _) = rel.partial_cmp(bound);
    matches!(
        cmp,
        Some(core::cmp::Ordering::Less | core::cmp::Ordering::Equal)
    )
}

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

const ULPS: u32 = 1;

#[test]
fn sinh_one() {
    check_unary(
        "sinh",
        "1",
        |x| x.sinh(RoundingMode::NearestEven),
        astro_float::BigFloat::sinh,
        ULPS,
    );
}
#[test]
fn sinh_two() {
    check_unary(
        "sinh",
        "2",
        |x| x.sinh(RoundingMode::NearestEven),
        astro_float::BigFloat::sinh,
        ULPS,
    );
}
#[test]
fn sinh_tiny() {
    check_unary(
        "sinh",
        "0.001",
        |x| x.sinh(RoundingMode::NearestEven),
        astro_float::BigFloat::sinh,
        ULPS,
    );
}
#[test]
fn sinh_neg() {
    check_unary(
        "sinh",
        "-1.5",
        |x| x.sinh(RoundingMode::NearestEven),
        astro_float::BigFloat::sinh,
        ULPS,
    );
}

#[test]
fn cosh_one() {
    check_unary(
        "cosh",
        "1",
        |x| x.cosh(RoundingMode::NearestEven),
        astro_float::BigFloat::cosh,
        ULPS,
    );
}
#[test]
fn cosh_two() {
    check_unary(
        "cosh",
        "2",
        |x| x.cosh(RoundingMode::NearestEven),
        astro_float::BigFloat::cosh,
        ULPS,
    );
}
#[test]
fn cosh_tiny() {
    check_unary(
        "cosh",
        "0.001",
        |x| x.cosh(RoundingMode::NearestEven),
        astro_float::BigFloat::cosh,
        ULPS,
    );
}

#[test]
fn tanh_half() {
    check_unary(
        "tanh",
        "0.5",
        |x| x.tanh(RoundingMode::NearestEven),
        astro_float::BigFloat::tanh,
        ULPS,
    );
}
#[test]
fn tanh_one() {
    check_unary(
        "tanh",
        "1",
        |x| x.tanh(RoundingMode::NearestEven),
        astro_float::BigFloat::tanh,
        ULPS,
    );
}
#[test]
fn tanh_three() {
    check_unary(
        "tanh",
        "3",
        |x| x.tanh(RoundingMode::NearestEven),
        astro_float::BigFloat::tanh,
        ULPS,
    );
}

#[test]
fn asinh_one() {
    check_unary(
        "asinh",
        "1",
        |x| x.asinh(RoundingMode::NearestEven),
        astro_float::BigFloat::asinh,
        ULPS,
    );
}
#[test]
fn asinh_huge() {
    check_unary(
        "asinh",
        "1e30",
        |x| x.asinh(RoundingMode::NearestEven),
        astro_float::BigFloat::asinh,
        ULPS,
    );
}
#[test]
fn asinh_tiny() {
    check_unary(
        "asinh",
        "1e-15",
        |x| x.asinh(RoundingMode::NearestEven),
        astro_float::BigFloat::asinh,
        ULPS,
    );
}

#[test]
fn acosh_two() {
    check_unary(
        "acosh",
        "2",
        |x| x.acosh(RoundingMode::NearestEven),
        astro_float::BigFloat::acosh,
        ULPS,
    );
}
#[test]
fn acosh_huge() {
    check_unary(
        "acosh",
        "1e30",
        |x| x.acosh(RoundingMode::NearestEven),
        astro_float::BigFloat::acosh,
        ULPS,
    );
}

#[test]
fn atanh_half() {
    check_unary(
        "atanh",
        "0.5",
        |x| x.atanh(RoundingMode::NearestEven),
        astro_float::BigFloat::atanh,
        ULPS,
    );
}
#[test]
fn atanh_quarter() {
    check_unary(
        "atanh",
        "0.25",
        |x| x.atanh(RoundingMode::NearestEven),
        astro_float::BigFloat::atanh,
        ULPS,
    );
}
#[test]
fn atanh_neg_three_quarter() {
    check_unary(
        "atanh",
        "-0.75",
        |x| x.atanh(RoundingMode::NearestEven),
        astro_float::BigFloat::atanh,
        ULPS,
    );
}
