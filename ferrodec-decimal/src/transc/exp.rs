//! The natural exponential `exp(x) = e^x`.
//!
//! # Algorithm (derived fresh)
//!
//! Range reduction in base ten: write `x = k * ln 10 + r` with `k =
//! round(x / ln 10)` an integer and `|r| <= (ln 10) / 2 ≈ 1.151`. Then
//! `exp(x) = (e^{ln 10})^k * e^r = 10^k * e^r`, so the reapplication of the
//! reduced range is a free decimal exponent shift. `e^r` is summed from its
//! Taylor series `sum_{n>=0} r^n / n!`, which converges geometrically for the
//! reduced `r`. The constant `ln 10` is supplied by [`ConstCache`].
//!
//! Two precisions matter. The range reduction `x - k*ln 10` cancels the leading
//! `digits(k)` digits, so the series is evaluated at an internal precision that
//! adds those digits plus a guard on top of the requested working precision;
//! the returned value is then accurate to well under one ulp, which the bounded
//! Ziv strategy ([`finish`]) turns into a correctly rounded result. Like
//! `squareRoot`, and like libmpdec's `exp`, the rounding is half-even
//! regardless of the context's rounding mode.
//!
//! Derived from the Taylor series and the base-ten range reduction; see Muller,
//! *Elementary Functions*, for the reduction and error-budget framing. The
//! General Decimal Arithmetic specification defines the special cases and the
//! overflow / underflow behaviour.

use super::consts::ConstCache;
use super::strategy::finish;
use super::work::Work;
use crate::arith::nan_unary;
use crate::round::round_finite;
use crate::{Context, Decimal, Rounding, Status};
use core::cmp::Ordering;
use ferrodec_multiword::DecBig;

/// Internal guard digits carried below the working precision so the returned
/// `wp`-digit value is accurate to well under one ulp.
const KERNEL_GUARD: u32 = 8;

/// The kernel's ulp error bound at the working precision; the bracket half-width
/// the Ziv strategy uses. Conservative against the sub-ulp true error.
const EXP_ERR: u128 = 2;

impl Decimal {
    /// The natural exponential `e^self`, correctly rounded under `ctx`.
    ///
    /// Rounding is half-even regardless of `ctx.rounding`, matching the General
    /// Decimal Arithmetic reference. `exp(+/-0) = 1`, `exp(-Infinity) = +0`,
    /// `exp(+Infinity) = +Infinity`; a signaling NaN raises `Invalid_operation`.
    #[must_use]
    pub fn exp(&self, ctx: &Context) -> (Decimal, Status) {
        if let Some(r) = nan_unary(self, ctx) {
            return r;
        }
        if self.is_infinite() {
            return if self.is_negative() {
                (Decimal::finite(false, DecBig::zero(), 0), Status::OK)
            } else {
                (Decimal::infinity(false), Status::OK)
            };
        }
        if self.is_zero() {
            return (Decimal::finite(false, DecBig::from_u32(1), 0), Status::OK);
        }

        let x = Work::from_decimal(self);
        let neg = self.is_negative();
        // exp forces half-even, like squareRoot.
        let round_ctx = Context {
            rounding: Rounding::HalfEven,
            ..*ctx
        };
        let mut cache = ConstCache::new();

        // Far-field gate. exp(x) is finite-representable only for x roughly in
        // [Etiny*ln10, (Emax+1)*ln10]; past a generous margin the result is
        // unambiguously +Infinity (x > 0) or rounds to +0 / the smallest
        // subnormal (x < 0), and the reduction multiple k would be huge.
        let span = i64::from(ctx.emax) - i64::from(ctx.emin) + i64::from(ctx.precision.get()) + 16;
        let bound = cache.ln10(24).mul(&Work::from_i64(span));
        if x.cmp_magnitude(&bound) == Ordering::Greater {
            return if neg {
                far_underflow(&round_ctx)
            } else {
                far_overflow(&round_ctx)
            };
        }

        finish(&round_ctx, EXP_ERR, |wp| exp_kernel(&x, wp, &mut cache))
    }
}

/// `exp(x)` to `wp` significant digits, accurate to within [`EXP_ERR`] ulp.
/// Precondition: `x` is finite, nonzero, and within the far-field gate.
pub(super) fn exp_kernel(x: &Work, wp: u32, cache: &mut ConstCache) -> Work {
    // Resolve the reduction multiple k = round(x / ln 10). The quotient needs
    // enough digits to carry the integer part of x/ln10 plus slack.
    let kp = u32::try_from(x.adj_exp().max(0))
        .unwrap_or(u32::MAX)
        .saturating_add(24);
    let k = x.div_to(&cache.ln10(kp), kp).round_to_i64();
    let k_digits = if k == 0 {
        0
    } else {
        k.unsigned_abs().ilog10() + 1
    };

    // Internal precision absorbs the range-reduction cancellation and a guard.
    let ip = wp.saturating_add(k_digits).saturating_add(KERNEL_GUARD);

    // Argument halving: e^r = (e^(r / 2^j))^(2^j). Reducing r by a power of two
    // cuts the Taylor term count from about ip to about ip/j; squaring back
    // costs j multiplies, so j ~ sqrt(ip) balances the two and the kernel runs
    // in about O(sqrt(ip)) full-width multiplies instead of O(ip). The j
    // squarings amplify the error by 2^j (about 0.3j digits), which the j guard
    // digits added to the internal precision absorb.
    let j = u32::try_from((u64::from(ip) * 10 / 3).isqrt()).unwrap_or(u32::MAX);
    let ip2 = ip.saturating_add(j);
    let ln10 = cache.ln10(ip2);
    // r = x - k*ln10, |r| <= ln10/2.
    let r = x.sub(&ln10.mul(&Work::from_i64(k)), ip2);
    // r / 2^j, exact: multiply the coefficient by 5^j and lower the exponent.
    let r_small = if j == 0 {
        r
    } else {
        let mut rr = Work::new(r.sign, r.coeff.mul(&pow5(j)), r.exp - i64::from(j));
        rr.sticky = r.sticky;
        rr.normalize_to(ip2);
        rr
    };

    // e^(r / 2^j) by its Taylor series; the tiny argument needs few terms.
    let mut term = Work::one();
    let mut sum = Work::one();
    let mut n: i64 = 1;
    loop {
        // term *= (r / 2^j) / n.
        term = term.mul_to(&r_small, ip2).div_to(&Work::from_i64(n), ip2);
        let negligible = term.is_zero() || sum.adj_exp() - i64::from(ip2) - 2 > term.adj_exp();
        sum = sum.add(&term, ip2);
        if negligible {
            break;
        }
        n += 1;
    }

    // Square j times to undo the halving: (e^(r / 2^j))^(2^j) = e^r.
    for _ in 0..j {
        sum = sum.mul_to(&sum, ip2);
    }

    // exp(x) = 10^k * e^r.
    sum.scale_pow10(k);
    sum.normalize_to(wp);
    sum
}

/// `5^j` as a `DecBig`, for the exact halving `r / 2^j = r * 5^j * 10^-j`.
fn pow5(j: u32) -> DecBig {
    let five = DecBig::from_u32(5);
    let mut acc = DecBig::from_u32(1);
    for _ in 0..j {
        acc = acc.mul(&five);
    }
    acc
}

/// The result when `exp` overflows: a positive magnitude past `Emax`, which
/// `round_finite` resolves to `+Infinity` (or `Nmax` under the directed modes,
/// though `exp` forces half-even) with `Overflow` and `Inexact`.
fn far_overflow(ctx: &Context) -> (Decimal, Status) {
    let e = i64::from(ctx.emax) + 2;
    round_finite(false, DecBig::from_u32(1), e, true, e, ctx, Status::OK)
}

/// The result when `exp` of a very negative argument underflows: a tiny
/// positive magnitude below `Etiny`, which `round_finite` resolves to `+0`
/// (half-even) with `Underflow`, `Inexact`, and `Clamped`.
fn far_underflow(ctx: &Context) -> (Decimal, Status) {
    let etiny = i64::from(ctx.emin) - i64::from(ctx.precision.get()) + 1;
    let e = etiny - 2;
    round_finite(false, DecBig::from_u32(1), e, true, e, ctx, Status::OK)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    /// `exp(1) = e`, exercising the halving (`j ~ sqrt(ip)`) and the squaring
    /// recombination at a precision well past one squaring round. The first 60
    /// digits of `e` are
    /// `2.71828182845904523536028747135266249775724709369995957496697...`.
    #[test]
    fn exp_one_matches_reference() {
        const E: &str = "271828182845904523536028747135266249775724709369995957496697";
        let mut cache = ConstCache::new();
        let mut v = exp_kernel(&Work::one(), 50, &mut cache);
        v.normalize_to(50);
        let got = v.coeff.to_string();
        // Compare the leading 48 digits, leaving the guard tail out of the check.
        assert_eq!(&got[..48], &E[..48], "exp(1) got {got}");
    }
}
