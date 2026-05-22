//! Moved from `ferrodec/src/math/sincos.rs` @ commit 82a7fe1 (P0a.2 c8).
//! Behaviour-neutral: genericized over [`DecimalFormat`]; the
//! `Decimal128` instantiation is byte-identical to the pre-move kernel.
//!
//! `sin(x)` and `cos(x)`.
//!
//! ## Algorithm
//!
//! 1. Special cases: NaN / sNaN propagate; `sin(±0) = ±0`;
//!    `cos(±0) = +1`; `sin(±∞) = cos(±∞) = NaN + INVALID`.
//! 2. Range reduction via Payne-Hanek (see [`crate::argred`]):
//!    compute `k = round(|x| · 2/π) mod 4` and the residual `r` such
//!    that `|x| = k · π/2 + r` and `|r| ≤ π/4`. The reduction works
//!    across the full `Decimal128` magnitude range — there's no
//!    `|x| ≤ 10^9` cap. `r` is returned as an [`Extended`] so the
//!    Taylor body below sees ~38-40 digits of fractional residual,
//!    not just 34.
//! 3. Taylor series for `sin(r)` and `cos(r)` on `|r| ≤ π/4`,
//!    evaluated at `Extended` (50-digit) precision. Then rotate by
//!    `k mod 4`:
//!
//!    ```text
//!    k mod 4   sin(|x|)   cos(|x|)
//!    -------  --------   --------
//!         0    sin(r)     cos(r)
//!         1    cos(r)    -sin(r)
//!         2   -sin(r)    -cos(r)
//!         3   -cos(r)     sin(r)
//!    ```
//!
//!    `sin` is odd, so `sin(x) = -sin(|x|)` when `x < 0`. `cos` is
//!    even, so `cos(x) = cos(|x|)` regardless of sign.
//! 4. Round once to the format at the end via
//!    [`Extended::to_format`].
//!
//! ## Accuracy
//!
//! Correctly rounded across the full `Decimal128` magnitude range
//! (ADR-0032; supersedes ADR-0024's faithful contract). The Arb
//! empirical worst case half ULP margins from the per function
//! provenance files (ADR-0026, fd-97a) are:
//!
//! - `sin.prov`: `1.134e-4` at `Decimal32`, `1.609e-4` at
//!   `Decimal64`, `5.056e-4` at `Decimal128`.
//! - `cos.prov`: `2.054e-3` at `Decimal32`, `7.996e-4` at
//!   `Decimal64`, `4.051e-4` at `Decimal128`.
//! - `tan.prov`: `8.147e-4` at `Decimal32`, `3.177e-4` at
//!   `Decimal64`, `2.272e-3` at `Decimal128`.
//!
//! At 50 digit kernel working precision the cumulative error
//! (Payne Hanek reduction error plus Taylor series error plus
//! format boundary round) clears the smallest of these margins by
//! more than thirty orders of magnitude on every format.
//!
//! `tan(x) = sin(x) / cos(x)` discharges the per decade Payne Hanek
//! bound across the full argument range without an ε band carve out
//! near odd multiples of π / 2. The 6300 digit `2/π` table in
//! [`crate::argred`] sizes the reduction so the residual `r`
//! carries 38 to 40 fractional digits, which exceeds every format
//! precision (7, 16, 34) by enough margin that `cos(r)` retains
//! full relative precision even when `|cos(r)|` is small. At odd
//! multiples of π / 2 the kernel returns `±∞` per IEEE 754 §9.2.1's
//! asymptote convention (no `DIV_BY_ZERO`); this is itself the
//! correctly rounded value.
//!
//! The shared error model lives in ADR-0032 §Decision; the corpus
//! test is the standing empirical witness.

use crate::argred;
use crate::extended::Extended;
use crate::format::DecimalFormat;
use ferrodec_ieee::IeeeDecodedClass as Class;
use ferrodec_ieee::{RoundingMode, Status};

/// Sine, in radians.
pub fn sin_kernel<F: DecimalFormat>(x: F, rm: RoundingMode) -> (F, Status) {
    match x.classify() {
        Class::SignalingNaN { .. } => (x.nan_from(), Status::INVALID),
        Class::QuietNaN { .. } => (x, Status::OK),
        Class::Infinity { .. } => (F::NAN, Status::INVALID),
        Class::Zero { sign, .. } => (if sign { F::NEG_ZERO } else { F::ZERO }, Status::OK),
        Class::Finite { .. } => sincos_kernel(x, rm).0,
    }
}

/// Cosine, in radians.
pub fn cos_kernel<F: DecimalFormat>(x: F, rm: RoundingMode) -> (F, Status) {
    match x.classify() {
        Class::SignalingNaN { .. } => (x.nan_from(), Status::INVALID),
        Class::QuietNaN { .. } => (x, Status::OK),
        Class::Infinity { .. } => (F::NAN, Status::INVALID),
        Class::Zero { .. } => (F::ONE, Status::OK),
        Class::Finite { .. } => sincos_kernel(x, rm).1,
    }
}

/// Tangent, in radians.
///
/// `tan(x) = sin(x) / cos(x)`, computed by dividing the two
/// extended-precision sin/cos values before rounding to
/// the format. At `cos(x) = 0` (odd multiples of π/2) the
/// result diverges; we return `±∞` without raising
/// `DIV_BY_ZERO` (since `tan` of a finite input doesn't fit the
/// IEEE 754 §7.3 division-by-zero condition — it's just an
/// asymptote).
pub fn tan_kernel<F: DecimalFormat>(x: F, rm: RoundingMode) -> (F, Status) {
    match x.classify() {
        Class::SignalingNaN { .. } => return (x.nan_from(), Status::INVALID),
        Class::QuietNaN { .. } => return (x, Status::OK),
        Class::Infinity { .. } => return (F::NAN, Status::INVALID),
        Class::Zero { sign, .. } => {
            return (if sign { F::NEG_ZERO } else { F::ZERO }, Status::OK);
        }
        Class::Finite { .. } => {}
    }
    let (sin_ext, cos_ext, status_red) = sincos_extended(x);
    if cos_ext.is_zero() {
        // sin/cos at the asymptote: return ±∞ with the sign of sin.
        let sign = sin_ext.sign;
        return (
            if sign { F::NEG_INFINITY } else { F::INFINITY },
            status_red | Status::INEXACT,
        );
    }
    let tan_ext = sin_ext.div::<F>(cos_ext);
    let (tan_d, st) = tan_ext.to_format::<F>(0, rm);
    (tan_d, st | status_red | Status::INEXACT)
}

/// Compute both `(sin(x), status)` and `(cos(x), status)` from one
/// reduction. Returns them as `((sin, sin_status), (cos, cos_status))`.
fn sincos_kernel<F: DecimalFormat>(x: F, rm: RoundingMode) -> ((F, Status), (F, Status)) {
    let (sin_x_ext, cos_x_ext, status_red) = sincos_extended(x);
    let (sin_d, sin_status) = sin_x_ext.to_format::<F>(0, rm);
    let (cos_d, cos_status) = cos_x_ext.to_format::<F>(0, rm);
    let status = status_red | Status::INEXACT;
    ((sin_d, sin_status | status), (cos_d, cos_status | status))
}

/// Compute `(sin(x), cos(x))` at `Extended` precision. Used directly
/// by the public `sin` / `cos` (after rounding) and by `tan(x) =
/// sin(x) / cos(x)` (which divides the two extended values before
/// rounding). Caller filters NaN / Inf / Zero.
pub fn sincos_extended<F: DecimalFormat>(x: F) -> (Extended, Extended, Status) {
    let neg = match x.classify() {
        Class::Finite { sign, .. } => sign,
        _ => false,
    };
    let abs_x = if neg { x.neg() } else { x };

    let (k_mod_4, r, status_red) = argred::reduce(abs_x);
    let r_sq = r.square();
    let sin_r = taylor_sin_ext(r, r_sq);
    let cos_r = taylor_cos_ext(r_sq);

    let (sin_abs_ext, cos_abs_ext) = match k_mod_4 {
        0 => (sin_r, cos_r),
        1 => (cos_r, sin_r.neg()),
        2 => (sin_r.neg(), cos_r.neg()),
        3 => (cos_r.neg(), sin_r),
        _ => unreachable!(),
    };

    let sin_x_ext = if neg { sin_abs_ext.neg() } else { sin_abs_ext };
    (sin_x_ext, cos_abs_ext, status_red)
}

/// `sin(r) = r − r³/3! + r⁵/5! − …` for `|r| ≤ π/4`. Evaluated at
/// `Extended` precision; caller passes `r²` so it can be shared with
/// the cosine evaluation.
fn taylor_sin_ext(r: Extended, r_sq: Extended) -> Extended {
    let mut sum = r;
    let mut term = r;
    let mut alt = true; // next term subtracts.
                        // n indexes the term series (term_n = r^{2n-1} / (2n-1)!).
                        // Update: term_{n+1} = term_n · r² / ((2n)(2n+1)).
    let mut n: u32 = 1;
    for _ in 0..120 {
        n += 1;
        let denom = (2 * n - 2) * (2 * n - 1); // u32, fits up to n ≈ 32k
        term = term.mul(r_sq).div_u32(denom);
        let signed = if alt { term.neg() } else { term };
        alt = !alt;
        let next_sum = sum.add(signed);
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

/// `cos(r) = 1 − r²/2! + r⁴/4! − …` for `|r| ≤ π/4`.
fn taylor_cos_ext(r_sq: Extended) -> Extended {
    let mut sum = Extended::ONE;
    let mut term = Extended::ONE;
    let mut alt = true; // next term subtracts.
    let mut n: u32 = 0;
    for _ in 0..120 {
        n += 1;
        let denom = (2 * n - 1) * (2 * n);
        term = term.mul(r_sq).div_u32(denom);
        let signed = if alt { term.neg() } else { term };
        alt = !alt;
        let next_sum = sum.add(signed);
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
