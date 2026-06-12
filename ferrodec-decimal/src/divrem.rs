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
            let etiny = i64::from(ctx.emin) - i64::from(ctx.precision.get()) + 1;
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
        let p = i64::from(ctx.precision.get());
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
        if self.is_zero() {
            return (Decimal::finite(sign, DecBig::zero(), 0), Status::OK);
        }
        // Decide what the alignment inside integer_divide could possibly
        // produce BEFORE paying for it (ADR-0053): the quotient carries at
        // least adj(self) - adj(other) digits, so a difference above the
        // precision is INVALID, and a negative difference means
        // |self| < |other| with a zero quotient. Only the digit-bounded
        // band between them needs the real division.
        let adj_gap = adjusted_exponent_gap(self, other);
        if adj_gap > i64::from(ctx.precision.get()) {
            return (invalid_nan(), Status::INVALID);
        }
        if adj_gap < 0 {
            return (Decimal::finite(sign, DecBig::zero(), 0), Status::OK);
        }
        let (q, _r, _min_e, _b) = integer_divide(self, other);
        if q.decimal_digit_count() > u64::from(ctx.precision.get()) {
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

        // The same pre-alignment screen as divide_integer (ADR-0053). A
        // dividend whose adjusted exponent sits at least two below the
        // divisor's is its own remainder even for remainderNear (the residue
        // is below a tenth of the divisor, so the nearest quotient is zero);
        // exactly one below can still flip around half the divisor, and the
        // alignment gap there is bounded by the operands' digit counts.
        let (_, ca, ea) = self.finite_parts().expect("finite");
        let (_, _, eb) = other.finite_parts().expect("finite");
        let min_e_short = i64::from(ea.min(eb));
        if self.is_zero() {
            return round_finite(
                sa,
                DecBig::zero(),
                min_e_short,
                false,
                min_e_short,
                ctx,
                Status::OK,
            );
        }
        let adj_gap = adjusted_exponent_gap(self, other);
        if adj_gap > i64::from(ctx.precision.get()) {
            return (invalid_nan(), Status::INVALID);
        }
        if adj_gap < if near { -1 } else { 0 } {
            return round_finite(
                sa,
                ca.clone(),
                i64::from(ea),
                false,
                min_e_short,
                ctx,
                Status::OK,
            );
        }

        let (q, rem0, min_e, big_b) = integer_divide(self, other);

        if !near {
            if q.decimal_digit_count() > u64::from(ctx.precision.get()) {
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
        if qn.decimal_digit_count() > u64::from(ctx.precision.get()) {
            return (invalid_nan(), Status::INVALID);
        }
        round_finite(sign, mag, min_e, false, min_e, ctx, Status::OK)
    }
}

/// `adj(a) - adj(b)` for two finite nonzero operands, where the adjusted
/// exponent `exponent + digits - 1` locates the most significant digit. The
/// integer quotient `|a| div |b|` carries at least this many digits when it
/// is positive, and is zero when it is negative; callers screen on it before
/// paying for the alignment inside [`integer_divide`] (ADR-0053).
fn adjusted_exponent_gap(a: &Decimal, b: &Decimal) -> i64 {
    let (_, ca, ea) = a.finite_parts().expect("finite");
    let (_, cb, eb) = b.finite_parts().expect("finite");
    i64::from(ea) + ca.decimal_digit_count() as i64
        - i64::from(eb)
        - cb.decimal_digit_count() as i64
}

/// Align both finite operands to `min(ea, eb)` and integer-divide their
/// magnitudes. Returns `(quotient, remainder, min_e, divisor)` where `quotient
/// = |self| div |other|` (truncated), `remainder = |self| mod |other|`, and
/// `divisor` is `|other|` scaled to `min_e`; the remainder and divisor are
/// both as if at exponent `min_e`. The screens in the callers bound the
/// alignment gaps here by `precision + digits(a) + digits(b)`.
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
        Context::new(
            core::num::NonZeroU32::new(precision).unwrap(),
            9999,
            -9999,
            Rounding::HalfEven,
        )
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
    fn divrem_family_guards_extreme_gaps() {
        // fd-aqs.3 witness: integer_divide aligned both operands to
        // min(ea, eb) before any validity check, allocating gigabytes for
        // i32-range exponent gaps. The screens must decide first.
        let c = Context::new(
            core::num::NonZeroU32::new(9).unwrap(),
            i32::MAX,
            i32::MIN,
            Rounding::HalfEven,
        );
        let big = fin(false, 1, i32::MAX);
        let tiny = fin(false, 1, i32::MIN);
        // Quotient would need ~4.3e9 digits: INVALID without aligning.
        let (r, s) = big.divide_integer(&tiny, &c);
        assert!(r.is_nan() && s.invalid());
        let (r2, s2) = big.remainder(&tiny, &c);
        assert!(r2.is_nan() && s2.invalid());
        // |a| < |b|: zero quotient, the dividend is its own remainder.
        let (q, qs) = tiny.divide_integer(&big, &c);
        assert_eq!(q, fin(false, 0, 0));
        assert!(qs.is_ok());
        let (rem, rs) = tiny.remainder(&big, &c);
        assert_eq!(rem, fin(false, 1, i32::MIN), "got {rem:?}");
        assert!(rs.is_ok());
        let (rn, ns) = tiny.remainder_near(&big, &c);
        assert_eq!(rn, fin(false, 1, i32::MIN), "got {rn:?}");
        assert!(ns.is_ok());
    }

    #[test]
    fn divide_integer_decides_precision_boundary_by_division() {
        // adj-gap == precision cannot be decided by the screen alone: 1000
        // needs four digits (INVALID at precision 3) while 999 fits.
        let c = ctx(3);
        let (r, s) = fin(false, 1000, 0).divide_integer(&fin(false, 1, 0), &c);
        assert!(r.is_nan() && s.invalid());
        let (r2, s2) = fin(false, 999, 0).divide_integer(&fin(false, 1, 0), &c);
        assert_eq!(r2, fin(false, 999, 0));
        assert!(s2.is_ok());
        // One past the precision the screen rejects outright.
        let (r3, s3) = fin(false, 10_000, 0).divide_integer(&fin(false, 1, 0), &c);
        assert!(r3.is_nan() && s3.invalid());
    }

    #[test]
    fn remainder_near_adjacent_adjusted_exponents_still_flips() {
        // adj(6) is one below adj(10), but 6 is past half of 10: the
        // nearest quotient is 1 and the residue flips to -(10 - 6). The
        // adjacent-adjusted-exponent band must reach the real division.
        let c = ctx(9);
        let (r, _) = fin(false, 6, 0).remainder_near(&fin(false, 10, 0), &c);
        assert_eq!(r, fin(true, 4, 0), "got {r:?}");
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
