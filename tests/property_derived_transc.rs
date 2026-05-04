//! Faithful-rounding cross-check for the derived transcendentals
//! (`log2`, `exp2`, `cbrt`, `tan`) vs astro-float.

#![cfg(feature = "transcendentals")]

use astro_float::{BigFloat, Consts, Radix, RoundingMode as AfRm, Sign};
use ferrodec::{Decimal128, RoundingMode};

fn parse(s: &str) -> Decimal128 {
    Decimal128::parse_str(s, RoundingMode::NearestEven).unwrap().0
}

fn oracle_apply<F>(x_str: &str, f: F) -> String
where
    F: FnOnce(&BigFloat, usize, AfRm, &mut Consts) -> BigFloat,
{
    let p = 220;
    let rm = AfRm::None;
    let mut cc = Consts::new().expect("init consts");
    let x = BigFloat::parse(x_str, Radix::Dec, p, rm, &mut cc);
    let r = f(&x, p, rm, &mut cc);
    bigfloat_to_decimal_string(&r, &mut cc, 50)
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
    let want_str = oracle_apply(&exact, oracle_op);
    let want = parse(&want_str);
    assert!(
        within_ulps(got, want, ulps),
        "{name}({exact}): got {got:?}, want {want:?} (oracle {want_str})"
    );
}

// log2 -------------------------------------------------------------------

#[test]
fn log2_two() {
    check_unary("log2", "2",
        |x| x.log2(RoundingMode::NearestEven),
        |x, p, rm, cc| x.log2(p, rm, cc), 1);
}
#[test]
fn log2_eight() {
    check_unary("log2", "8",
        |x| x.log2(RoundingMode::NearestEven),
        |x, p, rm, cc| x.log2(p, rm, cc), 1);
}
#[test]
fn log2_half() {
    check_unary("log2", "0.5",
        |x| x.log2(RoundingMode::NearestEven),
        |x, p, rm, cc| x.log2(p, rm, cc), 1);
}
#[test]
fn log2_pi() {
    check_unary("log2", "3.14159265358979323846264338327950288",
        |x| x.log2(RoundingMode::NearestEven),
        |x, p, rm, cc| x.log2(p, rm, cc), 1);
}
#[test]
fn log2_huge() {
    check_unary("log2", "1e1000",
        |x| x.log2(RoundingMode::NearestEven),
        |x, p, rm, cc| x.log2(p, rm, cc), 1);
}

// exp2 -------------------------------------------------------------------
//
// astro-float has no `exp2`; compute the oracle via `pow(2, x)`.

fn check_exp2(x_str: &str, ulps: u32) {
    let x = parse(x_str);
    let exact = format!("{x}");
    let (got, _) = x.exp2(RoundingMode::NearestEven);
    let p = 220;
    let rm = AfRm::None;
    let mut cc = Consts::new().expect("init consts");
    let two = BigFloat::from_word(2, p);
    let xv = BigFloat::parse(&exact, Radix::Dec, p, rm, &mut cc);
    let want_bf = two.pow(&xv, p, rm, &mut cc);
    let want_str = bigfloat_to_decimal_string(&want_bf, &mut cc, 50);
    let want = parse(&want_str);
    assert!(
        within_ulps(got, want, ulps),
        "exp2({exact}): got {got:?}, want {want:?} (oracle {want_str})"
    );
}

#[test] fn exp2_zero() { check_exp2("0", 1); }
#[test] fn exp2_one() { check_exp2("1", 1); }
#[test] fn exp2_ten() { check_exp2("10", 1); }
#[test] fn exp2_neg_one() { check_exp2("-1", 1); }
#[test] fn exp2_pi() { check_exp2("3.14159265358979323846264338327950288", 1); }

// cbrt -------------------------------------------------------------------

fn check_cbrt(x_str: &str, ulps: u32) {
    let x = parse(x_str);
    let exact = format!("{x}");
    let (got, _) = x.cbrt(RoundingMode::NearestEven);
    let p = 220;
    let rm = AfRm::None;
    let mut cc = Consts::new().expect("init consts");
    let xv = BigFloat::parse(&exact, Radix::Dec, p, rm, &mut cc);
    let want_bf = xv.cbrt(p, rm);
    let want_str = bigfloat_to_decimal_string(&want_bf, &mut cc, 50);
    let want = parse(&want_str);
    assert!(
        within_ulps(got, want, ulps),
        "cbrt({exact}): got {got:?}, want {want:?} (oracle {want_str})"
    );
}

#[test] fn cbrt_two() { check_cbrt("2", 1); }
#[test] fn cbrt_minus_two() { check_cbrt("-2", 1); }
#[test] fn cbrt_huge() { check_cbrt("1e90", 1); }
#[test] fn cbrt_tiny() { check_cbrt("1e-90", 1); }

// tan --------------------------------------------------------------------

#[test]
fn tan_zero() {
    let (r, _) = parse("0").tan(RoundingMode::NearestEven);
    assert!(r.is_zero());
}
#[test]
fn tan_pi_over_four() {
    check_unary("tan", "0.7853981633974483096156608458198757",
        |x| x.tan(RoundingMode::NearestEven),
        |x, p, rm, cc| x.tan(p, rm, cc), 1);
}
#[test]
fn tan_one() {
    check_unary("tan", "1",
        |x| x.tan(RoundingMode::NearestEven),
        |x, p, rm, cc| x.tan(p, rm, cc), 1);
}
#[test]
fn tan_minus_one() {
    check_unary("tan", "-1",
        |x| x.tan(RoundingMode::NearestEven),
        |x, p, rm, cc| x.tan(p, rm, cc), 1);
}
#[test]
fn tan_pi() {
    // tan(π) ≈ 0 — but the rounded π input gives a tiny non-zero
    // residual. Check that we're within ULPs of astro-float's view.
    check_unary("tan", "3.141592653589793238462643383279503",
        |x| x.tan(RoundingMode::NearestEven),
        |x, p, rm, cc| x.tan(p, rm, cc), 1);
}
