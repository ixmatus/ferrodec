//! Exponent-shaping operations: quantize, round-to-integral, and reduce.
//!
//! Unlike the arithmetic operations, these are driven by a target exponent
//! rather than the working precision, so they round directly to that exponent
//! and report `INVALID` when the result would not fit the precision.

use crate::arith::{invalid_nan, nan_result, nan_unary};
use crate::{Context, Decimal, Status};
use ferrodec_multiword::DecBig;

impl Decimal {
    /// Round `self` to the exponent of `other`, keeping `other`'s exponent. The
    /// result is invalid if it would need more than the context's precision.
    #[must_use]
    pub fn quantize(&self, other: &Self, ctx: &Context) -> (Decimal, Status) {
        if let Some(r) = nan_result(self, other, ctx) {
            return r;
        }
        let (a_inf, b_inf) = (self.is_infinite(), other.is_infinite());
        if a_inf || b_inf {
            // Two infinities quantize to the dividend infinity; a mix is invalid.
            return if a_inf && b_inf {
                (Decimal::infinity(self.is_negative()), Status::OK)
            } else {
                (invalid_nan(), Status::INVALID)
            };
        }
        let (sa, ca, ea) = self.finite_parts().expect("finite");
        let (_, _, eb) = other.finite_parts().expect("finite");
        let p = i64::from(ctx.precision);
        let emax = i64::from(ctx.emax);
        let etiny = i64::from(ctx.emin) - (p - 1);
        let eb_i = i64::from(eb);

        // The target exponent must sit at or above the subnormal floor.
        if eb_i < etiny {
            return (invalid_nan(), Status::INVALID);
        }

        let (coeff, status) = if ea >= eb {
            // Pad with trailing zeros: exact, no rounding.
            (ca.mul_pow10((ea - eb) as u32), Status::OK)
        } else {
            // Round away the digits below the target exponent.
            round_to_exponent(sa, ca, (eb - ea) as u32, ctx)
        };

        // Invalid if the result does not fit the precision or its adjusted
        // exponent overflows Emax.
        let digits = coeff.decimal_digit_count() as i64;
        if digits > p || eb_i + digits - 1 > emax {
            return (invalid_nan(), Status::INVALID);
        }
        (Decimal::finite(sa, coeff, eb), status)
    }

    /// Round `self` to an integer (exponent zero, or the operand's own exponent
    /// if it is already integral), without raising `INEXACT`.
    #[must_use]
    pub fn round_to_integral_value(&self, ctx: &Context) -> (Decimal, Status) {
        self.to_integral(ctx, false)
    }

    /// Like [`round_to_integral_value`](Self::round_to_integral_value) but
    /// raises `INEXACT` when the operand had a fractional part.
    #[must_use]
    pub fn round_to_integral_exact(&self, ctx: &Context) -> (Decimal, Status) {
        self.to_integral(ctx, true)
    }

    fn to_integral(&self, ctx: &Context, exact: bool) -> (Decimal, Status) {
        if let Some(r) = nan_unary(self, ctx) {
            return r;
        }
        if self.is_infinite() {
            return (Decimal::infinity(self.is_negative()), Status::OK);
        }
        let (sa, ca, ea) = self.finite_parts().expect("finite");
        if ea >= 0 {
            // Already integral: the value is returned unchanged.
            return (Decimal::finite(sa, ca.clone(), ea), Status::OK);
        }
        let (coeff, mut status) = round_to_exponent(sa, ca, (-ea) as u32, ctx);
        if !exact {
            // roundToIntegralValue never signals inexact.
            status = Status::OK;
        }
        (Decimal::finite(sa, coeff, 0), status)
    }

    /// Round to the precision and strip trailing zeros (the `reduce` /
    /// `normalize` operation). A zero reduces to exponent zero.
    #[must_use]
    pub fn reduce(&self, ctx: &Context) -> (Decimal, Status) {
        if let Some(r) = nan_unary(self, ctx) {
            return r;
        }
        if self.is_infinite() {
            return (Decimal::infinity(self.is_negative()), Status::OK);
        }
        // Round to the precision first, then strip trailing zeros.
        let (rounded, status) = self.plus(ctx);
        let Some((sign, coeff, exp)) = rounded.finite_parts().map(|(s, c, e)| (s, c.clone(), e))
        else {
            return (rounded, status);
        };
        if coeff.is_zero() {
            return (Decimal::finite(sign, DecBig::zero(), 0), status);
        }
        let mut c = coeff;
        let mut e = i64::from(exp);
        loop {
            let (q, r) = c.div_rem10();
            if r != 0 {
                break;
            }
            c = q;
            e += 1;
        }
        (Decimal::finite(sign, c, e as i32), status)
    }
}

/// Round `coeff` (sign `sign`) down by `drop` decimal places under the
/// context's rounding mode, returning `(rounded_coeff, status)` where status
/// carries `INEXACT` if any dropped digit was non-zero.
fn round_to_exponent(sign: bool, coeff: &DecBig, drop: u32, ctx: &Context) -> (DecBig, Status) {
    let (kept, rem) = coeff.div_rem_pow10(drop);
    let (rd, lower) = rem.div_rem_pow10(drop - 1);
    let round_digit = rd.to_u128().unwrap_or(0) as u32;
    let sticky = !lower.is_zero();
    let last = kept.div_rem10().1;
    let mut status = Status::OK;
    if round_digit != 0 || sticky {
        status |= Status::INEXACT;
    }
    let result = if ctx.rounding.round_up(sign, last, round_digit, sticky) {
        kept.add(&DecBig::from_u32(1))
    } else {
        kept
    };
    (result, status)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Rounding;
    use alloc::string::ToString;

    fn ctx(precision: u32) -> Context {
        Context::new(precision, 9999, -9999, Rounding::HalfEven)
    }

    fn fin(coeff: u128, exp: i32) -> Decimal {
        Decimal::finite(false, DecBig::from_u128(coeff), exp)
    }

    #[test]
    fn quantize_pads_and_rounds() {
        let c = ctx(9);
        // 2.17 quantized to 1E-3 -> 2.170 (pad).
        assert_eq!(
            fin(217, -2).quantize(&fin(1, -3), &c).0.to_string(),
            "2.170"
        );
        // 2.17 quantized to 1E-1 -> 2.2 (round half even).
        assert_eq!(fin(217, -2).quantize(&fin(1, -1), &c).0.to_string(), "2.2");
        // 2.17 quantized to 1E0 -> 2.
        assert_eq!(fin(217, -2).quantize(&fin(1, 0), &c).0.to_string(), "2");
    }

    #[test]
    fn quantize_invalid_when_too_many_digits() {
        // 217 quantized to 1E-2 needs 5 digits; precision 3 cannot hold it.
        let (r, s) = fin(217, 0).quantize(&fin(1, -2), &ctx(3));
        assert!(r.is_nan() && s.invalid());
    }

    #[test]
    fn round_to_integral_keeps_or_rounds() {
        let c = ctx(9);
        // 2.5 -> 2 (half even), and exact flavor reports inexact.
        assert_eq!(fin(25, -1).round_to_integral_value(&c).0, fin(2, 0));
        assert!(fin(25, -1).round_to_integral_exact(&c).1.inexact());
        assert!(!fin(25, -1).round_to_integral_value(&c).1.inexact());
        // 1E2 is already integral; exponent preserved.
        assert_eq!(fin(1, 2).round_to_integral_value(&c).0, fin(1, 2));
    }

    #[test]
    fn reduce_strips_trailing_zeros() {
        let c = ctx(9);
        assert_eq!(fin(1200, -3).reduce(&c).0.to_string(), "1.2");
        assert_eq!(fin(120, 0).reduce(&c).0.to_string(), "1.2E+2");
        // Zero reduces to exponent zero.
        assert_eq!(fin(0, -3).reduce(&c).0.to_string(), "0");
    }
}
