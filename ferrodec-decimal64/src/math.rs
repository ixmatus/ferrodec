//! High-precision mathematical constants, correctly rounded to
//! Decimal64's 16-digit precision.
//!
//! Each constant is held as a high-precision decimal literal (more
//! digits than Decimal64 carries) and rounded once by
//! [`Decimal64::parse_str`] at [`RoundingMode::NearestEven`], which is
//! correctly rounded. Holding extra digits in the source literal makes
//! the parsed value the correctly rounded 16-digit Decimal64 for any
//! rounding mode, so the value the caller sees is not a truncation of
//! the constant.
//!
//! Each call parses the string fresh. That is measurably cheap (a
//! single fixed-size scan through a few dozen bytes) and avoids the
//! once-cell / static-init complexity that would otherwise leak `std`
//! into a `no_std` crate. Callers that need many uses of `pi()` or
//! `ln10()` should cache the result in a local. Mirrors the `ferrodec`
//! (Decimal128) parent's `math::consts` surface, scaled to Decimal64.

use crate::decimal::Decimal64;
use ferrodec_ieee::RoundingMode;

// Source literals carry well beyond the 16-digit Decimal64 precision so
// the parsed value is the correctly rounded representative, not a
// truncation. Derived by rounding the canonical high-precision values
// (the same references the Decimal128 parent uses) to Decimal64
// precision; the unit tests below re-round an independent
// high-precision literal and confirm the parsed constant agrees.
const PI_STR: &str = "3.14159265358979323846264338327950288";
const E_STR: &str = "2.71828182845904523536028747135266250";
const LN2_STR: &str = "0.693147180559945309417232121458176568";
const LN10_STR: &str = "2.30258509299404568401799145468436421";

/// `π`, correctly rounded to Decimal64 precision.
#[must_use]
pub fn pi() -> Decimal64 {
    parse_const(PI_STR)
}

/// Euler's number `e`, correctly rounded to Decimal64 precision.
#[must_use]
pub fn e() -> Decimal64 {
    parse_const(E_STR)
}

/// `ln(2)`, correctly rounded to Decimal64 precision.
#[must_use]
pub fn ln2() -> Decimal64 {
    parse_const(LN2_STR)
}

/// `ln(10)`, correctly rounded to Decimal64 precision.
#[must_use]
pub fn ln10() -> Decimal64 {
    parse_const(LN10_STR)
}

/// Shared parse helper. The constants are hand-curated, so a parse
/// error is a programmer mistake, not a runtime condition: we `expect`
/// here.
#[inline]
fn parse_const(s: &str) -> Decimal64 {
    Decimal64::parse_str(s, RoundingMode::NearestEven)
        .expect("ferrodec-decimal64 math const literal must parse")
        .0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Re-round an independent high-precision literal of the same
    /// constant and confirm the published constant matches. This checks
    /// correct rounding rather than asserting it: a mistyped digit in
    /// the source literal would round to a different 16-digit value than
    /// the reference and fail here. The reference literals are the
    /// 34-digit values the Decimal128 parent publishes, which round
    /// correctly to 16 digits through the same parser.
    fn rounds_equal(c: Decimal64, reference_high_precision: &str, expected_16dig: Decimal64) {
        let (reference, _) =
            Decimal64::parse_str(reference_high_precision, RoundingMode::NearestEven).unwrap();
        assert_eq!(
            c.partial_cmp(reference).0,
            Some(core::cmp::Ordering::Equal),
            "constant must equal the reference rounded to 16 digits"
        );
        assert_eq!(
            c.partial_cmp(expected_16dig).0,
            Some(core::cmp::Ordering::Equal),
            "constant must equal the explicit 16-digit expected value"
        );
    }

    #[test]
    fn pi_correctly_rounded() {
        // Reference: 34-digit π (Decimal128 parent's literal).
        rounds_equal(
            pi(),
            "3.141592653589793238462643383279503",
            Decimal64::try_new(3_141_592_653_589_793, -15).unwrap(),
        );
    }

    #[test]
    fn e_correctly_rounded() {
        rounds_equal(
            e(),
            "2.718281828459045235360287471352662",
            Decimal64::try_new(2_718_281_828_459_045, -15).unwrap(),
        );
    }

    #[test]
    fn ln2_correctly_rounded() {
        rounds_equal(
            ln2(),
            "0.6931471805599453094172321214581766",
            Decimal64::try_new(6_931_471_805_599_453, -16).unwrap(),
        );
    }

    #[test]
    fn ln10_correctly_rounded() {
        rounds_equal(
            ln10(),
            "2.302585092994045684017991454684364",
            Decimal64::try_new(2_302_585_092_994_046, -15).unwrap(),
        );
    }

    #[test]
    fn constants_in_expected_intervals() {
        // Coarse sanity bounds, independent of the digit-level checks.
        let lt = |a: Decimal64, b: Decimal64| a.partial_cmp(b).0 == Some(core::cmp::Ordering::Less);
        let pi_lo = Decimal64::try_new(314, -2).unwrap();
        let pi_hi = Decimal64::try_new(315, -2).unwrap();
        assert!(lt(pi_lo, pi()) && lt(pi(), pi_hi));
        let e_lo = Decimal64::try_new(271, -2).unwrap();
        let e_hi = Decimal64::try_new(272, -2).unwrap();
        assert!(lt(e_lo, e()) && lt(e(), e_hi));
        assert!(!ln2().is_sign_negative());
        assert!(!ln10().is_sign_negative());
    }
}
