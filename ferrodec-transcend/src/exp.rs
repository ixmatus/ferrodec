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
//! Correctly rounded across the supported domain `|x| ≤ 14149`
//! (ADR-0032; supersedes ADR-0024's faithful contract). Values past
//! the domain short circuit to ±∞ / ±0 with the appropriate IEEE 754
//! flags.
//!
//! The worst case half ULP margins per format precision are
//! `5.350453e-09` at `Decimal32` (proven across the full canonical
//! Decimal32 input set by the ADR-0033 Plan C4 exhaustive Arb sweep
//! at input `2.408597e-3`;
//! `tests/vectors/transcend/exhaustive/exp.txt`), `5.159e-2` at
//! `Decimal64`, and `3.442e-2` at `Decimal128` (both sampled corpus
//! minima from `tests/vectors/transcend/exp.prov`, ADR-0026 fd-97a).
//! The kernel runs at 50 decimal digits (`EXT_PRECISION` in
//! `extended.rs`), which clears the smallest margin by more than
//! thirty orders of magnitude on every format. The shared error
//! model and the per format headroom derivation live in ADR-0032
//! §Decision; the sampled corpus test (`tests/transcend_vectors.rs`),
//! the ADR-0033 exhaustive worst case kernel verification
//! (`ferrodec-decimal32/tests/transcend_vectors_exhaustive.rs`,
//! 18/18 exact), and the MPFR cross-validation gate
//! (`ferrodec-test-support/tests/mpfr_gate.rs`, 0 disagreements)
//! are the empirical witnesses. `exp` has no TMD hard candidates:
//! `exp(0) = 1` is handled by the zero short circuit and is not in
//! the canonical sweep enumeration.

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
///
/// Correctly rounded across the supported domain (ADR-0032). Derived
/// from `exp` via a single composition step; the bound is the `exp`
/// bound plus one composition rounding. The ADR-0033 Plan C4
/// exhaustive `Decimal32` worst case is `exp2(-11) = 1/2048 =
/// 4.882812e-4` exactly (`tests/vectors/transcend/exhaustive/exp2.txt`);
/// the half ULP margin is exactly 0 because the true value sits
/// exactly at a NearestEven tie. NE ties-to-even resolves decisively
/// (rounds to the even significand `4.882812e-4` over odd
/// `4.882813e-4`), so this is the tightest possible NE constraint
/// for any function in the family rather than TMD hard. The kernel
/// produces the tie value exactly. Sampled corpus minima
/// (`tests/vectors/transcend/exp2.prov`, ADR-0026 fd-97a) are
/// `3.515e-2` at `Decimal64` and `2.015e-2` at `Decimal128`, both
/// cleared by the composed bound by more than thirty orders of
/// magnitude.
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
    // Magnitude gate: `exp` overflows past the format's
    // `exp_overflow_limit` and underflows past its
    // `exp_underflow_limit`. The two thresholds are asymmetric
    // because every IEEE decimal format has a lopsided exponent
    // range (the negative-side round-to-zero boundary sits further
    // out than the positive-side overflow boundary). Inputs between
    // the two limits on the negative side produce subnormals — they
    // must NOT short-circuit to zero, the Taylor pipeline handles
    // them.
    let abs = x_ext.abs();
    let limit = if x_ext.sign {
        F::exp_underflow_limit()
    } else {
        F::exp_overflow_limit()
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
/// thresholds — see [`DecimalFormat::exp_overflow_limit`] /
/// [`DecimalFormat::exp_underflow_limit`] for why.
fn saturate_extreme<F: DecimalFormat>(x: F) -> Option<(F, Status)> {
    let positive = !x.is_sign_negative();
    let abs_ext = Extended::from_format::<F>(x).abs();
    let threshold = if positive {
        F::exp_overflow_limit()
    } else {
        F::exp_underflow_limit()
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
