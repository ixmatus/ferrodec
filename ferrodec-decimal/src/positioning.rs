//! General Decimal Arithmetic positioning operations: `shift` and `rotate`.
//!
//! Both move the coefficient digits of the first operand within the context
//! precision by a count given by the second operand. A positive count moves
//! the digits toward the most significant end (a left shift), a negative count
//! toward the least significant end (a right shift). `shift` fills the vacated
//! positions with zeros and discards digits moved off either end; `rotate`
//! wraps the digits around so none are lost. The result keeps the first
//! operand's sign and exponent.
//!
//! The shift count must be an integer with an exponent of zero (so `1.0`, which
//! is numerically integral but carries a `-1` quantum, is rejected) whose
//! magnitude does not exceed the precision. NaN operands propagate as for the
//! arithmetic operations; an infinite first operand with a valid count passes
//! through unchanged. See the General Decimal Arithmetic specification ("shift"
//! and "rotate") and ADR-0041.

use alloc::vec;

use crate::arith::{invalid_nan, nan_result};
use crate::digits::{coeff_to_digits, digits_to_coeff};
use crate::{Context, Decimal, Status};

impl Decimal {
    /// General Decimal Arithmetic `shift`: shift the coefficient digits of
    /// `self` by `other` places within the context precision, filling vacated
    /// positions with zeros and discarding digits moved off either end.
    #[must_use]
    pub fn shift(&self, other: &Self, ctx: &Context) -> (Decimal, Status) {
        positioning(self, other, ctx, false)
    }

    /// General Decimal Arithmetic `rotate`: rotate the coefficient digits of
    /// `self` by `other` places within the context precision, wrapping digits
    /// around so none are lost.
    #[must_use]
    pub fn rotate(&self, other: &Self, ctx: &Context) -> (Decimal, Status) {
        positioning(self, other, ctx, true)
    }
}

fn positioning(a: &Decimal, b: &Decimal, ctx: &Context, wrap: bool) -> (Decimal, Status) {
    // NaN operands propagate first (a quiet NaN takes the OK path, an sNaN
    // signals invalid), before the shift count is examined: `shift NaN -Inf`
    // is the propagated NaN, not an invalid-count error.
    if let Some(r) = nan_result(a, b, ctx) {
        return r;
    }
    // The count must be an integer with a zero exponent whose magnitude is at
    // most the precision; a fraction, a non-zero exponent, an infinity, or an
    // out-of-range magnitude is invalid. This is checked before the first
    // operand's infinity passes through, so `shift Inf Inf` is invalid.
    let Some(n) = shift_count(b, ctx.precision.get()) else {
        return (invalid_nan(), Status::INVALID);
    };
    if a.is_infinite() {
        return (Decimal::infinity(a.is_negative()), Status::OK);
    }
    let (sign, coeff, exp) = a.finite_parts().expect("finite after NaN and infinity");
    let width = ctx.precision.get() as usize;
    let src = coeff_to_digits(coeff, width);
    let p = width as i64;
    let mut out = vec![0u8; width];
    for (j, slot) in out.iter_mut().enumerate() {
        // The digit landing at position `j` comes from `j - n`: a positive `n`
        // pulls from a lower position (a left shift), a negative one from a
        // higher position (a right shift).
        let from = j as i64 - n;
        let idx = if wrap {
            from.rem_euclid(p)
        } else if (0..p).contains(&from) {
            from
        } else {
            continue; // shifted-in zero
        };
        *slot = src[idx as usize];
    }
    (
        Decimal::finite(sign, digits_to_coeff(&out), exp),
        Status::OK,
    )
}

/// Validate the shift / rotate count operand and return it as a signed digit
/// count. The count is finite, has an exponent of zero, and has a magnitude no
/// greater than `precision`. `None` otherwise (a fraction or non-zero exponent,
/// an infinity or NaN, or an out-of-range magnitude).
fn shift_count(b: &Decimal, precision: u32) -> Option<i64> {
    let (sign, coeff, exp) = b.finite_parts()?;
    if exp != 0 {
        return None;
    }
    // A coefficient too large for `u128` is far past `precision`, so a failed
    // conversion is itself an out-of-range count.
    let mag = coeff.to_u128()?;
    if mag > u128::from(precision) {
        return None;
    }
    let n = mag as i64;
    Some(if sign { -n } else { n })
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

    fn parse(s: &str) -> Decimal {
        Decimal::parse_str(s).unwrap()
    }

    #[test]
    fn shift_left_and_right() {
        let c = ctx();
        assert_eq!(parse("1").shift(&parse("2"), &c).0, parse("100"));
        assert_eq!(parse("1").shift(&parse("8"), &c).0, parse("100000000"));
        // Shifted clean off the top at precision 9.
        assert_eq!(parse("1").shift(&parse("9"), &c).0, parse("0"));
        assert_eq!(
            parse("123456789").shift(&parse("-1"), &c).0,
            parse("12345678")
        );
        assert_eq!(parse("123456789").shift(&parse("-8"), &c).0, parse("1"));
        // Shift by zero is the identity.
        assert_eq!(
            parse("123456789").shift(&parse("0"), &c).0,
            parse("123456789")
        );
    }

    #[test]
    fn rotate_wraps() {
        let c = ctx();
        assert_eq!(parse("1").rotate(&parse("2"), &c).0, parse("100"));
        assert_eq!(parse("1").rotate(&parse("-1"), &c).0, parse("100000000"));
        assert_eq!(
            parse("123456789").rotate(&parse("-1"), &c).0,
            parse("912345678")
        );
        // A full rotation is the identity.
        assert_eq!(parse("1").rotate(&parse("9"), &c).0, parse("1"));
        assert_eq!(
            parse("123456789").rotate(&parse("-9"), &c).0,
            parse("123456789")
        );
    }

    #[test]
    fn count_must_be_zero_exponent_integer_in_range() {
        let c = ctx();
        for bad in ["1.5", "1.0", "0.1", "1E+1", "10", "-10", "1000", "Inf"] {
            assert!(
                parse("1").shift(&parse(bad), &c).1.invalid(),
                "count {bad} should be invalid"
            );
            assert!(parse("1").rotate(&parse(bad), &c).1.invalid());
        }
    }

    #[test]
    fn sign_and_exponent_are_preserved() {
        let c = ctx();
        // A zero keeps its sign and exponent through a shift.
        assert_eq!(parse("0E-10").shift(&parse("9"), &c).0, parse("0E-10"));
        assert_eq!(parse("-0E-10").shift(&parse("9"), &c).0, parse("-0E-10"));
        assert_eq!(parse("0E+10").rotate(&parse("-9"), &c).0, parse("0E+10"));
    }

    #[test]
    fn special_value_handling() {
        let c = ctx();
        // An infinite first operand with a valid count passes through.
        assert_eq!(
            Decimal::infinity(false).shift(&parse("-8"), &c).0,
            Decimal::infinity(false)
        );
        assert_eq!(
            Decimal::infinity(true).rotate(&parse("1"), &c).0,
            Decimal::infinity(true)
        );
        // An infinite count is invalid even with an infinite first operand.
        assert!(Decimal::infinity(true)
            .shift(&Decimal::infinity(true), &c)
            .1
            .invalid());
        // A quiet NaN first operand propagates with no flag, even with a count
        // that would otherwise be invalid.
        let (r, s) = Decimal::quiet_nan(false, DecBig::zero()).shift(&Decimal::infinity(false), &c);
        assert!(r.is_nan() && !s.invalid());
        // A signaling NaN signals invalid.
        assert!(Decimal::signaling_nan(false, DecBig::zero())
            .rotate(&parse("1"), &c)
            .1
            .invalid());
    }
}
