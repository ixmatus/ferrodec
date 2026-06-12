//! Digit-wise General Decimal Arithmetic logical operations: `and`, `or`,
//! `xor`, and `invert`.
//!
//! Each operand is treated as a string of zeros and ones aligned at the units
//! position, and the operation is applied digit by digit within the context
//! precision. A logical operand must be a non-negative integer with an exponent
//! of zero whose digits are all `0` or `1`; the result is a non-negative
//! integer with an exponent of zero.
//!
//! These diverge from every other operation in their NaN handling. A NaN is
//! not a valid logical operand, so *every* NaN raises `Invalid_operation`,
//! signaling or not, and (unlike the arithmetic operations) does not propagate:
//! the result is a plain default `NaN` with no payload or sign, exactly as for
//! any other invalid operand (an infinity, a sign, a non-zero exponent, or a
//! digit other than `0` or `1`). See the General Decimal Arithmetic
//! specification ("Logical operations") and ADR-0041.

use alloc::vec;
use alloc::vec::Vec;

use crate::arith::invalid_nan;
use crate::digits::{coeff_to_digits, digits_to_coeff};
use crate::{Context, Decimal, Status};

impl Decimal {
    /// General Decimal Arithmetic `and`: the digit-wise logical AND of two
    /// logical operands, within the context precision.
    #[must_use]
    pub fn and(&self, other: &Self, ctx: &Context) -> (Decimal, Status) {
        logical_binary(self, other, ctx, |a, b| a & b)
    }

    /// General Decimal Arithmetic `or`: the digit-wise logical OR.
    #[must_use]
    pub fn or(&self, other: &Self, ctx: &Context) -> (Decimal, Status) {
        logical_binary(self, other, ctx, |a, b| a | b)
    }

    /// General Decimal Arithmetic `xor`: the digit-wise logical exclusive OR.
    #[must_use]
    pub fn xor(&self, other: &Self, ctx: &Context) -> (Decimal, Status) {
        logical_binary(self, other, ctx, |a, b| a ^ b)
    }

    /// General Decimal Arithmetic `invert`: the digit-wise logical inversion of
    /// a single operand, complementing all `precision` digits.
    #[must_use]
    pub fn invert(&self, ctx: &Context) -> (Decimal, Status) {
        let Some(digits) = as_logical_digits(self, ctx.precision.get()) else {
            return (invalid_nan(), Status::INVALID);
        };
        let inverted: Vec<u8> = digits.iter().map(|&d| 1 - d).collect();
        (
            Decimal::finite(false, digits_to_coeff(&inverted), 0),
            Status::OK,
        )
    }
}

/// Shared binary logical kernel: handle the NaN cases, validate both operands,
/// then apply `op` digit by digit over the low `precision` digits.
fn logical_binary(
    a: &Decimal,
    b: &Decimal,
    ctx: &Context,
    op: fn(u8, u8) -> u8,
) -> (Decimal, Status) {
    // Any invalid logical operand (a NaN of either kind, an infinity, a sign, a
    // non-zero exponent, or a non-0/1 digit) yields the default NaN and signals
    // invalid; no NaN payload propagates.
    let (Some(da), Some(db)) = (
        as_logical_digits(a, ctx.precision.get()),
        as_logical_digits(b, ctx.precision.get()),
    ) else {
        return (invalid_nan(), Status::INVALID);
    };
    let out: Vec<u8> = da.iter().zip(&db).map(|(&x, &y)| op(x, y)).collect();
    (Decimal::finite(false, digits_to_coeff(&out), 0), Status::OK)
}

/// Validate `d` as a logical operand and return its low `precision` digits,
/// least significant first, zero-padded to exactly `precision`. A logical
/// operand is finite, non-negative, has an exponent of zero, and every digit of
/// its whole coefficient (not only the precision window) is `0` or `1`.
/// `None` if any of those fail (including any special value).
fn as_logical_digits(d: &Decimal, precision: u32) -> Option<Vec<u8>> {
    let (sign, coeff, exp) = d.finite_parts()?;
    if sign || exp != 0 {
        return None;
    }
    let full = coeff.decimal_digit_count() as usize;
    let all = coeff_to_digits(coeff, full);
    if all.iter().any(|&digit| digit > 1) {
        return None;
    }
    // Take the low `precision` digits the operation works on (truncating any
    // higher digits, padding with zeros when the operand is shorter).
    let width = precision as usize;
    let mut out = vec![0u8; width];
    let keep = width.min(all.len());
    out[..keep].copy_from_slice(&all[..keep]);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Rounding;
    use ferrodec_multiword::DecBig;

    fn ctx() -> Context {
        Context::new(
            core::num::NonZeroU32::new(9).unwrap(),
            999,
            -999,
            Rounding::HalfEven,
        )
    }

    fn fin(coeff: u128, exp: i32) -> Decimal {
        Decimal::finite(false, DecBig::from_u128(coeff), exp)
    }

    fn parse(s: &str) -> Decimal {
        Decimal::parse_str(s).unwrap()
    }

    #[test]
    fn and_or_xor_basic() {
        let c = ctx();
        assert_eq!(fin(1100, 0).and(&fin(1010, 0), &c).0, fin(1000, 0));
        assert_eq!(fin(1100, 0).or(&fin(1010, 0), &c).0, fin(1110, 0));
        assert_eq!(fin(1100, 0).xor(&fin(1010, 0), &c).0, fin(110, 0));
    }

    #[test]
    fn over_length_truncates_to_precision() {
        // 111111111111 (12 ones) AND 111111111 (9) -> 111111111 at precision 9.
        let c = ctx();
        let r = parse("111111111111").and(&parse("111111111"), &c).0;
        assert_eq!(r, fin(111_111_111, 0));
    }

    #[test]
    fn invert_pads_and_complements_precision_digits() {
        let c = ctx();
        // invert 1 -> 111111110 (pad 1 to nine digits, complement all nine).
        assert_eq!(parse("1").invert(&c).0, fin(111_111_110, 0));
        // invert 0 -> all ones; invert all ones -> 0.
        assert_eq!(parse("0").invert(&c).0, fin(111_111_111, 0));
        assert_eq!(parse("111111111").invert(&c).0, fin(0, 0));
    }

    #[test]
    fn non_logical_operand_is_invalid() {
        let c = ctx();
        // A non-0/1 digit, a sign, and a non-zero exponent each invalidate.
        assert!(fin(2, 0).and(&fin(1, 0), &c).1.invalid());
        assert!(parse("-1").and(&parse("1"), &c).1.invalid());
        assert!(parse("1.0").and(&parse("1"), &c).1.invalid()); // exp -1
        assert!(parse("1E+1").invert(&c).1.invalid()); // exp +1
        assert!(Decimal::infinity(false).and(&fin(1, 0), &c).1.invalid());
        for r in [fin(2, 0).and(&fin(1, 0), &c).0, parse("1.0").invert(&c).0] {
            assert!(r.is_nan());
        }
    }

    #[test]
    fn every_nan_yields_default_nan_and_invalid() {
        let c = ctx();
        let default_nan = Decimal::quiet_nan(false, DecBig::zero());
        // A quiet NaN with a payload and sign does not propagate: the result is
        // the default NaN (positive, no payload) and invalid is raised.
        let (r, s) = Decimal::quiet_nan(true, DecBig::from_u32(123)).and(&fin(1, 0), &c);
        assert_eq!(r, default_nan);
        assert!(s.invalid());
        // A signaling NaN likewise collapses to the default NaN.
        let (r, s) = fin(1, 0).xor(&Decimal::signaling_nan(false, DecBig::from_u32(7)), &c);
        assert_eq!(r, default_nan);
        assert!(!r.is_signaling_nan() && s.invalid());
        // Unary invert of a payloaded NaN: default NaN, invalid.
        let (r, s) = Decimal::quiet_nan(false, DecBig::from_u32(9)).invert(&c);
        assert_eq!(r, default_nan);
        assert!(s.invalid());
    }
}
