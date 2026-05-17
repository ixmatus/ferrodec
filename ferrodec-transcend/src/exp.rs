//! Moved from `ferrodec/src/math/exp.rs` @ commit 82a7fe1 (P0a.2 c6).
//! Behaviour-neutral: genericized over [`DecimalFormat`]; the
//! `Decimal128` instantiation is byte-identical to the pre-move kernel.
//!
//! `exp(x)` — natural exponential.
//!
//! ## Algorithm
//!
//! 1. Special cases: NaN / sNaN / ±∞ / ±0.
//! 2. Range reduction. Split `x = k · ln(10) + r` with `|r| ≤ ln(10)/2`,
//!    so `r` lives in roughly `[-1.151, 1.151]`. Then
//!
//!    ```text
//!    exp(x) = 10^k · exp(r)
//!    ```
//!
//!    where `10^k` is a quantum shift on the [`Extended`] (and the final
//!    format datum).
//! 3. Compute `exp(r)` via Taylor series at extended precision
//!    ([`Extended`]). 50-digit working
//!    precision keeps the cumulative series error below the
//!    34-digit envelope.
//! 4. Round to the format once at the end via
//!    `round_and_pack_finite`, threading through OVERFLOW / UNDERFLOW.
//!
//! ## Accuracy
//!
//! Faithfully rounded (≤ 1 ULP at 34 digits) against `astro-float`
//! across the supported domain `|x| ≤ 14149`. Values past the domain
//! short-circuit to ±∞ / ±0 with the appropriate IEEE 754 flags.

use crate::consts::{inv_ln10_ext, ln10_ext, ln2_ext};
use crate::extended::Extended;
use crate::format::DecimalFormat;
use ferrodec_ieee::IeeeDecodedClass as Class;
use ferrodec_ieee::{RoundingMode, Status};

pub fn exp_kernel<F: DecimalFormat>(x: F, rm: RoundingMode) -> (F, Status) {
    match x.classify() {
        Class::SignalingNaN { .. } => return (x.nan_from(), Status::INVALID),
        Class::QuietNaN { .. } => return (x, Status::OK),
        Class::Infinity { sign } => {
            return if sign {
                (F::ZERO, Status::OK)
            } else {
                (F::INFINITY, Status::OK)
            };
        }
        Class::Zero { .. } => return (F::ONE, Status::OK),
        Class::Finite { .. } => {}
    }

    if let Some(early) = saturate_extreme(x) {
        return early;
    }

    let x_ext = Extended::from_format(x);
    exp_from_extended(x_ext, rm)
}

/// Base-2 exponential `2^x`. Computed as `exp(x · ln(2))` at extended
/// precision.
pub fn exp2_kernel<F: DecimalFormat>(x: F, rm: RoundingMode) -> (F, Status) {
    match x.classify() {
        Class::SignalingNaN { .. } => return (x.nan_from(), Status::INVALID),
        Class::QuietNaN { .. } => return (x, Status::OK),
        Class::Infinity { sign } => {
            return if sign {
                (F::ZERO, Status::OK)
            } else {
                (F::INFINITY, Status::OK)
            };
        }
        Class::Zero { .. } => return (F::ONE, Status::OK),
        Class::Finite { .. } => {}
    }
    let arg_ext = Extended::from_format(x).mul(ln2_ext());
    exp_from_extended(arg_ext, rm)
}

/// Compute `exp(x_ext)` and round to the format. Used by the public
/// `exp` wrapper and by `pow`'s general `exp(y · ln(x))` path.
///
/// Caller is responsible for filtering NaN / Inf / Zero inputs (those
/// have shortcuts that don't go through Taylor). For finite inputs of
/// any magnitude this routine handles the OVERFLOW / UNDERFLOW
/// thresholds internally.
pub fn exp_from_extended<F: DecimalFormat>(x_ext: Extended, rm: RoundingMode) -> (F, Status) {
    // Magnitude gate: `exp` overflows past `+ln(MAX) ≈ +14149.4` and
    // underflows past `−ln(1/MIN_SUBNORMAL) ≈ −14223`. The
    // thresholds are asymmetric because Decimal128's exponent range
    // is lopsided (E_MAX = 6144, MIN_SUBNORMAL exponent = −6176).
    // Inputs in `(−14223, −14150]` produce subnormals — must NOT
    // short-circuit to zero, the Taylor pipeline handles them.
    let abs = x_ext.abs();
    let limit = if x_ext.sign {
        Extended::EXP_UNDERFLOW_LIMIT
    } else {
        Extended::EXP_OVERFLOW_LIMIT
    };
    if abs.cmp(limit) == core::cmp::Ordering::Greater {
        return if x_ext.sign {
            (F::ZERO, Status::UNDERFLOW | Status::INEXACT)
        } else {
            (F::INFINITY, Status::OVERFLOW | Status::INEXACT)
        };
    }

    let result_ext = exp_extended(x_ext);
    let (result, status) = result_ext.to_format::<F>(0, rm);
    (result, status | Status::INEXACT)
}

/// Compute `exp(x_ext)` and return the result *at extended precision*.
/// Distinct from [`exp_from_extended`] in that no rounding to the
/// format happens — the caller composes further at extended
/// precision and rounds once at the boundary.
///
/// Used by `sinh` / `cosh` to compute `(e^x ± e^{-x}) / 2` without
/// the precision-loss of an intermediate format round-trip.
///
/// Caller must guarantee `|x_ext|` is within the convergence window
/// (`|x| ≤ ~14150`); larger inputs land in [`exp_from_extended`]'s
/// saturation branch and are not handled here. The returned [`Extended`]
/// can have an exponent outside the format's representable range —
/// the boundary rounder handles that as OVERFLOW.
pub fn exp_extended(x_ext: Extended) -> Extended {
    // Reduction: x = k · ln(10) + r, with |r| ≤ ln(10)/2.
    let q = x_ext.mul(inv_ln10_ext());
    let k = round_to_i32(q);
    let r = x_ext.sub(Extended::from_i32(k).mul(ln10_ext()));

    // Taylor series at extended precision.
    let exp_r = taylor_exp_ext(r);

    // exp(x) = exp(r) · 10^k.
    exp_r.mul_pow10_exp(k)
}

/// Round an [`Extended`] to the nearest `i32`. Used to recover the
/// reduction integer `k` from `q = x / ln(10)`.
fn round_to_i32(q: Extended) -> i32 {
    if q.is_zero() {
        return 0;
    }
    // Add ±0.5 (depending on sign), then truncate toward zero.
    let nudged = if q.sign {
        q.sub(Extended::HALF)
    } else {
        q.add(Extended::HALF)
    };
    truncate_to_i32(nudged)
}

/// Truncate an [`Extended`] toward zero into an `i32`. Caller guarantees
/// the magnitude is well within `i32::MAX`.
fn truncate_to_i32(v: Extended) -> i32 {
    if v.is_zero() {
        return 0;
    }
    // Shift coef by exp to recover the integer value.
    if v.exp >= 0 {
        // coef · 10^exp — but for our `k` reduction, exp should
        // always be ≤ 0 (since |x| ≤ 14149 → |q| ≤ 6145 < 10^4 and
        // the .mul produced ~50-digit coef with exp ≈ -50).
        // Defensively widen: scale up.
        let mut c = v.coef;
        for _ in 0..(v.exp as u32) {
            c = c.mul10();
        }
        let val = c.lo as i64;
        return if v.sign { -(val as i32) } else { val as i32 };
    }
    // exp < 0: shift right.
    let mut c = v.coef;
    for _ in 0..((-v.exp) as u32) {
        let (q, _) = c.div_rem10();
        c = q;
    }
    let val = c.lo as i64;
    if v.sign {
        -(val as i32)
    } else {
        val as i32
    }
}

/// `exp(r) = Σ r^n / n!` evaluated at [`Extended`] precision.
///
/// Convergence: `|r| ≤ ln(10)/2 ≈ 1.151`, and `|r|^n / n!` decays
/// faster than geometrically once `n > |r|`. ~36 terms drives the
/// term magnitude below `10^{-49}`, well past `EXT_PRECISION = 50`.
fn taylor_exp_ext(r: Extended) -> Extended {
    let mut sum = Extended::ONE;
    let mut term = Extended::ONE;
    // Halt early if `term` falls below ~10^{-55} (well below
    // EXT_PRECISION's significance).
    for n in 1u32..=60 {
        term = term.mul(r).div_u32(n);
        let next_sum = sum.add(term);
        // Early exit: if `next_sum` matches `sum` at extended
        // precision, further terms will round to zero contribution.
        if next_sum.cmp(sum) == core::cmp::Ordering::Equal {
            sum = next_sum;
            break;
        }
        sum = next_sum;
        if term.is_zero() {
            break;
        }
    }
    sum
}

/// Coarse extreme-magnitude detection. Returns `Some((±∞ or ±0, status))`
/// when the input is way outside the convergence window. Asymmetric
/// thresholds — see [`Extended::EXP_OVERFLOW_LIMIT`] /
/// [`Extended::EXP_UNDERFLOW_LIMIT`] for why.
fn saturate_extreme<F: DecimalFormat>(x: F) -> Option<(F, Status)> {
    let positive = !x.is_sign_negative();
    let abs_ext = Extended::from_format::<F>(x).abs();
    let threshold = if positive {
        Extended::parse_str("14150")
    } else {
        Extended::parse_str("14221")
    };
    if abs_ext.cmp(threshold) != core::cmp::Ordering::Greater {
        return None;
    }
    if positive {
        Some((F::INFINITY, Status::OVERFLOW | Status::INEXACT))
    } else {
        Some((F::ZERO, Status::UNDERFLOW | Status::INEXACT))
    }
}
