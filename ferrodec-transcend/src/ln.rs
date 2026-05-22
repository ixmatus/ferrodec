//! Moved from `ferrodec/src/math/ln.rs` @ commit 82a7fe1 (P0a.2 c5).
//! Behaviour-neutral: genericized over [`DecimalFormat`]; the
//! `Decimal128` instantiation is byte-identical to the pre-move kernel.
//!
//! `ln(x)` — natural logarithm, plus `log10(x)` derived as `ln(x) · (1/ln(10))`.
//!
//! ## Algorithm
//!
//! 1. Special cases:
//!    * NaN propagates; sNaN raises `INVALID`.
//!    * `ln(0) = −∞ + DIV_BY_ZERO` per IEEE 754 §9.3.
//!    * `ln(negative_finite) = NaN + INVALID`.
//!    * `ln(+∞) = +∞`. `ln(−∞) = NaN + INVALID`.
//!    * `ln(1) = +0`.
//! 2. Decompose `x = m · 10^q` with `m ∈ [1, 10)`. Then
//!
//!    ```text
//!    ln(x) = ln(m) + q · ln(10)
//!    ```
//!
//! 3. Reduce `m` further: while `m > 1.5`, divide by 2 and add `ln(2)`
//!    (and below `2/3` for the symmetric branch). After this,
//!    `m ∈ [2/3, 3/2]`, so the Taylor series for
//!    `ln(1 + u)` (`u = m − 1`, `|u| ≤ 1/2`) converges to
//!    `EXT_PRECISION` = 50 digits in well under 200 terms.
//! 4. `ln(1 + u) = u − u²/2 + u³/3 − u⁴/4 + …`. Halt when terms fall
//!    below `EXT_PRECISION` significance.
//!
//! All intermediate work runs at extended precision (`Extended`, see
//! [`Extended`]). The final rounding to the format happens
//! once at the end via `to_format`.
//!
//! ## Accuracy
//!
//! Correctly rounded across the function's domain (ADR-0032;
//! supersedes ADR-0024's faithful contract). The Arb empirical worst
//! case half ULP margin from `tests/vectors/transcend/ln.prov`
//! (ADR-0026, fd-97a) is `3.333e-7` at `Decimal32` precision,
//! `2.037e-3` at `Decimal64` precision, and `4.227e-4` at
//! `Decimal128` precision. The 50 digit kernel clears the smallest
//! margin by more than thirty orders of magnitude on every format.
//! The shared error model lives in ADR-0032 §Decision; the corpus
//! test is the standing empirical witness.
//!
//! `log10` (`log10_kernel`) and `log2` (`log2_kernel`) are derived
//! as `ln(x) · (1 / ln(10))` and `ln(x) · (1 / ln(2))`; their bound
//! is `ln`'s bound plus one composition rounding. The corresponding
//! `log10.prov` and `log2.prov` margins are `7.250e-4` / `5.147e-4`
//! and `7.212e-4` / `8.820e-5` (smallest across the three
//! precisions), both far above the kernel error at 50 digit working
//! precision.

use crate::consts::{inv_ln10_ext, inv_ln2_ext, ln10_ext, ln2_ext};
use crate::extended::Extended;
use crate::format::DecimalFormat;
use ferrodec_ieee::IeeeDecodedClass as Class;
use ferrodec_ieee::{RoundingMode, Status};

pub fn ln_kernel<F: DecimalFormat>(x: F, rm: RoundingMode) -> (F, Status) {
    if let Some(early) = ln_special_cases(x) {
        return early;
    }
    if matches!(
        x.partial_cmp_fmt(F::ONE).0,
        Some(core::cmp::Ordering::Equal)
    ) {
        return (F::ZERO, Status::OK);
    }
    let result_ext = ln_extended(x);
    let (result, status) = result_ext.to_format::<F>(0, rm);
    (result, status | Status::INEXACT)
}

pub fn log10_kernel<F: DecimalFormat>(x: F, rm: RoundingMode) -> (F, Status) {
    if let Some(early) = ln_special_cases(x) {
        return early;
    }
    if matches!(
        x.partial_cmp_fmt(F::ONE).0,
        Some(core::cmp::Ordering::Equal)
    ) {
        return (F::ZERO, Status::OK);
    }
    // log10(x) = ln(x) · (1/ln(10)) at extended precision.
    let ln_ext = ln_extended(x);
    let result_ext = ln_ext.mul(inv_ln10_ext());
    let (result, status) = result_ext.to_format::<F>(0, rm);
    (result, status | Status::INEXACT)
}

pub fn log2_kernel<F: DecimalFormat>(x: F, rm: RoundingMode) -> (F, Status) {
    if let Some(early) = ln_special_cases(x) {
        return early;
    }
    if matches!(
        x.partial_cmp_fmt(F::ONE).0,
        Some(core::cmp::Ordering::Equal)
    ) {
        return (F::ZERO, Status::OK);
    }
    let ln_ext = ln_extended(x);
    let result_ext = ln_ext.mul(inv_ln2_ext());
    let (result, status) = result_ext.to_format::<F>(0, rm);
    (result, status | Status::INEXACT)
}

/// Short-circuit the special cases shared by `ln` and `log10`.
pub fn ln_special_cases<F: DecimalFormat>(x: F) -> Option<(F, Status)> {
    match x.classify() {
        Class::SignalingNaN { .. } => Some((x.nan_from(), Status::INVALID)),
        Class::QuietNaN { .. } => Some((x, Status::OK)),
        Class::Infinity { sign } => Some(if sign {
            (F::NAN, Status::INVALID)
        } else {
            (F::INFINITY, Status::OK)
        }),
        Class::Zero { .. } => Some((F::NEG_INFINITY, Status::DIV_BY_ZERO)),
        Class::Finite { sign, .. } if sign => Some((F::NAN, Status::INVALID)),
        Class::Finite { .. } => None,
    }
}

/// Compute `ln(x)` at extended precision. Caller has already filtered
/// NaN / Inf / zero / negative inputs and the `x == 1` edge case.
pub fn ln_extended<F: DecimalFormat>(x: F) -> Extended {
    ln_from_extended(Extended::from_format(x))
}

/// Compute `ln(x_ext)` at extended precision, given an extended-
/// precision argument. Used by inverse hyperbolics (`asinh` / `acosh` /
/// `atanh`), which build the argument `x + sqrt(x² ± 1)` (or the
/// `(1+x)/(1−x)` ratio) at extended precision and would otherwise lose
/// precision rounding to the format between operations.
///
/// Caller guarantees `x_ext > 0` and finite. Sign and zero are *not*
/// handled here — they are domain errors at the public-API boundary.
pub fn ln_from_extended(x_ext: Extended) -> Extended {
    let (m_ext, q) = decompose_extended_to_decade(x_ext);

    // Reduce m into [2/3, 3/2] by halving/doubling.
    let mut m = m_ext;
    let mut additional = Extended::ZERO;
    let ln2_v = ln2_ext();
    let upper = Extended::parse_str("1.5");
    let lower = Extended::parse_str("0.6666666666666666666666666666666666666666666666666667");

    // At most ~5 iterations to reach the target window (each halve/double
    // contracts by 2× and m starts in [1, 10)).
    let mut guard = 0u32;
    while guard < 20 {
        guard += 1;
        if m.cmp(upper) == core::cmp::Ordering::Greater {
            m = m.div_u32(2);
            additional = additional.add(ln2_v);
            continue;
        }
        if m.cmp(lower) == core::cmp::Ordering::Less {
            m = m.mul(Extended::from_i32(2));
            additional = additional.sub(ln2_v);
            continue;
        }
        break;
    }

    // u = m − 1, |u| ≤ 0.5.
    let u = m.sub(Extended::ONE);
    let ln_m = taylor_log1p_ext(u);

    // ln(original_m) = ln_m + accumulated halve/double corrections.
    let ln_orig_m = ln_m.add(additional);

    // Combine: ln(x) = ln(m) + q · ln(10).
    if q == 0 {
        return ln_orig_m;
    }
    let q_ln10 = Extended::from_i32(q).mul(ln10_ext());
    ln_orig_m.add(q_ln10)
}

/// `x_ext = m_ext × 10^q` with `m_ext ∈ [1, 10)`. Caller guarantees
/// `x_ext > 0` and finite (zero would have no defined decade).
fn decompose_extended_to_decade(x_ext: Extended) -> (Extended, i32) {
    debug_assert!(!x_ext.is_zero());
    debug_assert!(!x_ext.sign);
    let digits = x_ext.coef.decimal_digit_count() as i32;
    let q = x_ext.exp + digits - 1;
    let m_ext = Extended {
        coef: x_ext.coef,
        exp: -(digits - 1),
        sign: false,
    };
    (m_ext, q)
}

/// `ln(1 + u)` at extended precision via Taylor series.
///
/// Used by `ln_extended`'s halve-double loop and exposed for callers
/// (notably `acosh` near `x = 1`) that compute `ln(1 + small)` and
/// would otherwise lose precision routing through `ln_from_extended`.
///
/// Caller guarantees `|u|` is comfortably below the radius of
/// convergence (`u < 1`); the 250-iteration cap inside reliably
/// handles `|u| ≤ ~0.6` to 50-digit precision.
pub fn log1p_extended(u: Extended) -> Extended {
    taylor_log1p_ext(u)
}

/// Taylor series `ln(1 + u) = u − u²/2 + u³/3 − u⁴/4 + …` at
/// extended precision. Halts when adding the next term doesn't change
/// the partial sum at 50-digit precision.
fn taylor_log1p_ext(u: Extended) -> Extended {
    let mut sum = Extended::ZERO;
    let mut power = Extended::ONE; // u^0; updated to u^n inside the loop
    let mut sign_alt = false;

    // |u| ≤ 0.5 → |u^n / n| ≤ 0.5^n / n. To drive the term below
    // 10^{-50} we need n large enough that 0.5^n < 10^{-50} · n,
    // i.e. n ≳ 50 · log2(10) / 1 ≈ 166. Cap at 250 for safety.
    for n in 1u32..=250 {
        let new_power = power.mul(u);
        power = new_power;
        let term = power.div_u32(n);
        let signed = if sign_alt { term.neg() } else { term };
        let next_sum = sum.add(signed);
        sign_alt = !sign_alt;
        if next_sum.cmp(sum) == core::cmp::Ordering::Equal {
            sum = next_sum;
            break;
        }
        sum = next_sum;
        if power.is_zero() {
            break;
        }
    }
    sum
}
