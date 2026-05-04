//! High-precision mathematical constants.
//!
//! Each constant is held as a 36-digit decimal literal — two extra
//! digits beyond the 34-digit Decimal128 precision so the parsed value
//! is correctly rounded for any rounding mode.
//!
//! Each call parses the string fresh. That's measurably cheap (a
//! single fixed-size scan through ~40 bytes) and avoids the
//! once-cell / static-init complexity that would otherwise leak `std`
//! into a `no_std` crate. Callers that need many uses of `pi()` or
//! `ln10()` should cache the result in a local.

use crate::decimal::Decimal128;
use crate::status::RoundingMode;

const PI_STR: &str = "3.14159265358979323846264338327950288";
const E_STR: &str = "2.71828182845904523536028747135266250";
const LN2_STR: &str = "0.693147180559945309417232121458176568";
const LN10_STR: &str = "2.30258509299404568401799145468436421";

/// `π` to 35 significant digits, rounded to Decimal128 precision.
#[must_use]
pub fn pi() -> Decimal128 {
    parse_const(PI_STR)
}

/// Euler's number `e` to 35 significant digits.
#[must_use]
pub fn e() -> Decimal128 {
    parse_const(E_STR)
}

/// `ln(2)` to 35 significant digits.
#[must_use]
pub fn ln2() -> Decimal128 {
    parse_const(LN2_STR)
}

/// `ln(10)` to 35 significant digits.
#[must_use]
pub fn ln10() -> Decimal128 {
    parse_const(LN10_STR)
}

/// Shared parse helper. The constants are hand-curated, so a parse
/// error is a programmer mistake, not a runtime condition — we
/// `expect` here.
#[inline]
fn parse_const(s: &str) -> Decimal128 {
    Decimal128::parse_str(s, RoundingMode::NearestEven)
        .expect("ferrodec math const literal must parse")
        .0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pi_is_about_three_point_one_four() {
        let p = pi();
        // 3.14 < pi < 3.15
        let three_fourteen = Decimal128::parse_str("3.14", RoundingMode::default())
            .unwrap()
            .0;
        let three_fifteen = Decimal128::parse_str("3.15", RoundingMode::default())
            .unwrap()
            .0;
        assert_eq!(
            p.partial_cmp(three_fourteen).0,
            Some(core::cmp::Ordering::Greater)
        );
        assert_eq!(
            p.partial_cmp(three_fifteen).0,
            Some(core::cmp::Ordering::Less)
        );
    }

    #[test]
    fn e_is_about_two_point_seven_one_eight() {
        let v = e();
        let two_seven = Decimal128::parse_str("2.71", RoundingMode::default())
            .unwrap()
            .0;
        let two_eight = Decimal128::parse_str("2.72", RoundingMode::default())
            .unwrap()
            .0;
        assert_eq!(
            v.partial_cmp(two_seven).0,
            Some(core::cmp::Ordering::Greater)
        );
        assert_eq!(v.partial_cmp(two_eight).0, Some(core::cmp::Ordering::Less));
    }

    #[test]
    fn ln_constants_are_positive() {
        assert!(!ln2().is_sign_negative());
        assert!(!ln10().is_sign_negative());
    }

    #[test]
    fn parse_doesnt_panic() {
        // Touch all four to confirm none of the literals regress.
        let _ = pi();
        let _ = e();
        let _ = ln2();
        let _ = ln10();
    }
}
