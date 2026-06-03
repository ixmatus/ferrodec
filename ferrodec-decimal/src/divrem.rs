//! Division and the remainder family: divide, divideInteger, remainder, and
//! remainderNear.
//!
//! The remainder operations share one observation: aligning both operands to
//! `min(ea, eb)` turns them into integers `A` and `B` with `A / B == |a / b|`,
//! so `A div B` is the truncated integer quotient and `A mod B` is the exact
//! residue (already at exponent `min(ea, eb)`, which is the remainder's
//! natural exponent).

use crate::arith::{invalid_nan, nan_result};
use crate::round::round_finite;
use crate::{Context, Decimal, Status};
use core::cmp::Ordering;
use ferrodec_multiword::DecBig;

impl Decimal {
    /// Divide `self` by `other` under `ctx`, correctly rounded.
    #[must_use]
    pub fn divide(&self, other: &Self, ctx: &Context) -> (Decimal, Status) {
        if let Some(r) = nan_result(self, other, ctx) {
            return r;
        }
        let sign = self.is_negative() ^ other.is_negative();
        let (a_inf, b_inf) = (self.is_infinite(), other.is_infinite());
        if a_inf {
            // Infinity over infinity is undefined; over a finite it is infinite.
            return if b_inf {
                (invalid_nan(), Status::INVALID)
            } else {
                (Decimal::infinity(sign), Status::OK)
            };
        }
        if b_inf {
            // Finite over infinity is a signed zero at the floor exponent Etiny.
            // The exponent is constrained to Etiny, so Clamped is signaled (as
            // for any zero whose exponent is tidied into range, independent of
            // ctx.clamp); seed the rounding core with it.
            let etiny = i64::from(ctx.emin) - i64::from(ctx.precision) + 1;
            return round_finite(
                sign,
                DecBig::zero(),
                etiny,
                false,
                etiny,
                ctx,
                Status::CLAMPED,
            );
        }

        let (_, ca, ea) = self.finite_parts().expect("finite");
        let (_, cb, eb) = other.finite_parts().expect("finite");
        let ideal = i64::from(ea) - i64::from(eb);

        if other.is_zero() {
            // Division by zero: undefined for a zero dividend, else infinite.
            return if self.is_zero() {
                (invalid_nan(), Status::INVALID)
            } else {
                (Decimal::infinity(sign), Status::DIV_BY_ZERO)
            };
        }
        if self.is_zero() {
            return round_finite(sign, DecBig::zero(), ideal, false, ideal, ctx, Status::OK);
        }

        // Scale the numerator (or denominator) so the integer quotient lands at
        // precision + 1 digits: one guard digit for the rounding core, with the
        // division remainder feeding the sticky bit.
        let p = i64::from(ctx.precision);
        let da = ca.decimal_digit_count() as i64;
        let db = cb.decimal_digit_count() as i64;
        let shift = (p + 1) - (da - db);
        let (num, den) = if shift >= 0 {
            (ca.mul_pow10(shift as u32), cb.clone())
        } else {
            (ca.clone(), cb.mul_pow10((-shift) as u32))
        };
        let (q, r) = num.div_rem(&den);
        let result_exp = ideal - shift;
        round_finite(sign, q, result_exp, !r.is_zero(), ideal, ctx, Status::OK)
    }

    /// Integer division: the truncated integer part of `self / other`, with
    /// exponent zero.
    #[must_use]
    pub fn divide_integer(&self, other: &Self, ctx: &Context) -> (Decimal, Status) {
        if let Some(r) = nan_result(self, other, ctx) {
            return r;
        }
        let sign = self.is_negative() ^ other.is_negative();
        if self.is_infinite() || other.is_infinite() {
            // Infinity over infinity is undefined; an infinite dividend over a
            // finite divisor is infinite; a finite over infinity is zero.
            return if self.is_infinite() && other.is_infinite() {
                (invalid_nan(), Status::INVALID)
            } else if self.is_infinite() {
                (Decimal::infinity(sign), Status::OK)
            } else {
                (Decimal::finite(sign, DecBig::zero(), 0), Status::OK)
            };
        }
        if other.is_zero() {
            return if self.is_zero() {
                (invalid_nan(), Status::INVALID)
            } else {
                (Decimal::infinity(sign), Status::DIV_BY_ZERO)
            };
        }
        let (q, _r, _min_e, _b) = integer_divide(self, other);
        if q.decimal_digit_count() > u64::from(ctx.precision) {
            // The integer quotient does not fit the precision.
            return (invalid_nan(), Status::INVALID);
        }
        (Decimal::finite(sign, q, 0), Status::OK)
    }

    /// The remainder `self - other * (self divideInteger other)`, with the sign
    /// of `self`.
    #[must_use]
    pub fn remainder(&self, other: &Self, ctx: &Context) -> (Decimal, Status) {
        self.remainder_impl(other, ctx, false)
    }

    /// The remainder nearest to zero: `self - other * round-half-even(self /
    /// other)`. Its magnitude is at most half the divisor's.
    #[must_use]
    pub fn remainder_near(&self, other: &Self, ctx: &Context) -> (Decimal, Status) {
        self.remainder_impl(other, ctx, true)
    }

    fn remainder_impl(&self, other: &Self, ctx: &Context, near: bool) -> (Decimal, Status) {
        if let Some(r) = nan_result(self, other, ctx) {
            return r;
        }
        let sa = self.is_negative();
        // An infinite dividend or a zero divisor is undefined; a finite
        // dividend with an infinite divisor returns the dividend unchanged.
        if self.is_infinite() || other.is_zero() {
            return (invalid_nan(), Status::INVALID);
        }
        if other.is_infinite() {
            // self - Inf*trunc(self/Inf) = self - 0 = self (rounded to context).
            let (_, ca, ea) = self.finite_parts().expect("finite");
            return round_finite(
                sa,
                ca.clone(),
                i64::from(ea),
                false,
                i64::from(ea),
                ctx,
                Status::OK,
            );
        }

        let (q, rem0, min_e, big_b) = integer_divide(self, other);

        if !near {
            if q.decimal_digit_count() > u64::from(ctx.precision) {
                return (invalid_nan(), Status::INVALID);
            }
            return round_finite(sa, rem0, min_e, false, min_e, ctx, Status::OK);
        }

        // remainderNear: round the quotient to nearest, ties to even. If the
        // residue is more than half the divisor (or exactly half with an odd
        // quotient), the nearest quotient is one larger and the residue flips
        // sign to `divisor - residue`.
        let twice = rem0.add(&rem0);
        let q_odd = q.div_rem10().1 & 1 == 1;
        let round_up = match twice.cmp_ref(&big_b) {
            Ordering::Greater => true,
            Ordering::Equal => q_odd,
            Ordering::Less => false,
        };
        let one = DecBig::from_u32(1);
        let (mag, sign, qn) = if round_up {
            (big_b.sub(&rem0), !sa, q.add(&one))
        } else {
            (rem0, sa, q)
        };
        if qn.decimal_digit_count() > u64::from(ctx.precision) {
            return (invalid_nan(), Status::INVALID);
        }
        round_finite(sign, mag, min_e, false, min_e, ctx, Status::OK)
    }
}

/// Align both finite operands to `min(ea, eb)` and integer-divide their
/// magnitudes. Returns `(quotient, remainder, min_e, divisor)` where `quotient
/// = |self| div |other|` (truncated), `remainder = |self| mod |other|`, and
/// `divisor` is `|other|` scaled to `min_e`; the remainder and divisor are
/// both as if at exponent `min_e`.
fn integer_divide(a: &Decimal, b: &Decimal) -> (DecBig, DecBig, i64, DecBig) {
    let (_, ca, ea) = a.finite_parts().expect("finite");
    let (_, cb, eb) = b.finite_parts().expect("finite");
    let min_e = ea.min(eb);
    let big_a = ca.mul_pow10((ea - min_e) as u32);
    let big_b = cb.mul_pow10((eb - min_e) as u32);
    let (q, r) = big_a.div_rem(&big_b);
    (q, r, i64::from(min_e), big_b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Rounding;
    use alloc::string::ToString;

    fn ctx(precision: u32) -> Context {
        Context::new(precision, 9999, -9999, Rounding::HalfEven)
    }

    fn fin(sign: bool, coeff: u128, exp: i32) -> Decimal {
        Decimal::finite(sign, DecBig::from_u128(coeff), exp)
    }

    #[test]
    fn divide_exact_strips_to_ideal() {
        let c = ctx(9);
        // 1 / 1 = 1 (not 1.000...).
        assert_eq!(
            fin(false, 1, 0).divide(&fin(false, 1, 0), &c).0,
            fin(false, 1, 0)
        );
        // 6 / 2 = 3.
        assert_eq!(
            fin(false, 6, 0).divide(&fin(false, 2, 0), &c).0,
            fin(false, 3, 0)
        );
    }

    #[test]
    fn divide_inexact_rounds() {
        // 1 / 3 at precision 9 = 0.333333333.
        let (r, s) = fin(false, 1, 0).divide(&fin(false, 3, 0), &ctx(9));
        assert_eq!(r.to_string(), "0.333333333");
        assert!(s.inexact());
    }

    #[test]
    fn divide_finite_over_infinity_is_clamped_zero() {
        // x / Infinity is a signed zero at Etiny (here -10007), signaling
        // Clamped because the exponent is constrained, with no Underflow or
        // Inexact (the result is exact).
        let c = ctx(9);
        let (d, s) = fin(true, 1000, 0).divide(&Decimal::infinity(false), &c);
        assert!(d.is_zero() && d.is_negative());
        assert!(s.clamped() && !s.underflow() && !s.inexact());
        assert_eq!(d.finite_parts().unwrap().2, -10007);
    }

    #[test]
    fn divide_by_zero() {
        let c = ctx(9);
        let (r, s) = fin(false, 5, 0).divide(&fin(false, 0, 0), &c);
        assert!(r.is_infinite() && !r.is_negative() && s.div_by_zero());
        let (r2, s2) = fin(false, 0, 0).divide(&fin(false, 0, 0), &c);
        assert!(r2.is_nan() && s2.invalid());
    }

    #[test]
    fn divide_integer_and_remainder() {
        let c = ctx(9);
        // 7 // 2 = 3, 7 % 2 = 1.
        assert_eq!(
            fin(false, 7, 0).divide_integer(&fin(false, 2, 0), &c).0,
            fin(false, 3, 0)
        );
        assert_eq!(
            fin(false, 7, 0).remainder(&fin(false, 2, 0), &c).0,
            fin(false, 1, 0)
        );
        // 1 % 0.3 = 0.1 (exponent -1).
        assert_eq!(
            fin(false, 1, 0).remainder(&fin(false, 3, -1), &c).0,
            fin(false, 1, -1)
        );
    }

    #[test]
    fn remainder_near_picks_nearest() {
        let c = ctx(9);
        // remainderNear(10, 3) = 1 (10 = 3*3 + 1, nearest quotient 3).
        assert_eq!(
            fin(false, 10, 0).remainder_near(&fin(false, 3, 0), &c).0,
            fin(false, 1, 0)
        );
        // remainderNear(10, 4) = 2: 10/4 = 2.5, ties to even quotient 2, rem 2.
        assert_eq!(
            fin(false, 10, 0).remainder_near(&fin(false, 4, 0), &c).0,
            fin(false, 2, 0)
        );
        // remainderNear(11, 4) = -1: 11/4 = 2.75 -> 3, 11 - 12 = -1.
        assert_eq!(
            fin(false, 11, 0).remainder_near(&fin(false, 4, 0), &c).0,
            fin(true, 1, 0)
        );
    }
}
