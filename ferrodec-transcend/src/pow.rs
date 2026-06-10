//! Moved from `ferrodec/src/math/pow.rs` @ commit 82a7fe1 (P0a.2 c11).
//! Behaviour-neutral: genericized over [`DecimalFormat`]; the
//! `Decimal128` instantiation is byte-identical to the pre-move
//! kernel. ADR-0016: [`pow_special_cases`] stays loop-free; the kani
//! shim routes only through it, never through [`pow_kernel`].
//!
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
//! Correctly rounded for every finite input via the `Extended`
//! pipeline (ADR-0032; supersedes ADR-0024's faithful contract).
//! Integer exponents up to `±256` first try a square and multiply
//! path at `Decimal128` precision; the caller falls through to
//! `Extended` whenever any intermediate multiply rounds (i.e. the
//! path is *only* taken when it produces a bit exact result).
//! Pre-1.15 the fast path was taken unconditionally for `|y| ≤ 256`
//! and accumulated ~5 ULP for cases like `pow(3, 50)` (H1 of the
//! 2026-05-10 six agent correctness review).
//!
//! `pow(x, y) = exp(y · ln |x|)` couples the `exp` and `ln` bounds.
//! The Arb empirical worst case half ULP margin from
//! `tests/vectors/transcend/pow.prov` (ADR-0026, fd-97a; binary
//! search over `(x, y)`) is `1.406e-2` at `Decimal32` precision,
//! `6.849e-3` at `Decimal64` precision, and `1.020e-2` at
//! `Decimal128` precision. The kernel's working precision exceeds
//! the cumulative `exp` + `ln` + composition error budget by more
//! than thirty orders of magnitude on every format, so the
//! composed bound holds.
//!
//! `pow` is binary and was excluded from the ADR-0033 Plan C4
//! unary exhaustive sweep per the ADR-0033 §Rejected alternatives:
//! the canonical Decimal32 input pair cardinality is roughly 10^16,
//! beyond exhaustive reach at per-candidate Arb cost. `pow` at
//! `Decimal32` therefore continues to cite the sampled corpus
//! minimum as the binding empirical margin under the ADR-0033
//! Slice A corpus integrity discipline (cap hits asserted zero per
//! regeneration).
//!
//! The shared error model lives in ADR-0032 §Decision; the corpus
//! test (`tests/transcend_vectors.rs`) is the standing empirical
//! witness.

use crate::exp::exp_from_extended;
use crate::extended::Extended;
use crate::format::DecimalFormat;
use crate::ln::ln_extended;
use ferrodec_ieee::{decimal_digit_count_u128 as decimal_digit_count, IeeeDecodedClass as Class};
use ferrodec_ieee::{RoundingMode, Status};

/// Apply IEEE 754-2019 §9.2.1 rules 1–7 for `pow(x, y)` without
/// touching the `Extended`-precision general path.
///
/// Returns `Some((result, status))` whenever an IEEE-distinguished
/// rule fires; returns `None` for the rule-8 general path (finite
/// non-zero positive-base or integer-y over negative base).
///
/// Loop-free and self-contained; the Kani special-case harness in
/// `ferrodec/src/verify/pow.rs` proves the rule table by exhausting a
/// small operand pool against this function rather than against the
/// full [`pow_kernel`], keeping CBMC inside its time budget. ADR-0016
/// requires this routine stay loop-free: its only `F`-touch points
/// (`F::classify`, `partial_cmp_fmt`, `propagate_nan2`, the named
/// constants) are all on the explicit loop-free list.
pub fn pow_special_cases<F: DecimalFormat>(x: F, y: F) -> Option<(F, Status)> {
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
        return Some((F::ONE, status));
    }

    // Rule 2: pow(1, y) = 1, regardless of y.
    if !x.is_nan() {
        let (cmp, _) = x.partial_cmp_fmt(F::ONE);
        if matches!(cmp, Some(core::cmp::Ordering::Equal)) {
            let status = if y.is_signaling_nan() {
                Status::INVALID
            } else {
                Status::OK
            };
            return Some((F::ONE, status));
        }
    }

    // Rules 3: NaN propagation.
    if x.is_signaling_nan() || y.is_signaling_nan() {
        return Some((x.propagate_nan2(y), Status::INVALID));
    }
    if x.is_nan() || y.is_nan() {
        return Some((x.propagate_nan2(y), Status::OK));
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
                    F::NEG_INFINITY
                } else {
                    F::INFINITY
                },
                Status::DIV_BY_ZERO,
            ));
        }
        // ±0
        return Some((if result_sign { F::NEG_ZERO } else { F::ZERO }, Status::OK));
    }

    // Rule 5: pow(±∞, y).
    if x.is_infinite() {
        let result_sign = x.is_sign_negative() && matches!(y_int, IntegerKind::OddInteger);
        if y_sign_neg {
            return Some((if result_sign { F::NEG_ZERO } else { F::ZERO }, Status::OK));
        }
        return Some((
            if result_sign {
                F::NEG_INFINITY
            } else {
                F::INFINITY
            },
            Status::OK,
        ));
    }

    // Rule 6: pow(x, ±∞).
    if y.is_infinite() {
        let abs_x = x.abs();
        let (cmp, _) = abs_x.partial_cmp_fmt(F::ONE);
        return Some(match (cmp, y_sign_neg) {
            (Some(core::cmp::Ordering::Greater), false) => (F::INFINITY, Status::OK),
            (Some(core::cmp::Ordering::Greater), true) => (F::ZERO, Status::OK),
            (Some(core::cmp::Ordering::Less), false) => (F::ZERO, Status::OK),
            (Some(core::cmp::Ordering::Less), true) => (F::INFINITY, Status::OK),
            // pow(±1, ±∞) = 1 per IEEE 754-2019 §9.2.1. Rule 2 above
            // only short-circuits for x = +1 (so that pow(-1, qNaN)
            // can still propagate NaN), so the negative-base case
            // arrives here.
            (Some(core::cmp::Ordering::Equal), _) => (F::ONE, Status::OK),
            (None, _) => unreachable!("NaN handled above"),
        });
    }

    // Rule 7: negative finite base with non-integer exponent.
    if x.is_sign_negative() && matches!(y_int, IntegerKind::NonInteger) {
        return Some((F::NAN, Status::INVALID));
    }

    // Rule 8: general path. Caller (`pow_kernel`) handles the integer
    // fast path and the `exp(y · ln(|x|))` Extended pipeline.
    None
}

pub fn pow_kernel<F: DecimalFormat>(x: F, y: F, rm: RoundingMode) -> (F, Status) {
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
    // Extended precision. Single round when converting back to the
    // format, so the final result is correctly rounded per ADR-0032
    // (the Extended pipeline's cumulative error is bounded well inside
    // the half-ULP grid at every format precision; see the module
    // Accuracy section and ADR-0032 §Decision).
    let abs_x = x.abs();
    let ln_x_ext = ln_extended(abs_x);
    let y_ext = Extended::from_format(y);
    let y_ln_x_ext = y_ext.mul(ln_x_ext);

    // The pipeline evaluates |x|^y and re-applies the sign for an odd
    // integer y over a negative base. Round the magnitude under the
    // negation-reflected mode so the directed modes land on the
    // correct neighbour after the sign flip (the cbrt `for_negation`
    // rule; fd-aqs.5).
    let sign_neg = x.is_sign_negative() && matches!(y_int, IntegerKind::OddInteger);
    let eff_rm = if sign_neg { rm.for_negation() } else { rm };
    let (result, status) = exp_from_extended::<F>(y_ln_x_ext, eff_rm);
    let signed = if sign_neg { result.neg() } else { result };

    // `exp_from_extended` already raised INEXACT. pow can land on an exact
    // value (an exact integer or rational power: pow(10, 300) = 1E+300,
    // pow(4, 0.5) = 2), where IEEE 754-2019 §7.5 forbids the flag. Suppress
    // it only when the delivered result raised back through the exponent
    // reproduces the input exactly. Overflow / ±∞ results never enter the
    // check (decoding a non-finite datum is undefined). Small exact integer
    // powers are already handled by the fast path above and never reach
    // here.
    let final_status = if !status.overflow()
        && !signed.is_infinite()
        && crate::exact::power_is_exact(signed, x, y)
    {
        crate::exact::clear_inexact(status)
    } else {
        status
    };
    (signed, final_status)
}

/// Try the square-and-multiply fast path for integer `y` up to `±256`.
/// Beyond that the cumulative rounding error in repeated multiplication
/// can exceed the ulp envelope, and we fall through to the general
/// `exp(y·ln(x))` path.
fn pow_integer_fast_path<F: DecimalFormat>(
    x: F,
    y: F,
    y_int: &IntegerKind,
    rm: RoundingMode,
) -> Option<(F, Status)> {
    if matches!(y_int, IntegerKind::NonInteger) {
        return None;
    }
    let (n_i32, st) = y.to_i32_fmt(RoundingMode::NearestEven);
    if !st.is_ok() {
        return None;
    }
    if !(-256..=256).contains(&n_i32) {
        return None;
    }
    Some(int_pow(x, n_i32, rm))
}

fn int_pow<F: DecimalFormat>(x: F, n: i32, rm: RoundingMode) -> (F, Status) {
    if n == 0 {
        return (F::ONE, Status::OK);
    }
    let mut status = Status::OK;
    let invert = n < 0;
    let mut exp = n.unsigned_abs();
    let mut base = x;
    let mut result = F::ONE;

    while exp > 0 {
        if exp & 1 == 1 {
            let (r, s) = result.mul_fmt(base, rm);
            result = r;
            status |= s;
        }
        exp >>= 1;
        if exp > 0 {
            let (b, s) = base.mul_fmt(base, rm);
            base = b;
            status |= s;
        }
    }

    if invert {
        let (r, s) = F::ONE.div_fmt(result, rm);
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

fn integer_test<F: DecimalFormat>(y: F) -> IntegerKind {
    if y.is_nan() || y.is_infinite() {
        return IntegerKind::NonInteger;
    }
    match y.classify() {
        Class::Zero { .. } => IntegerKind::EvenInteger,
        Class::Finite {
            sign: _,
            biased_exp,
            coefficient,
        } => {
            let unbiased = biased_exp as i32 - F::BIAS;
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
