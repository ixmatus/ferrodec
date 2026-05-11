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
//! 8. Otherwise: positive-base path via `exp(y · ln(x))` evaluated at
//!    `Extended` precision and rounded once at the end. Negative-
//!    integer-y over negative base applies the sign of `(-1)^y`.
//!
//! ## Accuracy
//!
//! Faithfully rounded (≤ 1 ULP at 34 digits) for every finite input
//! via the `Extended` pipeline. Integer exponents up to `±256` first
//! try a square-and-multiply path at `Decimal128` precision; the
//! caller falls through to `Extended` whenever any intermediate
//! multiply rounds (i.e. the path is *only* taken when it produces
//! a bit-exact result). Pre-1.15 the fast path was taken
//! unconditionally for `|y| ≤ 256` and accumulated ~5 ULP for cases
//! like `pow(3, 50)` (H1 of the 2026-05-10 six-agent correctness
//! review).

use crate::bid::{classify_bits, decimal_digit_count, Class, BIAS};
use crate::decimal::Decimal128;
use crate::math::exp::exp_from_extended;
use crate::math::extended::Extended;
use crate::math::ln::ln_extended;
use crate::ops::propagate_nan2;
use crate::status::{RoundingMode, Status};

impl Decimal128 {
    /// `self` raised to the power `exp`.
    #[must_use]
    pub fn pow(self, exp: Self, rm: RoundingMode) -> (Self, Status) {
        pow_kernel(self, exp, rm)
    }

    /// Kani-only entry point that returns the IEEE 754-2019 §9.2.1
    /// special-case branch only (rules 1–7), without invoking the
    /// `Extended`-precision `exp(y · ln(x))` pipeline.
    ///
    /// This exists so symbolic proofs of the pow rule table don't drag
    /// the heavyweight transcendental path through CBMC's path
    /// explosion. Production code uses [`Decimal128::pow`]. Returns
    /// `None` for the general-path inputs (rule 8: positive-base
    /// non-special exponent). `rm` is accepted for convention parity
    /// with the other `*_special_only_for_kani` shims but ignored —
    /// rules 1–7 don't depend on rounding direction.
    #[cfg(kani)]
    #[doc(hidden)]
    #[must_use]
    pub fn pow_special_only_for_kani(self, exp: Self, _rm: RoundingMode) -> Option<(Self, Status)> {
        pow_special_cases(self, exp)
    }
}

/// Apply IEEE 754-2019 §9.2.1 rules 1–7 for `pow(x, y)` without
/// touching the `Extended`-precision general path.
///
/// Returns `Some((result, status))` whenever an IEEE-distinguished
/// rule fires; returns `None` for the rule-8 general path (finite
/// non-zero positive-base or integer-y over negative base).
///
/// Loop-free and self-contained; the Kani special-case harness in
/// `src/verify/pow.rs` proves the rule table by exhausting a small
/// operand pool against this function rather than against the full
/// `pow_kernel`, keeping CBMC inside its time budget.
fn pow_special_cases(x: Decimal128, y: Decimal128) -> Option<(Decimal128, Status)> {
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
        return Some((Decimal128::ONE, status));
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
            return Some((Decimal128::ONE, status));
        }
    }

    // Rules 3: NaN propagation.
    if x.is_signaling_nan() || y.is_signaling_nan() {
        return Some((propagate_nan2(x, y), Status::INVALID));
    }
    if x.is_nan() || y.is_nan() {
        return Some((propagate_nan2(x, y), Status::OK));
    }

    let y_sign_neg = y.is_sign_negative();
    let y_int = integer_test(y);

    // Rule 4: pow(±0, y).
    if x.is_zero() {
        let result_sign = x.is_sign_negative() && matches!(y_int, IntegerKind::OddInteger);
        if y_sign_neg {
            // ±∞ + DIV_BY_ZERO
            return Some((
                if result_sign {
                    Decimal128::NEG_INFINITY
                } else {
                    Decimal128::INFINITY
                },
                Status::DIV_BY_ZERO,
            ));
        }
        // ±0
        return Some((
            if result_sign {
                Decimal128::NEG_ZERO
            } else {
                Decimal128::ZERO
            },
            Status::OK,
        ));
    }

    // Rule 5: pow(±∞, y).
    if x.is_infinite() {
        let result_sign = x.is_sign_negative() && matches!(y_int, IntegerKind::OddInteger);
        if y_sign_neg {
            return Some((
                if result_sign {
                    Decimal128::NEG_ZERO
                } else {
                    Decimal128::ZERO
                },
                Status::OK,
            ));
        }
        return Some((
            if result_sign {
                Decimal128::NEG_INFINITY
            } else {
                Decimal128::INFINITY
            },
            Status::OK,
        ));
    }

    // Rule 6: pow(x, ±∞).
    if y.is_infinite() {
        let abs_x = x.abs();
        let (cmp, _) = abs_x.partial_cmp(Decimal128::ONE);
        return Some(match (cmp, y_sign_neg) {
            (Some(core::cmp::Ordering::Greater), false) => (Decimal128::INFINITY, Status::OK),
            (Some(core::cmp::Ordering::Greater), true) => (Decimal128::ZERO, Status::OK),
            (Some(core::cmp::Ordering::Less), false) => (Decimal128::ZERO, Status::OK),
            (Some(core::cmp::Ordering::Less), true) => (Decimal128::INFINITY, Status::OK),
            // pow(±1, ±∞) = 1 per IEEE 754-2019 §9.2.1. Rule 2 above
            // only short-circuits for x = +1 (so that pow(-1, qNaN)
            // can still propagate NaN), so the negative-base case
            // arrives here.
            (Some(core::cmp::Ordering::Equal), _) => (Decimal128::ONE, Status::OK),
            (None, _) => unreachable!("NaN handled above"),
        });
    }

    // Rule 7: negative finite base with non-integer exponent.
    if x.is_sign_negative() && matches!(y_int, IntegerKind::NonInteger) {
        return Some((Decimal128::NAN, Status::INVALID));
    }

    // Rule 8: general path. Caller (`pow_kernel`) handles the integer
    // fast path and the `exp(y · ln(|x|))` Extended pipeline.
    None
}

fn pow_kernel(x: Decimal128, y: Decimal128, rm: RoundingMode) -> (Decimal128, Status) {
    if let Some(early) = pow_special_cases(x, y) {
        return early;
    }

    // Rule 8: general path. `pow_special_cases` returned None, so x is
    // a finite non-zero (positive-base after rule 7 cleared the
    // negative-non-integer case) and y is a finite non-zero. Try the
    // integer fast path; *only* take its result when it was exact
    // (square-and-multiply at Decimal128 precision accumulates ULP
    // errors otherwise — the H1 finding in the 2026-05-10 review
    // documented ~5 ULP for `pow(3, 50)`). The fast path remains
    // valuable for small integer exponents where the result fits in
    // 34 digits and no multiply rounds.
    let y_int = integer_test(y);
    if let Some((v, status)) = pow_integer_fast_path(x, y, &y_int, rm) {
        if !status.inexact() {
            return (v, status);
        }
        // Fall through: int_pow accumulated rounding error; the
        // Extended pipeline below is more accurate.
    }

    // General path: pow(x, y) = exp(y · ln(|x|)) evaluated entirely at
    // Extended precision. Single round when converting back to
    // `Decimal128`, so the final result is faithfully rounded
    // (≤ 1 ULP) for typical inputs.
    let abs_x = x.abs();
    let ln_x_ext = ln_extended(abs_x);
    let y_ext = Extended::from_decimal128(y);
    let y_ln_x_ext = y_ext.mul(ln_x_ext);
    let (result, mut status) = exp_from_extended(y_ln_x_ext, rm);

    let sign_neg = x.is_sign_negative() && matches!(y_int, IntegerKind::OddInteger);
    let signed = if sign_neg { result.neg() } else { result };

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
        Decimal128::parse_str(s, RoundingMode::NearestEven)
            .unwrap()
            .0
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

        let (r, _) = Decimal128::NEG_ZERO.pow(Decimal128::from_i32(3), RoundingMode::NearestEven);
        assert!(r.is_zero());
        assert!(r.is_sign_negative());

        let (r, _) = Decimal128::NEG_ZERO.pow(Decimal128::from_i32(2), RoundingMode::NearestEven);
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
        let (r, _) =
            Decimal128::from_i32(3).pow(Decimal128::from_i32(3), RoundingMode::NearestEven);
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
        let (r, _) = Decimal128::from_i32(2).pow(Decimal128::INFINITY, RoundingMode::NearestEven);
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
    fn pow_neg_one_to_infinity_is_one() {
        // Per IEEE 754-2019 §9.2.1, pow(±1, ±∞) = 1. The previous
        // implementation panicked at unreachable!() because rule 2's
        // short-circuit only matched x = +1 (deliberately, so that
        // pow(-1, qNaN) can still propagate NaN), and rule 6 then
        // saw |x| = 1 and had no Equal arm.
        for &y in &[Decimal128::INFINITY, Decimal128::NEG_INFINITY] {
            let (r, s) = Decimal128::NEG_ONE.pow(y, RoundingMode::NearestEven);
            assert_eq!(
                r.to_bits(),
                Decimal128::ONE.to_bits(),
                "pow(-1, {y:?}) must be 1"
            );
            assert_eq!(s, Status::OK, "pow(-1, {y:?}) must not raise any flag");
        }
        // Also confirm pow(+1, ±∞) = 1 (this path used to be handled
        // by rule 2 alone; the new rule 6 Equal arm covers it too).
        for &y in &[Decimal128::INFINITY, Decimal128::NEG_INFINITY] {
            let (r, _) = Decimal128::ONE.pow(y, RoundingMode::NearestEven);
            assert_eq!(r.to_bits(), Decimal128::ONE.to_bits());
        }
    }

    #[test]
    fn pow_neg_one_qnan_propagates() {
        // Regression for the rule-2 / rule-3 interplay: extending rule
        // 2 to |x|=1 would be incorrect because pow(-1, qNaN) must
        // propagate NaN per IEEE 754-2019 §9.2.1 (rule 2 explicitly
        // covers only x = +1).
        let (r, s) = Decimal128::NEG_ONE.pow(Decimal128::NAN, RoundingMode::NearestEven);
        assert!(r.is_nan(), "pow(-1, NaN) must be NaN, got {r:?}");
        assert!(!s.invalid(), "pow(-1, qNaN) must not raise INVALID");
        // Sanity: pow(+1, qNaN) still returns 1.
        let (r, _) = Decimal128::ONE.pow(Decimal128::NAN, RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), Decimal128::ONE.to_bits());
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
