//! The power function `power(x, y) = x ** y`.
//!
//! # Special cases (General Decimal Arithmetic / IEEE 754-2019 section 9.2.1)
//!
//! The ordered special-case table is derived from the spec and cross-checked
//! against `power.decTest`: a signaling NaN is `Invalid_operation`; a quiet NaN
//! propagates (with no `power(NaN, 0) = 1` or `power(1, NaN) = 1` exception,
//! unlike IEEE `pow`); `power(0, 0)` is `Invalid_operation`; `power(x, 0) = 1`
//! for any other `x`; `power(0, y)` is a signed zero or signed infinity (no
//! `Division_by_zero` flag) by the sign of `y` and the parity of `y`; the
//! infinite cases follow the magnitude and parity rules; a negative base with a
//! non-integer or infinite exponent is `Invalid_operation`; and `power(1, y)`
//! is `1` for integer `y` but the rounded `1.00...` (Inexact) otherwise.
//!
//! # General case (derived fresh)
//!
//! An integer exponent gives an exact result when it is feasible to compute:
//! `|x|^n` is formed exactly in [`DecBig`] by binary exponentiation (so the
//! cohort matches, `power(2, 3) = 8` not `8.000`), the sign set by the base sign
//! and the parity of `n`, and a negative exponent taken as the reciprocal. A
//! non-integer exponent (or an infeasibly large integer one) uses
//! `x^y = exp(y * ln|x|)`, evaluated through the `ln` and `exp` Work kernels at
//! an internal precision that covers the error amplification `exp` applies to
//! the product, then correctly rounded by the bounded Ziv strategy. Unlike
//! `exp` / `ln` / `log10`, `power` rounds with the context's rounding mode.
//!
//! Derived from `exp(y ln x)` and integer exponentiation; see Muller,
//! *Elementary Functions*. The independent oracle in `tests/pow_oracle.rs`
//! confirms the rounding against a from-scratch high-precision computation.

use super::consts::ConstCache;
use super::exp::exp_kernel;
use super::ln::{ln_kernel, power_of_ten_exponent};
use super::strategy::{finish, DEFAULT_STRATEGY};
use super::work::Work;
use crate::arith::{invalid_nan, quiet_from};
use crate::round::round_finite;
use crate::{Context, Decimal, Status};
use core::cmp::Ordering;
use ferrodec_multiword::DecBig;

/// Internal precision above the working precision for the general path.
const POW_GUARD: u32 = 8;

/// The general path's ulp error bound (the Ziv bracket half-width).
const POW_ERR: u128 = 4;

/// Maximum result digit count for the exact integer fast path. Beyond it the
/// general path is used (such a result either overflows the context or comes
/// from a near-one base, where `exp(y ln x)` is the only feasible route).
const EXACT_DIGIT_CAP: u128 = 20_000;

/// Whether a finite exponent is a non-integer, an even integer, or an odd one.
#[derive(Clone, Copy, PartialEq, Eq)]
enum IntKind {
    NonInteger,
    Even,
    Odd,
}

impl Decimal {
    /// `self` raised to the power `other`, correctly rounded under `ctx`.
    ///
    /// Rounds with `ctx.rounding` (unlike `exp` / `ln` / `log10`). An integer
    /// exponent gives an exact result where feasible. See the module
    /// documentation for the full special-case table.
    #[must_use]
    pub fn power(&self, other: &Self, ctx: &Context) -> (Decimal, Status) {
        let (x, y) = (self, other);

        // Signaling NaN, then quiet NaN propagation (no IEEE 0/1 exception).
        if x.is_signaling_nan() {
            return (quiet_from(x, ctx), Status::INVALID);
        }
        if y.is_signaling_nan() {
            return (quiet_from(y, ctx), Status::INVALID);
        }
        if x.is_nan() {
            return (quiet_from(x, ctx), Status::OK);
        }
        if y.is_nan() {
            return (quiet_from(y, ctx), Status::OK);
        }

        let y_kind = if y.is_finite() {
            integer_kind(y)
        } else {
            IntKind::NonInteger
        };
        let y_is_int = y.is_finite() && y_kind != IntKind::NonInteger;

        // power(x, 0): 1 unless x is zero, which is invalid.
        if y.is_zero() {
            return if x.is_zero() {
                (invalid_nan(), Status::INVALID)
            } else {
                (one(), Status::OK)
            };
        }

        // power(0, y != 0): signed zero (y > 0) or signed infinity (y < 0), the
        // sign set by -0 raised to an odd integer; no Division_by_zero flag.
        if x.is_zero() {
            let neg = x.is_negative() && y_kind == IntKind::Odd;
            return if y.is_negative() {
                (Decimal::infinity(neg), Status::OK)
            } else {
                (Decimal::finite(neg, DecBig::zero(), 0), Status::OK)
            };
        }

        // power(1, y): 1 for an integer exponent, else the rounded 1.00...
        if is_value_one(x) && !x.is_negative() {
            return if y_is_int {
                (one(), Status::OK)
            } else {
                rounded_one(ctx)
            };
        }

        if x.is_infinite() || y.is_infinite() {
            return power_infinite(x, y, y_kind);
        }

        // Both finite, nonzero, x != 1. A negative base needs an integer
        // exponent.
        if x.is_negative() && y_kind == IntKind::NonInteger {
            return (invalid_nan(), Status::INVALID);
        }

        let result_sign = x.is_negative() && y_kind == IntKind::Odd;

        if y_is_int {
            if let Some(result) = integer_power(x, y, result_sign, ctx) {
                return result;
            }
            // Infeasibly large integer exponent: fall through to the general
            // path (the result overflows / underflows, or the base is near one).
        }

        general_power(x, y, result_sign, ctx)
    }
}

/// The exact integer-exponent path, or `None` if the exponent magnitude makes
/// the exact result too large to form (the caller then uses the general path).
fn integer_power(
    x: &Decimal,
    y: &Decimal,
    result_sign: bool,
    ctx: &Context,
) -> Option<(Decimal, Status)> {
    let (_, base_coeff, base_exp) = x.finite_parts().expect("finite");
    let n_db = integer_magnitude(y);
    let n = n_db.to_u128()?;
    let base_digits = u128::from(base_coeff.decimal_digit_count());
    if base_digits.saturating_mul(n) > EXACT_DIGIT_CAP {
        return None;
    }
    let n = u64::try_from(n).ok()?;

    // |x|^n exactly: coefficient^n, exponent * n.
    let mag_coeff = pow_decbig(base_coeff, n);
    // n is bounded by the digit cap, so it fits i64.
    let mag_exp = i64::from(base_exp) * (n as i64);

    if y.is_negative() {
        // x^(-n) = 1 / (signed |x|^n), correctly rounded by division.
        let denom = Decimal::finite(result_sign, mag_coeff, exp_to_i32(mag_exp));
        Some(one().divide(&denom, ctx))
    } else {
        // Exact integer power, rounded to the context (Inexact only if it
        // exceeds the precision).
        Some(round_finite(
            result_sign,
            mag_coeff,
            mag_exp,
            false,
            mag_exp,
            ctx,
            Status::OK,
        ))
    }
}

/// The general path `x^y = exp(y * ln|x|)`, rounded with the context's rounding.
fn general_power(x: &Decimal, y: &Decimal, result_sign: bool, ctx: &Context) -> (Decimal, Status) {
    let x_abs = Work::from_decimal(&x.copy_abs());
    let y_work = Work::from_decimal(y);
    let span = i64::from(ctx.emax) - i64::from(ctx.emin) + i64::from(ctx.precision) + 16;
    let amp = digit_count(span);
    let mut cache = ConstCache::new();

    // Far-field gate: if y*ln|x| is past the representable exponent span, the
    // result is unambiguously +Infinity (product > 0) or +0 (product < 0).
    let ln_lo = ln_kernel(&x_abs, 30, &mut cache);
    let prod_lo = y_work.mul_to(&ln_lo, 30);
    let bound = cache.ln10(24).mul(&Work::from_i64(span));
    if prod_lo.cmp_magnitude(&bound) == Ordering::Greater {
        return if prod_lo.sign {
            far_underflow(result_sign, ctx)
        } else {
            far_overflow(result_sign, ctx)
        };
    }

    finish(ctx, POW_ERR, DEFAULT_STRATEGY, |wp| {
        // exp amplifies the product's absolute error by |product|, so ln is
        // taken with `amp` extra digits to keep that below the working ulp.
        let ip = wp + POW_GUARD;
        let ip_ln = ip + amp + 4;
        let ln_x = ln_kernel(&x_abs, ip_ln, &mut cache);
        let prod = y_work.mul_to(&ln_x, ip_ln);
        let mut r = exp_kernel(&prod, ip, &mut cache);
        r.sign = result_sign;
        r.normalize_to(wp);
        r
    })
}

/// The `power` special cases where `x` or `y` is infinite (and neither is NaN,
/// `y != 0`, and `power(1, ...)` is already handled).
fn power_infinite(x: &Decimal, y: &Decimal, y_kind: IntKind) -> (Decimal, Status) {
    if x.is_infinite() {
        if y.is_infinite() {
            // (+-Inf) ^ (+-Inf): -Inf base is invalid; +Inf^+Inf = +Inf,
            // +Inf^-Inf = +0.
            if x.is_negative() {
                return (invalid_nan(), Status::INVALID);
            }
            return if y.is_negative() {
                (Decimal::finite(false, DecBig::zero(), 0), Status::OK)
            } else {
                (Decimal::infinity(false), Status::OK)
            };
        }
        // x = +-Inf, y finite nonzero.
        if x.is_negative() {
            if y_kind == IntKind::NonInteger {
                return (invalid_nan(), Status::INVALID);
            }
            let neg = y_kind == IntKind::Odd;
            return if y.is_negative() {
                (Decimal::finite(neg, DecBig::zero(), 0), Status::OK)
            } else {
                (Decimal::infinity(neg), Status::OK)
            };
        }
        // +Inf ^ y: +Inf for y > 0, +0 for y < 0.
        return if y.is_negative() {
            (Decimal::finite(false, DecBig::zero(), 0), Status::OK)
        } else {
            (Decimal::infinity(false), Status::OK)
        };
    }

    // x finite nonzero, y = +-Inf. A negative base is invalid.
    if x.is_negative() {
        return (invalid_nan(), Status::INVALID);
    }
    // Positive base by magnitude versus one (|x| = 1 was handled as x = 1).
    let lt_one = x.adj_exp_value() < 0;
    let to_infinity = (lt_one && y.is_negative()) || (!lt_one && !y.is_negative());
    if to_infinity {
        (Decimal::infinity(false), Status::OK)
    } else {
        (Decimal::finite(false, DecBig::zero(), 0), Status::OK)
    }
}

/// The result when the power overflows: a magnitude past `Emax` with the
/// result's sign, which `round_finite` resolves to infinity or `Nmax` per the
/// context's rounding, with `Overflow` and `Inexact`.
fn far_overflow(sign: bool, ctx: &Context) -> (Decimal, Status) {
    let e = i64::from(ctx.emax) + 2;
    round_finite(sign, DecBig::from_u32(1), e, true, e, ctx, Status::OK)
}

/// The result when the power underflows far below `Etiny`: a signed zero at
/// `Etiny` with `Underflow`, `Inexact`, and `Clamped`, regardless of the
/// rounding mode. The reference computes `exp(y ln x)` at a working precision
/// whose exponent range the product exceeds, so the internal exponential is
/// exactly zero before the final rounding; a value that small never rounds up
/// to the smallest subnormal even under round-away (the near-`Etiny` cases that
/// do round per mode go through the general Ziv path, not this gate).
fn far_underflow(sign: bool, ctx: &Context) -> (Decimal, Status) {
    let etiny = i64::from(ctx.emin) - i64::from(ctx.precision) + 1;
    let status = Status::UNDERFLOW | Status::INEXACT | Status::CLAMPED;
    (Decimal::finite(sign, DecBig::zero(), etiny as i32), status)
}

/// `1`, exact.
fn one() -> Decimal {
    Decimal::finite(false, DecBig::from_u32(1), 0)
}

/// `1` padded to the precision and flagged `Inexact`, the result of `power(1, y)`
/// for a non-integer or infinite `y` (e.g. `1.00000000`).
fn rounded_one(ctx: &Context) -> (Decimal, Status) {
    let ideal = -i64::from(ctx.precision.saturating_sub(1));
    round_finite(false, DecBig::from_u32(1), 0, true, ideal, ctx, Status::OK)
}

/// True when `|d|` is exactly one (any cohort).
fn is_value_one(d: &Decimal) -> bool {
    let Some((_, coeff, exp)) = d.finite_parts() else {
        return false;
    };
    power_of_ten_exponent(coeff, i64::from(exp)) == Some(0)
}

/// Classify a finite exponent as non-integer, even integer, or odd integer.
fn integer_kind(y: &Decimal) -> IntKind {
    let (_, coeff, exp) = y.finite_parts().expect("finite");
    if coeff.is_zero() {
        return IntKind::Even;
    }
    if exp >= 0 {
        // coeff * 10^exp: a positive exponent multiplies in a factor of ten.
        if exp > 0 {
            IntKind::Even
        } else {
            parity(coeff.div_rem10().1)
        }
    } else {
        let (q, r) = coeff.div_rem_pow10((-exp) as u32);
        if r.is_zero() {
            parity(q.div_rem10().1)
        } else {
            IntKind::NonInteger
        }
    }
}

/// The non-negative integer value of an integral exponent's magnitude.
fn integer_magnitude(y: &Decimal) -> DecBig {
    let (_, coeff, exp) = y.finite_parts().expect("finite");
    if exp >= 0 {
        coeff.mul_pow10(exp as u32)
    } else {
        coeff.div_rem_pow10((-exp) as u32).0
    }
}

/// Even/odd of a single digit.
fn parity(last_digit: u32) -> IntKind {
    if last_digit % 2 == 0 {
        IntKind::Even
    } else {
        IntKind::Odd
    }
}

/// `base ** n` by binary exponentiation.
fn pow_decbig(base: &DecBig, mut n: u64) -> DecBig {
    let mut result = DecBig::from_u32(1);
    let mut b = base.clone();
    while n > 0 {
        if n & 1 == 1 {
            result = result.mul(&b);
        }
        n >>= 1;
        if n > 0 {
            b = b.mul(&b);
        }
    }
    result
}

/// Decimal digit count of a positive `i64`, `1` for zero or negative-or-less.
fn digit_count(n: i64) -> u32 {
    if n <= 0 {
        1
    } else {
        (n as u64).ilog10() + 1
    }
}

/// Saturating `i64` exponent into the `i32` a `Decimal` stores. Used only to
/// build the reciprocal's denominator, which `divide` then rounds; a saturated
/// extreme simply rounds to zero or infinity, the same outcome as the exact
/// value.
fn exp_to_i32(e: i64) -> i32 {
    e.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

impl Decimal {
    /// The adjusted exponent of a finite value as an `i64` (the power of ten of
    /// its leading digit), for the infinite-exponent magnitude test.
    fn adj_exp_value(&self) -> i64 {
        let (_, coeff, exp) = self.finite_parts().expect("finite");
        i64::from(exp) + coeff.decimal_digit_count() as i64 - 1
    }
}
