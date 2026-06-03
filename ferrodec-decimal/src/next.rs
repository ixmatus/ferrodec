//! General Decimal Arithmetic next-value operations: `next_plus`, `next_minus`,
//! and `next_toward`.
//!
//! These navigate the context's representable grid: the values with at most
//! `precision` significant digits and an adjusted exponent in the inclusive
//! range from `emin` to `emax`, together with the subnormals down to the
//! quantum `Etiny` (which is `emin - precision + 1`). `next_plus` returns the
//! least representable value greater than the operand, `next_minus` the
//! greatest one less than it, and `next_toward` steps the first operand one
//! place toward the second.
//!
//! `next_plus` and `next_minus` signal nothing except `Invalid_operation` for a
//! signaling NaN: stepping into the subnormal range or off the top to infinity
//! raises no flag. `next_toward` instead signals like an arithmetic step: the
//! result raises `Underflow` and `Inexact` when it is subnormal, `Overflow` and
//! `Inexact` when it is infinite, and nothing when it is a normal number. See
//! the General Decimal Arithmetic specification ("next-plus", "next-minus",
//! "next-toward") and ADR-0041.

use core::cmp::Ordering;

use crate::arith::{nan_result, nan_unary};
use crate::compare::numeric_cmp;
use crate::round::round_finite;
use crate::{Context, Decimal, Rounding, Status};
use ferrodec_multiword::DecBig;

impl Decimal {
    /// General Decimal Arithmetic `next_plus`: the least representable value
    /// greater than `self`. Signals only `Invalid_operation`, for a signaling
    /// NaN.
    #[must_use]
    pub fn next_plus(&self, ctx: &Context) -> (Decimal, Status) {
        if let Some(r) = nan_unary(self, ctx) {
            return r;
        }
        (step(self, true, ctx), Status::OK)
    }

    /// General Decimal Arithmetic `next_minus`: the greatest representable value
    /// less than `self`. Signals only `Invalid_operation`, for a signaling NaN.
    #[must_use]
    pub fn next_minus(&self, ctx: &Context) -> (Decimal, Status) {
        if let Some(r) = nan_unary(self, ctx) {
            return r;
        }
        (step(self, false, ctx), Status::OK)
    }

    /// General Decimal Arithmetic `next_toward`: `self` stepped one representable
    /// place toward `other`. When the operands are numerically equal the result
    /// is `self` with the sign of `other` and no flags. Otherwise the step
    /// signals `Underflow` / `Overflow` and `Inexact` when the result is
    /// subnormal or infinite, as an arithmetic operation would.
    #[must_use]
    pub fn next_toward(&self, other: &Self, ctx: &Context) -> (Decimal, Status) {
        if let Some(r) = nan_result(self, other, ctx) {
            return r;
        }
        match numeric_cmp(self, other) {
            Ordering::Equal => (self.copy_sign(other), Status::OK),
            Ordering::Less => {
                let v = step(self, true, ctx);
                let s = toward_flags(&v, ctx);
                (v, s)
            }
            Ordering::Greater => {
                let v = step(self, false, ctx);
                let s = toward_flags(&v, ctx);
                (v, s)
            }
        }
    }
}

/// Step `x` one representable place up (`up`) or down. `next_minus` is the
/// mirror of `next_plus` through negation, so the down direction reuses the up
/// path on the negated value.
fn step(x: &Decimal, up: bool, ctx: &Context) -> Decimal {
    if !up {
        return step(&x.copy_negate(), true, ctx).copy_negate();
    }
    // An infinity steps to itself going up (+Inf) or to the largest finite
    // magnitude coming down from above (-Inf up is -Nmax).
    if x.is_infinite() {
        return if x.is_negative() {
            nmax(ctx, true)
        } else {
            Decimal::infinity(false)
        };
    }
    // Round `x` up onto the grid (toward +infinity). For an operand that is not
    // representable this already yields the next value above it; for one that is
    // representable it returns the value unchanged, and the explicit successor
    // below takes the single step.
    let r = round_ceiling(x, ctx);
    if numeric_cmp(&r, x) == Ordering::Greater {
        return r;
    }
    let (sign, coeff, exp) = r.finite_parts().expect("on-grid finite");
    if coeff.is_zero() {
        // The successor of either signed zero is the least positive subnormal.
        return Decimal::finite(false, DecBig::from_u32(1), etiny_of(ctx));
    }
    if sign {
        // A negative value increases by shrinking its magnitude toward zero.
        pred_mag(coeff, exp, ctx).copy_negate()
    } else {
        succ_mag(coeff, exp, ctx)
    }
}

/// Round `x` to the grid toward positive infinity, discarding the status (the
/// next-value operations own their own flag rules).
fn round_ceiling(x: &Decimal, ctx: &Context) -> Decimal {
    let (sign, coeff, exp) = x.finite_parts().expect("finite");
    let cctx = ctx.with_rounding(Rounding::Ceiling);
    round_finite(
        sign,
        coeff.clone(),
        i64::from(exp),
        false,
        i64::from(exp),
        &cctx,
        Status::OK,
    )
    .0
}

/// The next larger magnitude above the on-grid magnitude `coeff * 10^exp`,
/// returned as a non-negative value (`+Infinity` past the largest finite).
fn succ_mag(coeff: &DecBig, exp: i32, ctx: &Context) -> Decimal {
    let p = i64::from(ctx.precision);
    let et = i64::from(etiny_of(ctx));
    let digits = coeff.decimal_digit_count() as i64;
    let adj = i64::from(exp) + digits - 1;
    let q = (adj - (p - 1)).max(et);
    let c_q = coeff.mul_pow10((i64::from(exp) - q) as u32);
    let c1 = c_q.add(&DecBig::from_u32(1));
    if c1.decimal_digit_count() as i64 > p {
        // Decade spill (10^p): renormalize to 10^(p-1) one decade up.
        let c2 = c1.div_rem10().0;
        let nexp = q + 1;
        let nadj = nexp + c2.decimal_digit_count() as i64 - 1;
        if nadj > i64::from(ctx.emax) {
            return Decimal::infinity(false);
        }
        return Decimal::finite(false, c2, nexp as i32);
    }
    Decimal::finite(false, c1, q as i32)
}

/// The next smaller magnitude below the on-grid magnitude `coeff * 10^exp`,
/// returned as a non-negative value (zero past the least positive subnormal).
fn pred_mag(coeff: &DecBig, exp: i32, ctx: &Context) -> Decimal {
    let p = i64::from(ctx.precision);
    let et = i64::from(etiny_of(ctx));
    let digits = coeff.decimal_digit_count() as i64;
    let adj = i64::from(exp) + digits - 1;
    let q = (adj - (p - 1)).max(et);
    let c_q = coeff.mul_pow10((i64::from(exp) - q) as u32);
    // Crossing below a power of ten refines the quantum by one decade, unless
    // already at the subnormal floor.
    if q > et && c_q.cmp_ref(&DecBig::pow10(ctx.precision - 1)) == Ordering::Equal {
        let nines = DecBig::pow10(ctx.precision).sub(&DecBig::from_u32(1));
        return Decimal::finite(false, nines, (q - 1) as i32);
    }
    let c1 = c_q.sub(&DecBig::from_u32(1));
    if c1.is_zero() {
        return Decimal::finite(false, DecBig::zero(), etiny_of(ctx));
    }
    Decimal::finite(false, c1, q as i32)
}

/// The largest finite magnitude `Nmax = (10^precision - 1) * 10^Etop`, signed.
fn nmax(ctx: &Context, negative: bool) -> Decimal {
    let etop = ctx.emax - (ctx.precision as i32 - 1);
    let coeff = DecBig::pow10(ctx.precision).sub(&DecBig::from_u32(1));
    Decimal::finite(negative, coeff, etop)
}

/// The subnormal floor quantum `Etiny = emin - (precision - 1)`.
fn etiny_of(ctx: &Context) -> i32 {
    ctx.emin - (ctx.precision as i32 - 1)
}

/// The flags a `next_toward` step raises, classified from its result: overflow
/// to infinity, underflow into the subnormal range (a zero result is at the
/// clamped subnormal floor), or nothing for a normal result.
fn toward_flags(v: &Decimal, ctx: &Context) -> Status {
    if v.is_infinite() {
        return Status::OVERFLOW | Status::INEXACT;
    }
    let (_, coeff, exp) = v.finite_parts().expect("finite step result");
    if coeff.is_zero() {
        // A zero result underflowed across the subnormal gap toward zero, and
        // lands at Etiny. It raises underflow, inexact, and clamped (the
        // exponent is held at the floor) exactly when that exponent is below
        // Emin, independent of the clamp flag. At precision one Etiny equals
        // Emin, so the signed zero carries no flag (libmpdec agrees).
        return if i64::from(exp) < i64::from(ctx.emin) {
            Status::UNDERFLOW | Status::INEXACT | Status::CLAMPED
        } else {
            Status::OK
        };
    }
    let adj = i64::from(exp) + coeff.decimal_digit_count() as i64 - 1;
    if adj < i64::from(ctx.emin) {
        Status::UNDERFLOW | Status::INEXACT
    } else {
        Status::OK
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> Context {
        // Matches the nextplus / nextminus suite: Etiny = -383 - 8 = -391.
        Context::new(9, 384, -383, Rounding::HalfEven)
    }

    fn parse(s: &str) -> Decimal {
        Decimal::parse_str(s).unwrap()
    }

    #[test]
    fn next_plus_steps_and_crosses_decades() {
        let c = ctx();
        assert_eq!(parse("1").next_plus(&c).0, parse("1.00000001"));
        assert_eq!(parse("1.0").next_plus(&c).0, parse("1.00000001"));
        assert_eq!(parse("0.999999999").next_plus(&c).0, parse("1.00000000"));
        assert_eq!(parse("-1.00000000").next_plus(&c).0, parse("-0.999999999"));
    }

    #[test]
    fn next_plus_edges_no_flags() {
        let c = ctx();
        // Zero to the least positive subnormal; a subnormal step; both flagless.
        let (r, s) = parse("0").next_plus(&c);
        assert_eq!(r, parse("1E-391"));
        assert!(s == Status::OK);
        assert_eq!(parse("1E-391").next_plus(&c).0, parse("2E-391"));
        assert!(parse("1E-391").next_plus(&c).1 == Status::OK);
        // A tiny negative steps up to negative zero at the floor, no flags.
        assert_eq!(parse("-1E-99999").next_plus(&c).0, parse("-0E-391"));
        // Largest finite steps to infinity, still no flag for next_plus.
        let (r, s) = parse("9.99999999E+384").next_plus(&c);
        assert!(r.is_infinite() && !r.is_negative() && s == Status::OK);
        // -Inf steps up to -Nmax.
        assert_eq!(
            Decimal::infinity(true).next_plus(&c).0,
            parse("-9.99999999E+384")
        );
    }

    #[test]
    fn next_minus_is_the_mirror() {
        let c = ctx();
        assert_eq!(parse("1").next_minus(&c).0, parse("0.999999999"));
        assert_eq!(parse("1.00000000").next_minus(&c).0, parse("0.999999999"));
        // +Inf steps down to +Nmax; sNaN is the only signal.
        assert_eq!(
            Decimal::infinity(false).next_minus(&c).0,
            parse("9.99999999E+384")
        );
        assert!(Decimal::signaling_nan(false, DecBig::zero())
            .next_minus(&c)
            .1
            .invalid());
    }

    #[test]
    fn next_toward_signals_like_a_step() {
        let c = ctx();
        // Equal operands: self with the sign of other, no flags.
        let (r, s) = parse("10").next_toward(&parse("10"), &c);
        assert_eq!(r, parse("10"));
        assert!(s == Status::OK);
        // Normal-to-normal step raises nothing.
        let (r, s) = parse("1").next_toward(&parse("10"), &c);
        assert_eq!(r, parse("1.00000001"));
        assert!(s == Status::OK);
        // A subnormal result raises underflow and inexact.
        let (r, s) = parse("0").next_toward(&Decimal::infinity(false), &c);
        assert_eq!(r, parse("1E-391"));
        assert!(s.underflow() && s.inexact() && !s.clamped());
        // A zero result lands at Etiny (-391), below Emin (-383), so it
        // underflows and clamps regardless of the clamp flag.
        let (r, s) = parse("-1E-99999").next_toward(&Decimal::infinity(false), &c);
        assert_eq!(r, parse("-0E-391"));
        assert!(s.underflow() && s.inexact() && s.clamped());
        // At precision one Etiny equals Emin, so a zero result carries no flag.
        let c1 = Context::new(1, 6, -6, Rounding::HalfEven);
        let (r, s) = parse("-1E-50").next_toward(&Decimal::infinity(false), &c1);
        assert!(r.is_zero() && r.is_negative() && s == Status::OK);
        // Stepping off the top raises overflow and inexact.
        let (r, s) = parse("9.99999999E+384").next_toward(&Decimal::infinity(false), &c);
        assert!(r.is_infinite() && s.overflow() && s.inexact());
    }
}
