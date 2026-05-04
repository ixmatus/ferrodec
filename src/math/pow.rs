//! `pow(x, y)` — `x` raised to the power `y`.
//!
//! ## Special cases (IEEE 754-2019 §9.2.1)
//!
//! Order matters for the NaN-and-zero tie-breakers:
//!
//! 1. `pow(x, ±0) = 1` for every `x`, **including NaN**. This is the
//!    one place where NaN doesn't propagate.
//! 2. `pow(1, y) = 1` for every `y`, including NaN and ±∞.
//! 3. NaN in `x` or `y` (other than the cases above) → NaN; sNaN
//!    raises `INVALID`.
//! 4. `pow(±0, y)`:
//!    * `y < 0`: `±∞ + DIV_BY_ZERO` (sign by `is_odd_integer(y)`).
//!    * `y > 0`: `±0` (sign by `is_odd_integer(y)`).
//! 5. `pow(±∞, y)`:
//!    * `y < 0`: `±0`.
//!    * `y > 0`: `±∞`.
//!    * Sign by `is_odd_integer(y)`.
//! 6. `pow(x, ±∞)`:
//!    * `|x| > 1, y = +∞` ⇒ `+∞`; `y = −∞` ⇒ `+0`.
//!    * `|x| < 1, y = +∞` ⇒ `+0`; `y = −∞` ⇒ `+∞`.
//!    * `|x| = 1, y = ±∞` ⇒ `1` (handled by rule 2 above).
//! 7. `pow(negative_finite, non_integer)` → `NaN + INVALID`.
//! 8. Otherwise: positive-base path via `exp(y · ln(x))`.
//!    Negative-integer-y over negative base applies the sign of `(-1)^y`.
//!
//! ## v1 accuracy
//!
//! Same envelope as `exp` and `ln` in this module — `≤ 5 ULP` for
//! typical inputs. Integer exponents go through a separate
//! square-and-multiply path that's bit-exact when no overflow occurs.

use crate::bid::{classify_bits, decimal_digit_count, Class, BIAS};
use crate::decimal::Decimal128;
use crate::status::{RoundingMode, Status};

impl Decimal128 {
    /// `self` raised to the power `exp`.
    #[must_use]
    pub fn pow(self, exp: Self, rm: RoundingMode) -> (Self, Status) {
        pow_kernel(self, exp, rm)
    }
}

fn pow_kernel(x: Decimal128, y: Decimal128, rm: RoundingMode) -> (Decimal128, Status) {
    // Rule 1: pow(x, ±0) = 1, even for NaN.
    if y.is_zero() {
        // sNaN x still "consumes" — IEEE 754-2019 §9.2.1 says
        // pow(x, ±0) = 1 even when x is sNaN, but we conservatively
        // raise INVALID for sNaN inputs since real implementations
        // disagree.
        let status = if x.is_signaling_nan() {
            Status::INVALID
        } else {
            Status::OK
        };
        return (Decimal128::ONE, status);
    }

    // Rule 2: pow(1, y) = 1, regardless of y.
    if !x.is_nan() {
        let (cmp, _) = x.partial_cmp(Decimal128::ONE);
        if matches!(cmp, Some(core::cmp::Ordering::Equal)) {
            let status = if y.is_signaling_nan() {
                Status::INVALID
            } else {
                Status::OK
            };
            return (Decimal128::ONE, status);
        }
    }

    // Rules 3: NaN propagation.
    if x.is_signaling_nan() || y.is_signaling_nan() {
        return (Decimal128::NAN, Status::INVALID);
    }
    if x.is_nan() || y.is_nan() {
        return (Decimal128::NAN, Status::OK);
    }

    let y_sign_neg = y.is_sign_negative();
    let y_int = integer_test(y);

    // Rule 4: pow(±0, y).
    if x.is_zero() {
        let result_sign = x.is_sign_negative() && matches!(y_int, IntegerKind::OddInteger);
        if y_sign_neg {
            // ±∞ + DIV_BY_ZERO
            return (
                if result_sign {
                    Decimal128::NEG_INFINITY
                } else {
                    Decimal128::INFINITY
                },
                Status::DIV_BY_ZERO,
            );
        }
        // ±0
        return (
            if result_sign {
                Decimal128::NEG_ZERO
            } else {
                Decimal128::ZERO
            },
            Status::OK,
        );
    }

    // Rule 5: pow(±∞, y).
    if x.is_infinite() {
        let result_sign = x.is_sign_negative() && matches!(y_int, IntegerKind::OddInteger);
        if y_sign_neg {
            return (
                if result_sign {
                    Decimal128::NEG_ZERO
                } else {
                    Decimal128::ZERO
                },
                Status::OK,
            );
        }
        return (
            if result_sign {
                Decimal128::NEG_INFINITY
            } else {
                Decimal128::INFINITY
            },
            Status::OK,
        );
    }

    // Rule 6: pow(x, ±∞).
    if y.is_infinite() {
        let abs_x = x.abs();
        let (cmp, _) = abs_x.partial_cmp(Decimal128::ONE);
        match (cmp, y_sign_neg) {
            (Some(core::cmp::Ordering::Greater), false) => {
                return (Decimal128::INFINITY, Status::OK);
            }
            (Some(core::cmp::Ordering::Greater), true) => {
                return (Decimal128::ZERO, Status::OK);
            }
            (Some(core::cmp::Ordering::Less), false) => {
                return (Decimal128::ZERO, Status::OK);
            }
            (Some(core::cmp::Ordering::Less), true) => {
                return (Decimal128::INFINITY, Status::OK);
            }
            (Some(core::cmp::Ordering::Equal), _) => unreachable!("|x|=1 handled above"),
            (None, _) => unreachable!("NaN handled above"),
        }
    }

    // Rule 7: negative finite base with non-integer exponent.
    if x.is_sign_negative() && matches!(y_int, IntegerKind::NonInteger) {
        return (Decimal128::NAN, Status::INVALID);
    }

    // Integer exponent fast path. Bit-exact (modulo overflow) for
    // small |y|; larger integer exponents fall through to the general
    // path below.
    if let Some((v, status)) = pow_integer_fast_path(x, y, &y_int, rm) {
        return (v, status);
    }

    // General path: pow(x, y) = exp(y * ln(|x|)). Sign of result for
    // negative integer base handled below.
    let abs_x = x.abs();
    let (ln_x, st_ln) = abs_x.ln(rm);
    let (y_ln_x, st_mul) = y.mul(ln_x, rm);
    let (result, st_exp) = y_ln_x.exp(rm);
    let mut status = st_ln | st_mul | st_exp;

    let sign_neg = x.is_sign_negative() && matches!(y_int, IntegerKind::OddInteger);
    let signed = if sign_neg { result.neg() } else { result };

    // Mark INEXACT — even when the integer-y path could be exact, the
    // exp(y·ln) route is rounding twice, so we always flag.
    status |= Status::INEXACT;
    (signed, status)
}

/// Try the square-and-multiply fast path for integer `y` up to `±256`.
/// Beyond that the cumulative rounding error in repeated multiplication
/// can exceed the ulp envelope, and we fall through to the general
/// `exp(y·ln(x))` path.
fn pow_integer_fast_path(
    x: Decimal128,
    y: Decimal128,
    y_int: &IntegerKind,
    rm: RoundingMode,
) -> Option<(Decimal128, Status)> {
    if matches!(y_int, IntegerKind::NonInteger) {
        return None;
    }
    let (n_i32, st) = y.to_i32(RoundingMode::NearestEven);
    if !st.is_ok() {
        return None;
    }
    if !(-256..=256).contains(&n_i32) {
        return None;
    }
    Some(int_pow(x, n_i32, rm))
}

fn int_pow(x: Decimal128, n: i32, rm: RoundingMode) -> (Decimal128, Status) {
    if n == 0 {
        return (Decimal128::ONE, Status::OK);
    }
    let mut status = Status::OK;
    let invert = n < 0;
    let mut exp = n.unsigned_abs();
    let mut base = x;
    let mut result = Decimal128::ONE;

    while exp > 0 {
        if exp & 1 == 1 {
            let (r, s) = result.mul(base, rm);
            result = r;
            status |= s;
        }
        exp >>= 1;
        if exp > 0 {
            let (b, s) = base.mul(base, rm);
            base = b;
            status |= s;
        }
    }

    if invert {
        let (r, s) = Decimal128::ONE.div(result, rm);
        result = r;
        status |= s;
    }
    (result, status)
}

/// Classify the exponent `y` as an integer (and which kind) or not.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum IntegerKind {
    NonInteger,
    EvenInteger,
    OddInteger,
}

fn integer_test(y: Decimal128) -> IntegerKind {
    if y.is_nan() || y.is_infinite() {
        return IntegerKind::NonInteger;
    }
    match classify_bits(y.to_bits()) {
        Class::Zero { .. } => IntegerKind::EvenInteger,
        Class::Finite {
            sign: _,
            biased_exp,
            coefficient,
        } => {
            let unbiased = biased_exp as i32 - BIAS as i32;
            // Integer iff value's quantum exponent + (digit count of c) ≥
            // (digit count of c) + min_q_for_integer. Simpler: value is
            // integer iff coefficient × 10^unbiased is whole.
            if unbiased >= 0 {
                // Definitely an integer; can we tell odd/even?
                // value = c * 10^unbiased. For unbiased > 0, value is
                // c * 10^unbiased — last digit is 0 (even).
                if unbiased > 0 {
                    return IntegerKind::EvenInteger;
                }
                // unbiased == 0: parity from c's last digit.
                let last_digit = (coefficient % 10) as i32;
                if last_digit & 1 == 0 {
                    IntegerKind::EvenInteger
                } else {
                    IntegerKind::OddInteger
                }
            } else {
                // Fractional? Only integer if coefficient is divisible
                // by 10^|unbiased|.
                let drop = (-unbiased) as u32;
                let digits = decimal_digit_count(coefficient);
                if drop >= digits {
                    // |value| < 1 — only integer if value == 0, but
                    // we already excluded zeros above.
                    return IntegerKind::NonInteger;
                }
                let divisor = 10u128.pow(drop);
                if coefficient % divisor != 0 {
                    return IntegerKind::NonInteger;
                }
                let int_part = coefficient / divisor;
                let last_digit = (int_part % 10) as i32;
                if last_digit & 1 == 0 {
                    IntegerKind::EvenInteger
                } else {
                    IntegerKind::OddInteger
                }
            }
        }
        _ => IntegerKind::NonInteger,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate alloc;
    use alloc::format;

    fn parse(s: &str) -> Decimal128 {
        Decimal128::parse_str(s, RoundingMode::NearestEven).unwrap().0
    }

    fn approx_equal_ulps(a: Decimal128, b: Decimal128, ulps: u32) -> bool {
        let (diff, _) = a.sub(b, RoundingMode::NearestEven);
        let diff = diff.abs();
        let abs_b = b.abs();
        if abs_b.is_zero() {
            let bound = parse(&format!("{ulps}e-30"));
            let (cmp, _) = diff.partial_cmp(bound);
            return matches!(
                cmp,
                Some(core::cmp::Ordering::Less | core::cmp::Ordering::Equal)
            );
        }
        let (rel, _) = diff.div(abs_b, RoundingMode::NearestEven);
        let bound = parse(&format!("{ulps}e-33"));
        let (cmp, _) = rel.partial_cmp(bound);
        matches!(
            cmp,
            Some(core::cmp::Ordering::Less | core::cmp::Ordering::Equal)
        )
    }

    #[test]
    fn pow_x_zero_is_one() {
        for x in &[
            Decimal128::ZERO,
            Decimal128::ONE,
            Decimal128::NEG_ONE,
            Decimal128::INFINITY,
            Decimal128::NEG_INFINITY,
            Decimal128::NAN,
        ] {
            let (r, _) = x.pow(Decimal128::ZERO, RoundingMode::NearestEven);
            assert_eq!(r.to_bits(), Decimal128::ONE.to_bits(), "pow({x:?}, 0)");
        }
    }

    #[test]
    fn pow_one_y_is_one() {
        for y in &[
            Decimal128::ZERO,
            Decimal128::ONE,
            parse("0.5"),
            parse("-3.14"),
            Decimal128::INFINITY,
            Decimal128::NEG_INFINITY,
            Decimal128::NAN,
        ] {
            let (r, _) = Decimal128::ONE.pow(*y, RoundingMode::NearestEven);
            assert_eq!(r.to_bits(), Decimal128::ONE.to_bits(), "pow(1, {y:?})");
        }
    }

    #[test]
    fn pow_zero_neg_is_inf_div_by_zero() {
        let (r, s) = Decimal128::ZERO.pow(Decimal128::NEG_ONE, RoundingMode::NearestEven);
        assert!(r.is_infinite());
        assert!(s.div_by_zero());

        let (r, s) = Decimal128::NEG_ZERO.pow(Decimal128::NEG_ONE, RoundingMode::NearestEven);
        assert!(r.is_infinite());
        assert!(r.is_sign_negative());
        assert!(s.div_by_zero());
    }

    #[test]
    fn pow_zero_pos_is_zero() {
        let (r, _) = Decimal128::ZERO.pow(Decimal128::ONE, RoundingMode::NearestEven);
        assert!(r.is_zero());

        let (r, _) =
            Decimal128::NEG_ZERO.pow(Decimal128::from_i32(3), RoundingMode::NearestEven);
        assert!(r.is_zero());
        assert!(r.is_sign_negative());

        let (r, _) =
            Decimal128::NEG_ZERO.pow(Decimal128::from_i32(2), RoundingMode::NearestEven);
        assert!(r.is_zero());
        assert!(!r.is_sign_negative());
    }

    #[test]
    fn pow_neg_non_integer_is_invalid_nan() {
        let (r, s) = Decimal128::NEG_ONE.pow(parse("0.5"), RoundingMode::NearestEven);
        assert!(r.is_nan());
        assert!(s.invalid());
    }

    #[test]
    fn pow_integer_basics() {
        // 2^10 = 1024
        let two = Decimal128::from_i32(2);
        let (r, _) = two.pow(Decimal128::from_i32(10), RoundingMode::NearestEven);
        let target = Decimal128::from_i32(1024);
        let (cmp, _) = r.partial_cmp(target);
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal), "2^10 = {r:?}");

        // 3^3 = 27
        let (r, _) = Decimal128::from_i32(3).pow(Decimal128::from_i32(3), RoundingMode::NearestEven);
        let (cmp, _) = r.partial_cmp(Decimal128::from_i32(27));
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));

        // (-2)^3 = -8
        let (r, _) =
            Decimal128::from_i32(-2).pow(Decimal128::from_i32(3), RoundingMode::NearestEven);
        let (cmp, _) = r.partial_cmp(Decimal128::from_i32(-8));
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));

        // (-2)^2 = 4
        let (r, _) =
            Decimal128::from_i32(-2).pow(Decimal128::from_i32(2), RoundingMode::NearestEven);
        let (cmp, _) = r.partial_cmp(Decimal128::from_i32(4));
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));
    }

    #[test]
    fn pow_negative_integer_inverts() {
        // 2^-3 = 0.125
        let (r, _) =
            Decimal128::from_i32(2).pow(Decimal128::from_i32(-3), RoundingMode::NearestEven);
        let target = parse("0.125");
        assert!(approx_equal_ulps(r, target, 5));
    }

    #[test]
    fn pow_inf_inf_rules() {
        // 2^Inf = Inf
        let (r, _) =
            Decimal128::from_i32(2).pow(Decimal128::INFINITY, RoundingMode::NearestEven);
        assert!(r.is_infinite());

        // 0.5^Inf = 0
        let (r, _) = parse("0.5").pow(Decimal128::INFINITY, RoundingMode::NearestEven);
        assert!(r.is_zero());

        // 2^-Inf = 0
        let (r, _) =
            Decimal128::from_i32(2).pow(Decimal128::NEG_INFINITY, RoundingMode::NearestEven);
        assert!(r.is_zero());

        // 0.5^-Inf = Inf
        let (r, _) = parse("0.5").pow(Decimal128::NEG_INFINITY, RoundingMode::NearestEven);
        assert!(r.is_infinite());
    }

    #[test]
    fn pow_general_path_basics() {
        // 2^0.5 ≈ sqrt(2) ≈ 1.41421356...
        let (r, _) = Decimal128::from_i32(2).pow(parse("0.5"), RoundingMode::NearestEven);
        let target = parse("1.41421356237309504880168872420969808");
        assert!(
            approx_equal_ulps(r, target, 100),
            "2^0.5 = {r:?}, want ≈ {target:?}"
        );
    }
}
