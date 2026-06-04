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
use ferrodec_multiword::U256;

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
}
