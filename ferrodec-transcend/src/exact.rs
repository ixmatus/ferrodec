//! Exact-result detection for `cbrt` and `pow` (IEEE 754-2019 §7.5).
//!
//! ## Why
//!
//! The §9.2 kernels evaluate at 50-digit [`Extended`] precision and round
//! once at the format boundary. `exp_from_extended` (`exp.rs`) raises
//! `INEXACT` unconditionally, because the extended-precision approximation
//! of a transcendental value almost never lands exactly and its own
//! round-step inexact bit reflects "rounded 50 digits down to the format
//! width", not "the true result differs from the delivered one". For
//! `exp`, `ln`, the trig and hyperbolic families that is correct: the
//! true result is irrational for every non-special input, and the special
//! inputs short-circuit. `cbrt` and `pow` are the exceptions: they can
//! land on an exact, representable value (a perfect cube root, an exact
//! integer or rational power), and §7.5 forbids `INEXACT` there. This
//! module proves exactness so the two kernels can clear the flag.
//!
//! ## Soundness (the invariant the whole module is built around)
//!
//! Every predicate defaults to "not proven" (returns `false`): any
//! coefficient that would exceed the fixed-width envelope, any exponent
//! that would overflow `i32`, any exponent magnitude past `u32` simply
//! bails. A `false` result leaves today's `INEXACT` in place, which is
//! always safe. The only dangerous outcome would be a `true` on a result
//! that is *not* exact, which would clear a real `INEXACT`; the bounds are
//! chosen so that can never happen. The proof leans on the kernels being
//! correctly rounded (ADR-0032): if the infinitely precise result were
//! exact and representable, correct rounding delivers it exactly, so the
//! cube / power check below sees the true value and succeeds.
//!
//! `no_std`, alloc-free: fixed-width [`U256`] / `U384` integer math only.

use crate::extended::{u256_mul_u256, Extended};
use crate::format::DecimalFormat;
use core::cmp::Ordering;
use ferrodec_ieee::Status;
use ferrodec_multiword::{U256, U384};

/// Drop the `INEXACT` flag after a positive exactness proof. All other
/// flags pass through unchanged. `UNDERFLOW` is deliberately *not* cleared
/// (an exact subnormal result is an astronomically rare boundary; see
/// ADR-0047), so the worst case is a spurious tininess flag, never a
/// wrongly suppressed one.
#[inline]
pub(crate) fn clear_inexact(status: Status) -> Status {
    Status::from_bits_truncate(status.bits() & !Status::INEXACT.bits())
}

/// Remove trailing decimal zeros from `(coef, exp)`, raising `exp` by one
/// for each zero stripped. Canonicalises a cohort so two values that are
/// numerically equal but stored at different quanta compare equal after
/// stripping. Bounded by the U256 digit cap (≤ 78 iterations).
pub(crate) fn strip_trailing_zeros(mut coef: U256, mut exp: i32) -> (U256, i32) {
    if coef.is_zero() {
        return (coef, exp);
    }
    loop {
        let (q, r) = coef.div_rem10();
        if r != 0 {
            break;
        }
        coef = q;
        exp += 1;
    }
    (coef, exp)
}

/// Exact `a × b` as a [`U256`], or `None` when the product would exceed
/// `U256`'s ~78-digit envelope. The combined-digit guard keeps the
/// intermediate `U384` product inside its ~115-digit capacity (so the
/// `u256_mul_u256` precondition holds), and the top-limb check then
/// rejects any product that does not fit `U256`.
fn checked_mul_u256(a: U256, b: U256) -> Option<U256> {
    if a.decimal_digit_count() + b.decimal_digit_count() > 115 {
        return None;
    }
    let prod = u256_mul_u256(a, b);
    if prod.hi != 0 {
        return None;
    }
    Some(U256 {
        lo: prod.lo,
        hi: prod.mid,
    })
}

/// `c³` as a [`U256`], or `None` if `c` is too wide to be an exact cube
/// root of a representable value. A perfect cube root of a value with at
/// most 34 significant digits has at most 12 significant digits (since
/// `(10^12)³ = 10^36` already exceeds the widest format coefficient), so
/// a wider `c` cannot be exact and bails. The retained range keeps `c³`
/// below ~36 digits, comfortably inside `U256`.
fn cube_u256(c: U256) -> Option<U256> {
    if c.decimal_digit_count() > 12 {
        return None;
    }
    let c2 = checked_mul_u256(c, c)?;
    checked_mul_u256(c2, c)
}

/// Exact value equality of `c1 · 10^e1` and `c2 · 10^e2`. Builds two
/// [`Extended`] magnitudes directly (no rounding) and compares with
/// [`Extended::cmp`], which aligns the coefficients in `U384` before
/// comparing. Callers must keep both coefficients inside `U256`'s
/// envelope so the alignment stays inside `U384`.
fn value_eq(c1: U256, e1: i32, c2: U256, e2: i32) -> bool {
    let a = Extended {
        coef: c1,
        exp: e1,
        sign: false,
    };
    let b = Extended {
        coef: c2,
        exp: e2,
        sign: false,
    };
    a.cmp(b) == Ordering::Equal
}

/// `true` when `result` is the exact cube root of `x` (a perfect cube).
///
/// Caller guarantees `result` and `x` are finite (NaN / Inf / overflow are
/// filtered at the kernel before this is reached). Compares magnitudes
/// only: `cbrt` is odd and the kernel re-applies the sign, so
/// `sign(result) == sign(x)` by construction and `result³ == x` reduces to
/// `|result|³ == |x|`.
pub(crate) fn cube_is_exact<F: DecimalFormat>(result: F, x: F) -> bool {
    let (cr, er, _) = result.to_extended_parts();
    let (cx, ex, _) = x.to_extended_parts();
    if cr.is_zero() || cx.is_zero() {
        return false;
    }
    let (cr, er) = strip_trailing_zeros(cr, er);
    let (cx, ex) = strip_trailing_zeros(cx, ex);
    let Some(cr3) = cube_u256(cr) else {
        return false;
    };
    let Some(exp3) = er.checked_mul(3) else {
        return false;
    };
    value_eq(cr3, exp3, cx, ex)
}

/// Count the factor `p` (2 or 5) in `n`. Caller guarantees `n >= 1` (a
/// zero `n` would loop forever, since `0 % p == 0`).
fn factor_count(mut n: u128, p: u128) -> u32 {
    let mut c = 0;
    while n % p == 0 {
        n /= p;
        c += 1;
    }
    c
}

/// Reduce `|y| = cy · 10^ey` to a lowest-terms rational `a / b`, where
/// `cy >= 1` is already stripped of trailing zeros. Returns `None` when
/// the numerator or denominator overflows `u128` — such a `y` has no
/// representable exact power, so the caller keeps INEXACT.
///
/// For `ey >= 0` the value is the integer `cy · 10^ey` over 1. For
/// `ey < 0` write `y = cy / 10^d` (`d = -ey`) and cancel the common
/// factors of 2 and 5: a stripped `cy` shares no factor of 10, so it has
/// at most one of the two, and `gcd(cy, 10^d) = 2^min(v2,d) · 5^min(v5,d)`.
fn reduce_rational(cy: u128, ey: i32) -> Option<(u128, u128)> {
    if ey >= 0 {
        let scale = 10u128.checked_pow(u32::try_from(ey).ok()?)?;
        let a = cy.checked_mul(scale)?;
        return Some((a, 1));
    }
    let d = (-ey) as u32;
    let v2 = factor_count(cy, 2).min(d);
    let v5 = factor_count(cy, 5).min(d);
    let a = cy / 2u128.pow(v2) / 5u128.pow(v5);
    let pow2 = 2u128.checked_pow(d - v2)?;
    let pow5 = 5u128.checked_pow(d - v5)?;
    let b = pow2.checked_mul(pow5)?;
    Some((a, b))
}

/// `base^exp` as a [`U256`], or `None` once an intermediate product would
/// exceed `U256`. Square-and-multiply; each multiply is bounds-checked, so
/// a base above 1 bails as soon as the running power outgrows the
/// envelope. `base = 1` stays at 1 for any `exp` (the power-of-ten cases).
fn int_pow_u256(base: U256, exp: u32) -> Option<U256> {
    let mut result = U256::from_u128(1);
    let mut b = base;
    let mut e = exp;
    while e > 0 {
        if e & 1 == 1 {
            result = checked_mul_u256(result, b)?;
        }
        e >>= 1;
        if e > 0 {
            b = checked_mul_u256(b, b)?;
        }
    }
    Some(result)
}

/// `true` when `prod · 10^exp` is exactly the value 1. Strips trailing
/// zeros from the product, then requires the residue to be 1 and the
/// accumulated exponent to cancel to zero.
fn value_is_one(prod: U384, exp: i32) -> bool {
    if prod.is_zero() {
        return false;
    }
    let mut p = prod;
    let mut zeros: i64 = 0;
    loop {
        let (q, r) = p.div_rem10();
        if r != 0 {
            break;
        }
        p = q;
        zeros += 1;
    }
    p == U384::from_u128(1) && i64::from(exp) + zeros == 0
}

/// `true` when `result` is the exact value of `x^y` (an exact integer or
/// rational power).
///
/// Caller guarantees `result`, `x`, `y` are finite (NaN / Inf / overflow
/// are filtered at the kernel). Writes `|y| = a / b` in lowest terms and
/// checks the relation in exact fixed-width integer arithmetic on the
/// canonical coefficients: for `y > 0`, `|result|^b == |x|^a`; for
/// `y < 0`, `|result|^b · |x|^a == 1` (since `result = x^{-a/b}`). Sign of
/// the result is handled by the kernel's odd-integer negation, so only
/// magnitudes are compared. Any width or exponent overflow bails to
/// `false`, leaving INEXACT in place.
pub(crate) fn power_is_exact<F: DecimalFormat>(result: F, x: F, y: F) -> bool {
    let (cr, er, _) = result.to_extended_parts();
    let (cx, ex, _) = x.to_extended_parts();
    let (cy, ey, y_neg) = y.to_extended_parts();
    if cr.is_zero() || cx.is_zero() || cy.is_zero() {
        return false;
    }
    let (cr, er) = strip_trailing_zeros(cr, er);
    let (cx, ex) = strip_trailing_zeros(cx, ex);
    let (cy, ey) = strip_trailing_zeros(cy, ey);
    // A representable exponent coefficient fits u128; bail defensively.
    if cy.hi != 0 {
        return false;
    }
    let Some((a, b)) = reduce_rational(cy.lo, ey) else {
        return false;
    };
    let Ok(a32) = u32::try_from(a) else {
        return false;
    };
    let Ok(b32) = u32::try_from(b) else {
        return false;
    };
    let Some(p) = int_pow_u256(cr, b32) else {
        return false;
    };
    let Some(q) = int_pow_u256(cx, a32) else {
        return false;
    };
    // |result|^b carries exponent er·b; |x|^a carries ex·a.
    let Ok(ep) = i32::try_from(i64::from(er) * i64::from(b32)) else {
        return false;
    };
    let Ok(eq) = i32::try_from(i64::from(ex) * i64::from(a32)) else {
        return false;
    };
    if y_neg {
        // result = x^{-a/b}, so result^b · x^a must equal 1. Keep the
        // product inside U384.
        if p.decimal_digit_count() + q.decimal_digit_count() > 115 {
            return false;
        }
        let Some(total) = ep.checked_add(eq) else {
            return false;
        };
        value_is_one(u256_mul_u256(p, q), total)
    } else {
        // result = x^{a/b}, so result^b == x^a.
        value_eq(p, ep, q, eq)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(n: u128) -> U256 {
        U256::from_u128(n)
    }

    #[test]
    fn clear_inexact_drops_only_inexact() {
        let s = Status::INEXACT | Status::UNDERFLOW;
        let cleared = clear_inexact(s);
        assert!(!cleared.inexact());
        assert!(cleared.underflow(), "UNDERFLOW is deliberately preserved");
        // Idempotent on an already-clear status.
        assert!(!clear_inexact(Status::OK).inexact());
        // Leaves an unrelated flag untouched.
        assert!(clear_inexact(Status::OVERFLOW).overflow());
    }

    #[test]
    fn strip_trailing_zeros_canonicalises() {
        // 8000 × 10^-3 == 8 × 10^0.
        assert_eq!(strip_trailing_zeros(u(8000), -3), (u(8), 0));
        // 1 × 10^72 has no trailing zeros in the coefficient.
        assert_eq!(strip_trailing_zeros(u(1), 72), (u(1), 72));
        // 100 × 10^0 == 1 × 10^2.
        assert_eq!(strip_trailing_zeros(u(100), 0), (u(1), 2));
        // Zero is left alone.
        assert_eq!(strip_trailing_zeros(U256::ZERO, -5), (U256::ZERO, -5));
        // No trailing zero: unchanged.
        assert_eq!(strip_trailing_zeros(u(123), -2), (u(123), -2));
    }

    #[test]
    fn cube_u256_small_values() {
        assert_eq!(cube_u256(u(2)), Some(u(8)));
        assert_eq!(cube_u256(u(3)), Some(u(27)));
        assert_eq!(cube_u256(u(10)), Some(u(1000)));
        // 12-digit input is in range: (10^12 - 1)³ < 10^36 fits U256.
        let c = u(999_999_999_999);
        let cubed = cube_u256(c).expect("12-digit cube fits");
        // Cross-check against an independent U256 multiply.
        let sq = checked_mul_u256(c, c).unwrap();
        assert_eq!(cube_u256(c), checked_mul_u256(sq, c));
        let _ = cubed;
    }

    #[test]
    fn cube_u256_bails_above_twelve_digits() {
        // 13-digit input: cannot be an exact cube root of a ≤34-digit value.
        assert_eq!(cube_u256(u(10_000_000_000_000)), None);
    }

    #[test]
    fn checked_mul_u256_detects_overflow() {
        // 40-digit × 40-digit = ~80 digits, exceeds U256 (~78).
        let big = U256::from_u128(1).mul_pow10(40);
        assert_eq!(checked_mul_u256(big, big), None);
        // 30-digit × 30-digit = ~60 digits, fits.
        let mid = U256::from_u128(1).mul_pow10(30);
        assert_eq!(
            checked_mul_u256(mid, mid),
            Some(U256::from_u128(1).mul_pow10(60))
        );
    }

    #[test]
    fn value_eq_ignores_cohort() {
        // 8 × 10^0 == 8000 × 10^-3 == 80 × 10^-1.
        assert!(value_eq(u(8), 0, u(8000), -3));
        assert!(value_eq(u(8000), -3, u(80), -1));
        // 27 × 10^0 == 27, but not 27 × 10^1.
        assert!(value_eq(u(27), 0, u(27), 0));
        assert!(!value_eq(u(27), 0, u(27), 1));
        // Different decades.
        assert!(!value_eq(u(2), 0, u(2), 3));
        // 1 × 10^72 == 10^72.
        assert!(value_eq(u(1), 72, U256::from_u128(1).mul_pow10(72), 0));
    }

    #[test]
    fn factor_count_counts_factors() {
        assert_eq!(factor_count(8, 2), 3);
        assert_eq!(factor_count(125, 5), 3);
        assert_eq!(factor_count(40, 2), 3); // 40 = 2^3 · 5
        assert_eq!(factor_count(40, 5), 1);
        assert_eq!(factor_count(7, 2), 0);
        assert_eq!(factor_count(1, 2), 0);
    }

    #[test]
    fn reduce_rational_integer_y() {
        // y = 3 (cy=3, ey=0) -> 3/1.
        assert_eq!(reduce_rational(3, 0), Some((3, 1)));
        // y = 300 (cy=3, ey=2) -> 300/1.
        assert_eq!(reduce_rational(3, 2), Some((300, 1)));
        // y = 1 -> 1/1.
        assert_eq!(reduce_rational(1, 0), Some((1, 1)));
    }

    #[test]
    fn reduce_rational_fractions() {
        // 0.5 = 5 × 10^-1 -> 1/2.
        assert_eq!(reduce_rational(5, -1), Some((1, 2)));
        // 0.25 = 25 × 10^-2 -> 1/4.
        assert_eq!(reduce_rational(25, -2), Some((1, 4)));
        // 0.2 = 2 × 10^-1 -> 1/5.
        assert_eq!(reduce_rational(2, -1), Some((1, 5)));
        // 1.5 = 15 × 10^-1 -> 3/2.
        assert_eq!(reduce_rational(15, -1), Some((3, 2)));
        // 0.123 = 123 × 10^-3 -> 123/1000 (123 shares no 2 or 5).
        assert_eq!(reduce_rational(123, -3), Some((123, 1000)));
        // 0.75 = 75 × 10^-2 -> 3/4.
        assert_eq!(reduce_rational(75, -2), Some((3, 4)));
    }

    #[test]
    fn reduce_rational_bails_on_overflow() {
        // ey large enough that 10^ey overflows u128 (>38 digits).
        assert_eq!(reduce_rational(1, 39), None);
        // Tiny fraction whose denominator 10^d overflows: d=40, cy coprime
        // to 10, so b = 10^40 overflows u128.
        assert_eq!(reduce_rational(3, -40), None);
    }

    #[test]
    fn int_pow_u256_basic() {
        assert_eq!(int_pow_u256(u(2), 0), Some(u(1)));
        assert_eq!(int_pow_u256(u(2), 10), Some(u(1024)));
        assert_eq!(int_pow_u256(u(5), 2), Some(u(25)));
        assert_eq!(int_pow_u256(u(10), 3), Some(u(1000)));
        // base 1 stays 1 for any exponent (the power-of-ten case).
        assert_eq!(int_pow_u256(u(1), 6000), Some(u(1)));
    }

    #[test]
    fn int_pow_u256_bails_on_overflow() {
        // 2^300 ≈ 10^90, far past U256's ~78 digits.
        assert_eq!(int_pow_u256(u(2), 300), None);
        // A 34-digit base squared twice already exceeds U256.
        let wide = U256::from_u128(1).mul_pow10(33); // 34 digits
        assert_eq!(int_pow_u256(wide, 4), None);
    }

    #[test]
    fn value_is_one_detects_unit_value() {
        use crate::extended::u256_mul_u256;
        // 100 × 10^-2 == 1.
        assert!(value_is_one(U384::from_u128(100), -2));
        // 1 × 10^0 == 1.
        assert!(value_is_one(U384::from_u128(1), 0));
        // 25 × 4 × 10^-2 == 1 (the pow(4,-0.5) shape).
        let prod = u256_mul_u256(u(25), u(4));
        assert!(value_is_one(prod, -2));
        // 100 × 10^-1 == 10, not 1.
        assert!(!value_is_one(U384::from_u128(100), -1));
        // 2 × 10^0 == 2, not 1.
        assert!(!value_is_one(U384::from_u128(2), 0));
        // Zero is not 1.
        assert!(!value_is_one(U384::ZERO, 0));
    }
}
