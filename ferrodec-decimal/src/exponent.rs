//! General Decimal Arithmetic exponent operations: `scaleb` and `logb`.
//!
//! `scaleb(x, n)` multiplies `x` by `10^n` by adding the integer `n` to the
//! exponent, leaving the coefficient untouched, then rounds to the context.
//! `logb(x)` returns the adjusted exponent of `x` (the power of ten of its most
//! significant digit) as an integer, rounded to the context. See the General
//! Decimal Arithmetic specification ("scaleb" and "logb") and ADR-0041.

use crate::arith::{invalid_nan, nan_result, nan_unary};
use crate::round::round_finite;
use crate::{Context, Decimal, Status};
use ferrodec_multiword::DecBig;

impl Decimal {
    /// General Decimal Arithmetic `scaleb`: `self` multiplied by `10^other`.
    /// `other` must be an integer with an exponent of zero whose magnitude is at
    /// most `2 * (emax + precision)`; otherwise the result is invalid. The
    /// coefficient is unchanged and the exponent is shifted, then the result is
    /// rounded to the context (overflowing to infinity or underflowing to a
    /// subnormal as usual).
    #[must_use]
    pub fn scaleb(&self, other: &Self, ctx: &Context) -> (Decimal, Status) {
        if let Some(r) = nan_result(self, other, ctx) {
            return r;
        }
        let Some(n) = scaleb_amount(other, ctx) else {
            return (invalid_nan(), Status::INVALID);
        };
        if self.is_infinite() {
            return (Decimal::infinity(self.is_negative()), Status::OK);
        }
        let (sign, coeff, exp) = self.finite_parts().expect("finite after NaN and infinity");
        let new_exp = i64::from(exp) + n;
        round_finite(
            sign,
            coeff.clone(),
            new_exp,
            false,
            new_exp,
            ctx,
            Status::OK,
        )
    }

    /// General Decimal Arithmetic `logb`: the adjusted exponent of `self` as an
    /// integer, rounded to the context. A zero returns `-Infinity` and signals
    /// division by zero; an infinity returns `+Infinity`; a NaN propagates.
    #[must_use]
    pub fn logb(&self, ctx: &Context) -> (Decimal, Status) {
        if let Some(r) = nan_unary(self, ctx) {
            return r;
        }
        if self.is_infinite() {
            // logb of either infinity is +Infinity.
            return (Decimal::infinity(false), Status::OK);
        }
        let (_, coeff, exp) = self.finite_parts().expect("finite after NaN and infinity");
        if coeff.is_zero() {
            return (Decimal::infinity(true), Status::DIV_BY_ZERO);
        }
        let adj = i64::from(exp) + coeff.decimal_digit_count() as i64 - 1;
        let mag = DecBig::from_u128(u128::from(adj.unsigned_abs()));
        round_finite(adj < 0, mag, 0, false, 0, ctx, Status::OK)
    }
}

/// Validate the `scaleb` amount operand and return it as a signed exponent
/// shift. It must be finite, have an exponent of zero, and have a magnitude no
/// greater than `2 * (emax + precision)`. `None` otherwise.
fn scaleb_amount(b: &Decimal, ctx: &Context) -> Option<i64> {
    let (sign, coeff, exp) = b.finite_parts()?;
    if exp != 0 {
        return None;
    }
    let mag = coeff.to_u128()?;
    let bound = 2 * (i64::from(ctx.emax) + i64::from(ctx.precision));
    if mag > bound as u128 {
        return None;
    }
    let n = mag as i64;
    Some(if sign { -n } else { n })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Rounding;
    use alloc::string::ToString;

    fn ctx() -> Context {
        Context::new(9, 999, -999, Rounding::HalfEven)
    }

    fn parse(s: &str) -> Decimal {
        Decimal::parse_str(s).unwrap()
    }

    #[test]
    fn scaleb_shifts_exponent_keeping_cohort() {
        let c = ctx();
        // 7.50 (coefficient 750, exponent -2) scaled by 3 keeps the coefficient.
        assert_eq!(
            parse("7.50").scaleb(&parse("3"), &c).0.to_string(),
            "7.50E+3"
        );
        assert_eq!(
            parse("-7.50").scaleb(&parse("3"), &c).0.to_string(),
            "-7.50E+3"
        );
    }

    #[test]
    fn scaleb_range_and_overflow() {
        let c = ctx(); // bound = 2 * (999 + 9) = 2016
                       // In range but overflowing rounds to infinity with overflow.
        let (r, s) = parse("1.23").scaleb(&parse("2016"), &c);
        assert!(r.is_infinite() && s.overflow() && s.inexact());
        // Just out of range is invalid, not an overflow.
        let (r, s) = parse("1.23").scaleb(&parse("2017"), &c);
        assert!(r.is_nan() && s.invalid() && !s.overflow());
        // A non-integer or non-zero-exponent amount is invalid.
        assert!(parse("1.23").scaleb(&parse("1.0"), &c).1.invalid());
        assert!(parse("1.23").scaleb(&parse("1.5"), &c).1.invalid());
    }

    #[test]
    fn scaleb_specials() {
        let c = ctx();
        // An infinite first operand with a valid amount passes through.
        assert_eq!(
            Decimal::infinity(false).scaleb(&parse("5"), &c).0,
            Decimal::infinity(false)
        );
        // An infinite amount is invalid.
        assert!(parse("10")
            .scaleb(&Decimal::infinity(false), &c)
            .1
            .invalid());
        // A NaN propagates its sign and raises nothing (quiet) or invalid (sNaN).
        let (r, s) = Decimal::quiet_nan(true, DecBig::zero()).scaleb(&parse("1"), &c);
        assert!(r.is_nan() && r.is_negative() && !s.invalid());
        assert!(Decimal::signaling_nan(false, DecBig::zero())
            .scaleb(&parse("1"), &c)
            .1
            .invalid());
    }

    #[test]
    fn logb_adjusted_exponent() {
        let c = ctx();
        assert_eq!(parse("1").logb(&c).0, parse("0"));
        assert_eq!(parse("1000").logb(&c).0, parse("3"));
        assert_eq!(parse("0.001").logb(&c).0, parse("-3"));
        assert_eq!(parse("268268268").logb(&c).0, parse("8"));
    }

    #[test]
    fn logb_specials() {
        let c = ctx();
        // Zero of any cohort: -Infinity and division by zero.
        let (r, s) = parse("0").logb(&c);
        assert_eq!(r, Decimal::infinity(true));
        assert!(s.div_by_zero());
        assert_eq!(parse("0.0000").logb(&c).0, Decimal::infinity(true));
        // Either infinity: +Infinity, no flag.
        assert_eq!(Decimal::infinity(true).logb(&c).0, Decimal::infinity(false));
        assert_eq!(
            Decimal::infinity(false).logb(&c).0,
            Decimal::infinity(false)
        );
        // NaN propagates.
        assert!(Decimal::quiet_nan(false, DecBig::zero())
            .logb(&c)
            .0
            .is_nan());
    }

    #[test]
    fn logb_rounds_to_narrow_context() {
        // At precision 2 a three-digit exponent is rounded.
        let c = Context::new(2, 999, -999, Rounding::HalfEven);
        let (r, s) = parse("1E+999").logb(&c);
        assert_eq!(r.to_string(), "1.0E+3");
        assert!(s.inexact());
    }
}
