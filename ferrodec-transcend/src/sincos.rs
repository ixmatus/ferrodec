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
//! (ADR-0032; supersedes ADR-0024's faithful contract). The worst
//! case half ULP margins per format precision are:
//!
//! - `sin`: `2.811904e-10` at `Decimal32` (ADR-0033 Plan C4
//!   exhaustive sweep at input `6.109088e40`, deep in the
//!   Payne Hanek argument reduction regime;
//!   `tests/vectors/transcend/exhaustive/sin.txt`), `1.609e-4` at
//!   `Decimal64`, `5.056e-4` at `Decimal128`.
//! - `cos`: `7.699426e-10` at `Decimal32` (Plan C4 exhaustive at
//!   `5.734251e52`), `7.996e-4` at `Decimal64`, `4.051e-4` at
//!   `Decimal128`.
//! - `tan`: `5.107326e-10` at `Decimal32` (Plan C4 exhaustive at
//!   `8.40978e64`, the campaign's deepest Payne Hanek argument
//!   reduction input), `3.177e-4` at `Decimal64`, `2.272e-3` at
//!   `Decimal128`.
//!
//! The `Decimal32` figures are proven correctly rounded across the
//! full canonical input set by Arb; the `Decimal64` and `Decimal128`
//! figures are sampled corpus minima from
//! `tests/vectors/transcend/{sin,cos,tan}.prov` (ADR-0026 fd-97a)
//! under the ADR-0033 Slice A corpus integrity discipline (cap hits
//! asserted zero, trig scan extended to full per format `emax`).
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
//! None of `sin`, `cos`, `tan` has a TMD hard candidate in the
//! Plan C4 enumeration: `sin(0) = 0` and `cos(0) = 1` are not in
//! the canonical sweep (the enumeration skips coef = 0), and the
//! function values at nonzero canonical inputs are transcendental,
//! so the certified Arb ball never straddles the underflow boundary
//! in the way `ln(1) = 0` does.
//!
//! The shared error model lives in ADR-0032 §Decision; the sampled
//! corpus test, the ADR-0033 exhaustive worst case kernel
//! verification gate
//! (`ferrodec-decimal32/tests/transcend_vectors_exhaustive.rs`,
//! 18/18 exact), and the MPFR cross-validation gate
//! (`ferrodec-test-support/tests/mpfr_gate.rs`, 0 disagreements)
//! are the empirical witnesses.

use crate::extended::{ExtNum, Extended};
use crate::extended2::Extended2;
use crate::format::DecimalFormat;
use crate::ladder;
use ferrodec_ieee::IeeeDecodedClass as Class;
use ferrodec_ieee::{RoundingMode, Status};

/// Sine, in radians.
///
/// ## Exactness and ties (ADR-0059 classification leg)
///
/// `sin(r)` is transcendental for every algebraic `r ≠ 0`
/// (Lindemann–Weierstrass; docs/references/shidlovskii-transcendence.md,
/// with Niven's *Irrational Numbers* as the accessible source —
/// docs/references/niven-irrational-numbers.md). Representable inputs
/// are rational, so beyond the `sin(±0) = ±0` short-circuit no input
/// has an exact result and none lands on a nearest-mode tie (ties are
/// rational): the kernel's unconditional `INEXACT` is correct in every
/// mode, and every input sits a finite distance from its rounding
/// boundary (the escalation ladder's standing assumption).
pub fn sin_kernel<F: DecimalFormat>(x: F, rm: RoundingMode) -> (F, Status) {
    ladder::run(
        || sin_kernel_body::<F, Extended>(Extended::ZERO, x, rm),
        || sin_kernel_body::<F, Extended2>(Extended2::ZERO, x, rm),
    )
}

/// Generic body of [`sin_kernel`] (M4, ADR-0059); `None` escalates
/// (M8 ladder). `ex` is the working-precision exemplar (M8b): the
/// receiver the constant and constructor surface reads its width from,
/// never a value the result depends on.
pub(crate) fn sin_kernel_body<F: DecimalFormat, E: ExtNum>(
    ex: E,
    x: F,
    rm: RoundingMode,
) -> Option<(F, Status)> {
    match x.classify() {
        Class::SignalingNaN { .. } => Some((x.nan_from(), Status::INVALID)),
        Class::QuietNaN { .. } => Some((x, Status::OK)),
        Class::Infinity { .. } => Some((F::NAN, Status::INVALID)),
        Class::Zero { sign, .. } => Some((if sign { F::NEG_ZERO } else { F::ZERO }, Status::OK)),
        Class::Finite { .. } => sincos_kernel::<F, E>(ex, x, rm).0,
    }
}

/// Cosine, in radians.
///
/// ## Exactness and ties (ADR-0059 classification leg)
///
/// `cos(r)` is transcendental for every algebraic `r ≠ 0`
/// (Lindemann–Weierstrass; docs/references/shidlovskii-transcendence.md,
/// docs/references/niven-irrational-numbers.md), so beyond
/// `cos(±0) = 1` no representable input has an exact result or a
/// nearest-mode tie; the unconditional `INEXACT` is correct in every
/// mode.
pub fn cos_kernel<F: DecimalFormat>(x: F, rm: RoundingMode) -> (F, Status) {
    ladder::run(
        || cos_kernel_body::<F, Extended>(Extended::ZERO, x, rm),
        || cos_kernel_body::<F, Extended2>(Extended2::ZERO, x, rm),
    )
}

/// Generic body of [`cos_kernel`] (M4, ADR-0059); `None` escalates
/// (M8 ladder). `ex` is the working-precision exemplar (M8b).
pub(crate) fn cos_kernel_body<F: DecimalFormat, E: ExtNum>(
    ex: E,
    x: F,
    rm: RoundingMode,
) -> Option<(F, Status)> {
    match x.classify() {
        Class::SignalingNaN { .. } => Some((x.nan_from(), Status::INVALID)),
        Class::QuietNaN { .. } => Some((x, Status::OK)),
        Class::Infinity { .. } => Some((F::NAN, Status::INVALID)),
        Class::Zero { .. } => Some((F::ONE, Status::OK)),
        Class::Finite { .. } => sincos_kernel::<F, E>(ex, x, rm).1,
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
///
/// ## Exactness and ties (ADR-0059 classification leg)
///
/// `tan(r)` is transcendental for every algebraic `r ≠ 0` (a corollary
/// of Lindemann–Weierstrass: a rational `tan(r)` would make `e^{2ir}`
/// algebraic; docs/references/shidlovskii-transcendence.md,
/// docs/references/niven-irrational-numbers.md), so beyond
/// `tan(±0) = ±0` no representable input has an exact result or a
/// nearest-mode tie; the unconditional `INEXACT` is correct in every
/// mode. The `cos(x) = 0` asymptote is never hit exactly for the same
/// reason (odd multiples of π/2 are irrational), so the `±∞` branch is
/// working-precision saturation, not an exact-case claim.
pub fn tan_kernel<F: DecimalFormat>(x: F, rm: RoundingMode) -> (F, Status) {
    ladder::run(
        || tan_kernel_body::<F, Extended>(Extended::ZERO, x, rm),
        || tan_kernel_body::<F, Extended2>(Extended2::ZERO, x, rm),
    )
}

/// Generic body of [`tan_kernel`] (M4, ADR-0059); `None` escalates
/// (M8 ladder). `ex` is the working-precision exemplar (M8b).
pub(crate) fn tan_kernel_body<F: DecimalFormat, E: ExtNum>(
    ex: E,
    x: F,
    rm: RoundingMode,
) -> Option<(F, Status)> {
    match x.classify() {
        Class::SignalingNaN { .. } => return Some((x.nan_from(), Status::INVALID)),
        Class::QuietNaN { .. } => return Some((x, Status::OK)),
        Class::Infinity { .. } => return Some((F::NAN, Status::INVALID)),
        Class::Zero { sign, .. } => {
            return Some((if sign { F::NEG_ZERO } else { F::ZERO }, Status::OK));
        }
        Class::Finite { .. } => {}
    }
    let (sin_ext, cos_ext, status_red) = sincos_extended_body::<F, E>(ex, x);
    if cos_ext.is_zero() {
        // sin/cos at the asymptote: return ±∞ with the sign of sin.
        // Unguarded: the asymptote is never hit by a true value (odd
        // multiples of π/2 are irrational), so this is a working-
        // precision saturation whose verdict rung 2 shares.
        let sign = sin_ext.sign();
        return Some((
            if sign { F::NEG_INFINITY } else { F::INFINITY },
            status_red | Status::INEXACT,
        ));
    }
    let tan_ext = sin_ext.div::<F>(cos_ext);
    // Grid-stuck at the input (ADR-0051): a small argument absorbs
    // every correction and the quotient is exactly `x`; the directed
    // modes need the side, and `|tan x| > |x|` is a theorem.
    // Unguarded: the anchor leg runs before the ladder's predicate.
    let x_anchor = ex.from_format(x);
    if tan_ext.sticks_to(x_anchor) {
        let (tan_d, st) = x_anchor.to_format_with_residual::<F>(true, rm);
        return Some((tan_d, st | status_red | Status::INEXACT));
    }
    let (tan_d, st) = ladder::round_guarded::<F, E>(tan_ext, rm, &ladder::TAN)?;
    Some((tan_d, st | status_red))
}

/// Compute both `(sin(x), status)` and `(cos(x), status)` from one
/// reduction. Returns them as `((sin, sin_status), (cos, cos_status))`;
/// a `None` component escalates that component's caller to rung 2 (M8
/// ladder) while the anchor deliveries stay unconditional.
#[allow(clippy::type_complexity)]
fn sincos_kernel<F: DecimalFormat, E: ExtNum>(
    ex: E,
    x: F,
    rm: RoundingMode,
) -> (Option<(F, Status)>, Option<(F, Status)>) {
    let (sin_x_ext, cos_x_ext, status_red) = sincos_extended_body::<F, E>(ex, x);
    // Grid-stuck anchors (ADR-0051). `sin`: a small argument absorbs
    // every correction and the result is exactly `x` (`|sin x| < |x|`
    // is a theorem), and an argument within ~10^-25 of `±π/2 + 2πk`
    // (e.g. the 34-digit truncation of π/2 itself) collapses the
    // result to exactly `±1` (`|sin x| < 1` strictly at every finite
    // decimal). `cos`: the result collapses to exactly `1` for small
    // arguments and near even multiples of π, and to exactly `-1`
    // near the odd multiples (`|cos x| < 1` strictly likewise). The
    // magnitude shrinks in every case, so the residual side is a
    // theorem, not a measurement.
    let x_anchor = ex.from_format(x);
    let sin_one = ex.one().with_sign(sin_x_ext.sign());
    let sin = if sin_x_ext.sticks_to(x_anchor) {
        Some(x_anchor.to_format_with_residual::<F>(false, rm))
    } else if sin_x_ext.sticks_to(sin_one) {
        Some(sin_one.to_format_with_residual::<F>(false, rm))
    } else {
        ladder::round_guarded::<F, E>(sin_x_ext, rm, &ladder::SIN)
    };
    let cos_one = ex.one().with_sign(cos_x_ext.sign());
    let cos = if cos_x_ext.sticks_to(cos_one) {
        Some(cos_one.to_format_with_residual::<F>(false, rm))
    } else {
        ladder::round_guarded::<F, E>(cos_x_ext, rm, &ladder::COS)
    };
    let status = status_red | Status::INEXACT;
    (
        sin.map(|(d, st)| (d, st | status)),
        cos.map(|(d, st)| (d, st | status)),
    )
}

/// Compute `(sin(x), cos(x))` at `Extended` precision. Used directly
/// by the public `sin` / `cos` (after rounding) and by `tan(x) =
/// sin(x) / cos(x)` (which divides the two extended values before
/// rounding). Caller filters NaN / Inf / Zero.
pub fn sincos_extended<F: DecimalFormat>(x: F) -> (Extended, Extended, Status) {
    sincos_extended_body::<F, Extended>(Extended::ZERO, x)
}

/// Generic body of [`sincos_extended`] (M4, ADR-0059); `ex` is the
/// working-precision exemplar (M8b).
pub(crate) fn sincos_extended_body<F: DecimalFormat, E: ExtNum>(ex: E, x: F) -> (E, E, Status) {
    let neg = match x.classify() {
        Class::Finite { sign, .. } => sign,
        _ => false,
    };
    let abs_x = if neg { x.neg() } else { x };

    // Per-rung reduction dispatch (M8): rung 1 reads the 76-digit
    // window and 38-digit π/2, rung 2 `reduce_wide`'s 143-digit
    // window and 115-digit π/2 — re-running the narrow reduction at
    // wide arithmetic would inherit the truncation escalation exists
    // to outrun.
    let (k_mod_4, r, status_red) = ex.reduce_trig::<F>(abs_x);
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
/// working precision; caller passes `r²` so it can be shared with
/// the cosine evaluation.
fn taylor_sin_ext<E: ExtNum>(r: E, r_sq: E) -> E {
    let mut sum = r;
    let mut term = r;
    let mut alt = true; // next term subtracts.
                        // n indexes the term series (term_n = r^{2n-1} / (2n-1)!).
                        // Update: term_{n+1} = term_n · r² / ((2n)(2n+1)).
    let mut n: u32 = 1;
    for _ in 0..r.sin_cos_series_terms() {
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
fn taylor_cos_ext<E: ExtNum>(r_sq: E) -> E {
    let mut sum = r_sq.one();
    let mut term = r_sq.one();
    let mut alt = true; // next term subtracts.
    let mut n: u32 = 0;
    for _ in 0..r_sq.sin_cos_series_terms() {
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
