//! Core arithmetic: add, subtract, multiply.
//!
//! Each operation handles the special values (NaN propagation and the infinity
//! algebra) first, then computes the exact finite result and rounds it to the
//! context through [`round_finite`](crate::round::round_finite). Because
//! `DecBig` is unbounded, the finite intermediates are computed exactly, so
//! rounding sees a true result with no alignment loss.
//!
//! Note: add and subtract align the operands by their exponents exactly. For
//! operands whose exponents differ by a very large amount the aligned
//! coefficient can grow large; bounding that alignment with a sticky tail is a
//! stated performance and resource-bounding follow-up. It does not affect
//! correctness, and in-context operands stay within the context's exponent
//! span.

use crate::round::round_finite;
use crate::{Context, Decimal, Rounding, Status};
use core::cmp::Ordering;
use ferrodec_multiword::DecBig;

impl Decimal {
    /// Add two decimals under `ctx`, returning the rounded result and status.
    #[must_use]
    pub fn add(&self, other: &Self, ctx: &Context) -> (Decimal, Status) {
        add_sub(self, other, false, ctx)
    }

    /// Subtract `other` from `self` under `ctx`.
    #[must_use]
    pub fn subtract(&self, other: &Self, ctx: &Context) -> (Decimal, Status) {
        add_sub(self, other, true, ctx)
    }

    /// Multiply two decimals under `ctx`.
    #[must_use]
    pub fn multiply(&self, other: &Self, ctx: &Context) -> (Decimal, Status) {
        if let Some(r) = nan_result(self, other, ctx) {
            return r;
        }
        let sign = self.is_negative() ^ other.is_negative();
        let (a_inf, b_inf) = (self.is_infinite(), other.is_infinite());
        if a_inf || b_inf {
            // Infinity times zero is undefined; otherwise a signed infinity.
            if (a_inf && other.is_zero()) || (b_inf && self.is_zero()) {
                return (invalid_nan(), Status::INVALID);
            }
            return (Decimal::infinity(sign), Status::OK);
        }
        let (_, ca, ea) = self.finite_parts().expect("finite");
        let (_, cb, eb) = other.finite_parts().expect("finite");
        let exp = i64::from(ea) + i64::from(eb);
        round_finite(sign, ca.mul(cb), exp, false, exp, ctx, Status::OK)
    }

    /// Fused multiply-add `(self * factor) + addend` under `ctx`, with the
    /// product formed exactly and a single rounding applied at the end.
    #[must_use]
    pub fn fma(&self, factor: &Self, addend: &Self, ctx: &Context) -> (Decimal, Status) {
        // A signaling NaN anywhere signals invalid first.
        for d in [self, factor, addend] {
            if d.is_signaling_nan() {
                return (quiet_from(d, ctx), Status::INVALID);
            }
        }
        // A quiet NaN in a factor propagates as the product before the addend
        // is consulted.
        if self.is_nan() {
            return (quiet_from(self, ctx), Status::OK);
        }
        if factor.is_nan() {
            return (quiet_from(factor, ctx), Status::OK);
        }

        let psign = self.is_negative() ^ factor.is_negative();
        let (a_inf, b_inf) = (self.is_infinite(), factor.is_infinite());

        // The exact product, as either a signed infinity or finite parts. An
        // invalid product (infinity times zero) wins over a quiet-NaN addend.
        let product = if a_inf || b_inf {
            if (a_inf && factor.is_zero()) || (b_inf && self.is_zero()) {
                return (invalid_nan(), Status::INVALID);
            }
            None // signed infinity, carried by psign
        } else {
            let (_, ca, ea) = self.finite_parts().expect("finite");
            let (_, cb, eb) = factor.finite_parts().expect("finite");
            Some((ca.mul(cb), i64::from(ea) + i64::from(eb)))
        };

        // Now add the addend; a quiet-NaN addend propagates here.
        if addend.is_nan() {
            return (quiet_from(addend, ctx), Status::OK);
        }
        match product {
            None => {
                // Infinite product plus the addend.
                if addend.is_infinite() && addend.is_negative() != psign {
                    return (invalid_nan(), Status::INVALID);
                }
                (Decimal::infinity(psign), Status::OK)
            }
            Some((pc, pe)) => {
                if addend.is_infinite() {
                    return (Decimal::infinity(addend.is_negative()), Status::OK);
                }
                let (cs, cc, ce) = addend.finite_parts().expect("finite");
                combine_finite(psign, &pc, pe, cs, cc, i64::from(ce), ctx)
            }
        }
    }
}

/// Shared add / subtract. `subtract` flips the effective sign of `b`.
fn add_sub(a: &Decimal, b: &Decimal, subtract: bool, ctx: &Context) -> (Decimal, Status) {
    if let Some(r) = nan_result(a, b, ctx) {
        return r;
    }
    let a_neg = a.is_negative();
    let b_neg = b.is_negative() ^ subtract;
    let (a_inf, b_inf) = (a.is_infinite(), b.is_infinite());
    if a_inf || b_inf {
        return match (a_inf, b_inf) {
            // Opposite-signed infinities are undefined; like signs collapse.
            (true, true) if a_neg != b_neg => (invalid_nan(), Status::INVALID),
            (true, _) => (Decimal::infinity(a_neg), Status::OK),
            (_, true) => (Decimal::infinity(b_neg), Status::OK),
            _ => unreachable!(),
        };
    }

    let (sa, ca, ea) = a.finite_parts().expect("finite");
    let (sb_raw, cb, eb) = b.finite_parts().expect("finite");
    combine_finite(
        sa,
        ca,
        i64::from(ea),
        sb_raw ^ subtract,
        cb,
        i64::from(eb),
        ctx,
    )
}

/// Exactly combine two signed finite operands `(sa, ca, ea)` and
/// `(sb, cb, eb)` (where the value is `(-1)^s * coeff * 10^exp`) and round to
/// the context. Aligns to the smaller exponent, adds like signs or subtracts
/// by magnitude for opposite signs, and rounds toward the ideal exponent
/// `min(ea, eb)`. Exponents are `i64` so a fused product's exponent fits.
#[allow(clippy::too_many_arguments)]
pub(crate) fn combine_finite(
    sa: bool,
    ca: &DecBig,
    ea: i64,
    sb: bool,
    cb: &DecBig,
    eb: i64,
    ctx: &Context,
) -> (Decimal, Status) {
    let min_e = ea.min(eb);
    let ca2 = ca.mul_pow10((ea - min_e) as u32);
    let cb2 = cb.mul_pow10((eb - min_e) as u32);

    let (sign, coeff) = if sa == sb {
        (sa, ca2.add(&cb2))
    } else {
        match ca2.cmp_ref(&cb2) {
            Ordering::Greater => (sa, ca2.sub(&cb2)),
            Ordering::Less => (sb, cb2.sub(&ca2)),
            // Exact cancellation: the zero is positive except under round-floor.
            Ordering::Equal => (ctx.rounding == Rounding::Floor, DecBig::zero()),
        }
    };

    round_finite(sign, coeff, min_e, false, min_e, ctx, Status::OK)
}

/// NaN propagation common to every binary operation. A signaling NaN operand
/// yields its quieted form and an invalid-operation flag; a quiet NaN operand
/// propagates as a quiet NaN. The diagnostic payload is truncated to the
/// context precision (its low `precision` digits), matching the General
/// Decimal Arithmetic reference. Returns `None` when neither operand is a NaN.
pub(crate) fn nan_result(a: &Decimal, b: &Decimal, ctx: &Context) -> Option<(Decimal, Status)> {
    if a.is_signaling_nan() {
        return Some((quiet_from(a, ctx), Status::INVALID));
    }
    if b.is_signaling_nan() {
        return Some((quiet_from(b, ctx), Status::INVALID));
    }
    if a.is_nan() {
        return Some((quiet_from(a, ctx), Status::OK));
    }
    if b.is_nan() {
        return Some((quiet_from(b, ctx), Status::OK));
    }
    None
}

/// Unary NaN propagation (`squareRoot` and the other single-operand ops): a
/// signaling NaN quiets and signals invalid; a quiet NaN propagates. Returns
/// `None` when the operand is not a NaN.
pub(crate) fn nan_unary(a: &Decimal, ctx: &Context) -> Option<(Decimal, Status)> {
    if a.is_signaling_nan() {
        return Some((quiet_from(a, ctx), Status::INVALID));
    }
    if a.is_nan() {
        return Some((quiet_from(a, ctx), Status::OK));
    }
    None
}

/// Build the propagated quiet NaN: same sign, payload truncated to the low
/// `ctx.precision` digits.
pub(crate) fn quiet_from(d: &Decimal, ctx: &Context) -> Decimal {
    let (sign, _signaling, payload) = d.nan_parts().expect("nan");
    let truncated = payload.div_rem_pow10(ctx.precision).1;
    Decimal::quiet_nan(sign, truncated)
}

/// The default quiet NaN produced by an invalid operation.
pub(crate) fn invalid_nan() -> Decimal {
    Decimal::quiet_nan(false, DecBig::zero())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(precision: u32) -> Context {
        Context::new(precision, 9999, -9999, Rounding::HalfEven)
    }

    fn fin(sign: bool, coeff: u128, exp: i32) -> Decimal {
        Decimal::finite(sign, DecBig::from_u128(coeff), exp)
    }

    #[test]
    fn add_aligns_and_keeps_ideal_exponent() {
        let c = ctx(9);
        // 1.23 + 4.567 = 5.797 (ideal exponent min(-2,-3) = -3).
        let (r, s) = fin(false, 123, -2).add(&fin(false, 4567, -3), &c);
        assert_eq!(r, fin(false, 5797, -3));
        assert!(s.is_ok());
    }

    #[test]
    fn subtract_opposite_signs_and_cancellation() {
        let c = ctx(9);
        // 5 - 3 = 2.
        assert_eq!(
            fin(false, 5, 0).subtract(&fin(false, 3, 0), &c).0,
            fin(false, 2, 0)
        );
        // 3 - 5 = -2.
        assert_eq!(
            fin(false, 3, 0).subtract(&fin(false, 5, 0), &c).0,
            fin(true, 2, 0)
        );
        // 2.5 - 2.5 = 0 (positive zero, ideal exponent -1).
        let (z, _) = fin(false, 25, -1).subtract(&fin(false, 25, -1), &c);
        assert!(z.is_zero() && !z.is_negative());
        assert_eq!(z, fin(false, 0, -1));
    }

    #[test]
    fn multiply_signs_and_exponents() {
        let c = ctx(9);
        // 1.5 * -2 = -3.0 (ideal exponent -1 + 0 = -1).
        let (r, _) = fin(false, 15, -1).multiply(&fin(true, 2, 0), &c);
        assert_eq!(r, fin(true, 30, -1));
    }

    #[test]
    fn add_rounds_to_precision() {
        // Precision 3: 999 + 2 = 1001 -> 1.00E3 (round half even on the 1).
        let c = ctx(3);
        let (r, s) = fin(false, 999, 0).add(&fin(false, 2, 0), &c);
        assert_eq!(r, fin(false, 100, 1));
        assert!(s.inexact());
    }

    #[test]
    fn infinity_algebra() {
        let c = ctx(9);
        let pinf = Decimal::infinity(false);
        let ninf = Decimal::infinity(true);
        // Inf + Inf = Inf; Inf + -Inf = NaN invalid.
        assert_eq!(pinf.add(&pinf, &c).0, pinf);
        let (nan, s) = pinf.add(&ninf, &c);
        assert!(nan.is_nan() && s.invalid());
        // Inf * 0 = NaN invalid; Inf * 2 = Inf.
        let (nan2, s2) = pinf.multiply(&fin(false, 0, 0), &c);
        assert!(nan2.is_nan() && s2.invalid());
        assert_eq!(pinf.multiply(&fin(false, 2, 0), &c).0, pinf);
    }

    #[test]
    fn nan_propagation() {
        let c = ctx(9);
        let snan = Decimal::signaling_nan(false, DecBig::from_u32(7));
        let qnan = Decimal::quiet_nan(false, DecBig::from_u32(9));
        // sNaN signals invalid and quiets.
        let (r, s) = snan.add(&fin(false, 1, 0), &c);
        assert!(r.is_nan() && !r.is_signaling_nan() && s.invalid());
        // qNaN propagates quietly.
        let (r2, s2) = fin(false, 1, 0).multiply(&qnan, &c);
        assert_eq!(r2, qnan);
        assert!(s2.is_ok());
    }
}
