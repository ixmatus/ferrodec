//! The natural logarithm `ln(x)` and the base-ten logarithm `log10(x)`.
//!
//! # Algorithm (derived fresh)
//!
//! For `x` near one (`x` in `[1/2, 2]`) the logarithm is computed directly from
//! `ln(x) = 2 * atanh((x - 1) / (x + 1))`, whose argument has magnitude at most
//! `1/3` and which has no cancellation as `x -> 1` (the small result is built
//! up directly rather than as the difference of two larger logarithms).
//!
//! Otherwise the operand is split into a decade and a mantissa, `x = m * 10^q`
//! with `m` in `[1, 10)`, and the mantissa is further halved or doubled by a
//! power of two into `t = m / 2^j` in `[3/4, 3/2)`, both shifts exact in base
//! ten (dividing by two is multiplying by five and shifting the exponent). Then
//! `ln(x) = 2 * atanh((t - 1)/(t + 1)) + j * ln 2 + q * ln 10`, with `ln 2` and
//! `ln 10` from [`ConstCache`]. Away from one the result magnitude is at least
//! `ln 2`, so the recombination loses at most about one digit, absorbed by the
//! internal guard; the bounded Ziv strategy ([`finish`]) then rounds correctly.
//!
//! `log10(x) = ln(x) * (1 / ln 10)`, rounded once, except that an exact power of
//! ten returns its integer exponent exactly (so `log10(1000) = 3`, not a rounded
//! `3.000000`). Both rounding half-even regardless of the context, matching
//! `squareRoot`, `exp`, and libmpdec.
//!
//! Derived from the `atanh` identity and the decade reduction; see Muller,
//! *Elementary Functions*. The General Decimal Arithmetic specification fixes
//! the special cases (`ln(0) = -Infinity` with no flag, `ln(negative)` invalid,
//! `ln(1) = 0` exact).

use super::consts::ConstCache;
use super::strategy::{finish, DEFAULT_STRATEGY};
use super::work::Work;
use crate::arith::{invalid_nan, nan_unary};
use crate::round::round_finite;
use crate::{Context, Decimal, Rounding, Status};
use core::cmp::Ordering;
use ferrodec_multiword::DecBig;

/// Internal guard digits for the logarithm kernels.
const LN_GUARD: u32 = 12;

/// The kernels' ulp error bound at the working precision (the Ziv bracket
/// half-width). Conservative against the sub-ulp true error.
const LN_ERR: u128 = 2;

impl Decimal {
    /// The natural logarithm `ln(self)`, correctly rounded under `ctx`.
    ///
    /// Rounding is half-even regardless of `ctx.rounding`. `ln(1) = +0` (exact),
    /// `ln(0) = -Infinity` (no flag), `ln(+Infinity) = +Infinity`; a negative
    /// operand (including `-Infinity`) is `Invalid_operation`, and a signaling
    /// NaN raises `Invalid_operation`.
    #[must_use]
    pub fn ln(&self, ctx: &Context) -> (Decimal, Status) {
        if let Some(r) = nan_unary(self, ctx) {
            return r;
        }
        if self.is_zero() {
            return (Decimal::infinity(true), Status::OK);
        }
        if self.is_negative() {
            return (invalid_nan(), Status::INVALID);
        }
        if self.is_infinite() {
            return (Decimal::infinity(false), Status::OK);
        }
        let (_, coeff, exp) = self.finite_parts().expect("finite");
        if power_of_ten_exponent(coeff, i64::from(exp)) == Some(0) {
            // ln(1) = +0, exact (also resolves the table maker's dilemma at 1).
            return (Decimal::finite(false, DecBig::zero(), 0), Status::OK);
        }

        let x = Work::from_decimal(self);
        let round_ctx = Context {
            rounding: Rounding::HalfEven,
            ..*ctx
        };
        let mut cache = ConstCache::new();
        finish(&round_ctx, LN_ERR, DEFAULT_STRATEGY, |wp| {
            ln_kernel(&x, wp, &mut cache)
        })
    }

    /// The base-ten logarithm `log10(self)`, correctly rounded under `ctx`.
    ///
    /// Rounding is half-even regardless of `ctx.rounding`. An exact power of ten
    /// returns its integer exponent exactly (`log10(0.001) = -3`); otherwise the
    /// result is `ln(self) / ln 10`. `log10(0) = -Infinity` (no flag),
    /// `log10(+Infinity) = +Infinity`; a negative operand is `Invalid_operation`.
    #[must_use]
    pub fn log10(&self, ctx: &Context) -> (Decimal, Status) {
        if let Some(r) = nan_unary(self, ctx) {
            return r;
        }
        if self.is_zero() {
            return (Decimal::infinity(true), Status::OK);
        }
        if self.is_negative() {
            return (invalid_nan(), Status::INVALID);
        }
        if self.is_infinite() {
            return (Decimal::infinity(false), Status::OK);
        }
        let round_ctx = Context {
            rounding: Rounding::HalfEven,
            ..*ctx
        };
        let (_, coeff, exp) = self.finite_parts().expect("finite");
        if let Some(n) = power_of_ten_exponent(coeff, i64::from(exp)) {
            // log10(10^n) = n, exact integer (the likely first cohort mismatch
            // if computed as ln(x)/ln10 and rounded).
            return round_finite(
                n < 0,
                DecBig::from_u128(u128::from(n.unsigned_abs())),
                0,
                false,
                0,
                &round_ctx,
                Status::OK,
            );
        }

        let x = Work::from_decimal(self);
        let mut cache = ConstCache::new();
        finish(&round_ctx, LN_ERR, DEFAULT_STRATEGY, |wp| {
            // ln(x) at a small extra guard, divided by ln 10.
            let ip = wp + 4;
            let lnx = ln_kernel(&x, ip, &mut cache);
            let inv = cache.inv_ln10(ip);
            let mut r = lnx.mul_to(&inv, ip);
            r.normalize_to(wp);
            r
        })
    }
}

/// `ln(x)` to `wp` significant digits, accurate to within [`LN_ERR`] ulp.
/// Precondition: `x` is finite, positive, and not exactly one.
fn ln_kernel(x: &Work, wp: u32, cache: &mut ConstCache) -> Work {
    let ip = wp + LN_GUARD;
    let half = Work::new(false, DecBig::from_u32(5), -1); // 0.5
    let two = Work::from_i64(2);

    if x.cmp_magnitude(&half) != Ordering::Less && x.cmp_magnitude(&two) != Ordering::Greater {
        // Near one: ln(x) = 2 * atanh((x - 1) / (x + 1)), no cancellation.
        let one = Work::one();
        let w = x.sub(&one, ip).div_to(&x.add(&one, ip), ip);
        let a = atanh(&w, ip);
        let mut r = a.add(&a, ip);
        r.normalize_to(wp);
        return r;
    }

    // Decade split: x = m * 10^q, m in [1, 10).
    let q = x.adj_exp();
    let m = Work::new(false, x.coeff.clone(), x.exp - q);
    // Reduce m by a power of two into t = m / 2^j in [3/4, 3/2).
    let (j, t) = reduce_by_two(&m);
    let one = Work::one();
    let w = t.sub(&one, ip).div_to(&t.add(&one, ip), ip);
    let ln_t = {
        let a = atanh(&w, ip);
        a.add(&a, ip)
    };
    // Recombine: ln(x) = ln(t) + j*ln2 + q*ln10.
    let term_j = cache.ln2(ip).mul(&Work::from_i64(j));
    let term_q = cache.ln10(ip).mul(&Work::from_i64(q));
    let mut r = ln_t.add(&term_j, ip).add(&term_q, ip);
    r.normalize_to(wp);
    r
}

/// `atanh(w) = sum_{k>=0} w^(2k+1) / (2k+1)` at internal precision `ip`, for a
/// `Work` argument `w` with `|w| <= 1/3` (so the series converges geometrically
/// by `w^2`). `atanh(0) = 0`.
fn atanh(w: &Work, ip: u32) -> Work {
    let w_sq = w.mul_to(w, ip);
    let mut power = w.clone(); // w^(2k+1)
    let mut sum = w.clone();
    let mut k: i64 = 1;
    let max_iter = i64::from(ip) * 4 + 16;
    while k <= max_iter {
        power = power.mul_to(&w_sq, ip);
        let term = power.div_to(&Work::from_i64(2 * k + 1), ip);
        let negligible = term.is_zero() || sum.adj_exp() - i64::from(ip) - 2 > term.adj_exp();
        sum = sum.add(&term, ip);
        if negligible {
            break;
        }
        k += 1;
    }
    sum
}

/// Reduce a mantissa `m` in `[1, 10)` by a power of two, returning `(j, t)` with
/// `t = m / 2^j` in `[3/4, 3/2)`. The thresholds `1.5`, `3`, `6` are exact, and
/// dividing by `2^j` is multiplying the coefficient by `5^j` and lowering the
/// exponent by `j`, so the reduction is exact.
fn reduce_by_two(m: &Work) -> (i64, Work) {
    let t1_5 = Work::new(false, DecBig::from_u32(15), -1);
    let t3 = Work::from_i64(3);
    let t6 = Work::from_i64(6);
    let j: i64 = if m.cmp_magnitude(&t1_5) == Ordering::Less {
        0
    } else if m.cmp_magnitude(&t3) == Ordering::Less {
        1
    } else if m.cmp_magnitude(&t6) == Ordering::Less {
        2
    } else {
        3
    };
    let five_pow = [1u32, 5, 25, 125][j as usize];
    let t = Work::new(false, m.coeff.mul(&DecBig::from_u32(five_pow)), m.exp - j);
    (j, t)
}

/// If `coeff * 10^exp` equals `10^n` for an integer `n`, return `Some(n)`. A
/// value is an exact power of ten exactly when its coefficient is one after its
/// trailing zeros are stripped.
fn power_of_ten_exponent(coeff: &DecBig, exp: i64) -> Option<i64> {
    if coeff.is_zero() {
        return None;
    }
    let mut c = coeff.clone();
    let mut zeros = 0i64;
    loop {
        let (q, r) = c.div_rem10();
        if r != 0 {
            break;
        }
        c = q;
        zeros += 1;
    }
    if c == DecBig::from_u32(1) {
        Some(exp + zeros)
    } else {
        None
    }
}
