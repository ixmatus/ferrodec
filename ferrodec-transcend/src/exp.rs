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
//! the domain saturate through the format rounder, which applies the
//! IEEE 754-2019 §7.4 overflow / underflow disposition per rounding
//! direction (largest finite toward zero on overflow, smallest
//! subnormal toward `+∞` on underflow) with the appropriate flags.
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

use crate::extended::{ExtNum, Extended};
use crate::extended2::Extended2;
use crate::format::DecimalFormat;
use crate::ladder;
use ferrodec_ieee::IeeeDecodedClass as Class;
use ferrodec_ieee::{RoundingMode, Status};

/// Natural exponential.
///
/// ## Exactness and ties (ADR-0059 classification leg)
///
/// `e^r` is transcendental for every algebraic `r ≠ 0` (Lindemann;
/// docs/references/shidlovskii-transcendence.md, with Niven's
/// *Irrational Numbers* as the accessible source —
/// docs/references/niven-irrational-numbers.md). Representable inputs
/// are rational, so beyond the `exp(±0) = 1` short-circuit no input
/// has an exact result and none lands on a nearest-mode tie (ties are
/// rational): the kernel's unconditional `INEXACT` is correct in
/// every mode, and every input sits a finite distance from its
/// rounding boundary (the escalation ladder's standing assumption).
pub fn exp_kernel<F: DecimalFormat>(x: F, rm: RoundingMode) -> (F, Status) {
    ladder::run(
        || exp_kernel_body::<F, Extended>(x, rm),
        || exp_kernel_body::<F, Extended2>(x, rm),
    )
}

/// Generic body of [`exp_kernel`] (M4, ADR-0059); `None` escalates
/// (M8 ladder).
pub(crate) fn exp_kernel_body<F: DecimalFormat, E: ExtNum>(
    x: F,
    rm: RoundingMode,
) -> Option<(F, Status)> {
    match x.classify() {
        Class::SignalingNaN { .. } => return Some((x.nan_from(), Status::INVALID)),
        Class::QuietNaN { .. } => return Some((x, Status::OK)),
        Class::Infinity { sign } => {
            return Some(if sign {
                (F::ZERO, Status::OK)
            } else {
                (F::INFINITY, Status::OK)
            });
        }
        Class::Zero { .. } => return Some((F::ONE, Status::OK)),
        Class::Finite { .. } => {}
    }

    let x_ext = E::from_format(x);
    exp_from_extended_body::<F, E>(x_ext, rm, &ladder::EXP)
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
/// exactly at a `NearestEven` tie. NE ties-to-even resolves decisively
/// (rounds to the even significand `4.882812e-4` over odd
/// `4.882813e-4`), so this is the tightest possible NE constraint
/// for any function in the family rather than TMD hard. Since
/// ADR-0059 M7 the tie is delivered exactly by the input-side
/// classifier (`exact::exp2_exact_or_tie`), not by the approximation
/// kernel, whose error cannot resolve a value that is itself a
/// rounding boundary.
///
/// ## Exactness and ties (ADR-0059 classification leg)
///
/// `2^x` is exact or on a nearest-mode tie only at integer `x` (a
/// rational `2^(a/b)` forces `b = 1` by unique factorization; the
/// full completeness proof lives on `exact::exp2_exact_or_tie`), and
/// the classifier catches every such case, so the kernel's
/// unconditional `INEXACT` is correct on everything it still sees.
///
/// Sampled corpus minima
/// (`tests/vectors/transcend/exp2.prov`, ADR-0026 fd-97a) are
/// `3.515e-2` at `Decimal64` and `2.015e-2` at `Decimal128`, both
/// cleared by the composed bound by more than thirty orders of
/// magnitude.
pub fn exp2_kernel<F: DecimalFormat>(x: F, rm: RoundingMode) -> (F, Status) {
    ladder::run(
        || exp2_kernel_body::<F, Extended>(x, rm),
        || exp2_kernel_body::<F, Extended2>(x, rm),
    )
}

/// Generic body of [`exp2_kernel`] (M4, ADR-0059); `None` escalates
/// (M8 ladder).
pub(crate) fn exp2_kernel_body<F: DecimalFormat, E: ExtNum>(
    x: F,
    rm: RoundingMode,
) -> Option<(F, Status)> {
    match x.classify() {
        Class::SignalingNaN { .. } => return Some((x.nan_from(), Status::INVALID)),
        Class::QuietNaN { .. } => return Some((x, Status::OK)),
        Class::Infinity { sign } => {
            return Some(if sign {
                (F::ZERO, Status::OK)
            } else {
                (F::INFINITY, Status::OK)
            });
        }
        Class::Zero { .. } => return Some((F::ONE, Status::OK)),
        Class::Finite { .. } => {}
    }
    // Exact and tie classification (fd-aqs.8; widened to PRECISION + 1
    // ties by ADR-0059 M7): an integer `n` with `2^n` expressible in
    // at most PRECISION + 1 digits is delivered from the exact
    // coefficient through the format rounder — exact results at every
    // rounding direction with no INEXACT (IEEE 754-2019 §7.5), the
    // directed-mode hazard of the approximation landing on the wrong
    // side of an exact value repaired (`exp2(3)` at `TowardNegative`
    // returned `7.999999…` before fd-aqs.8), and the nearest-mode
    // ties (`exp2(-49)` / `exp2(-50)` at Decimal128) resolved by the
    // rounder's own tie rule, which no approximation kernel can do:
    // the true value IS the boundary.
    if let Some(result) = crate::exact::exp2_exact_or_tie::<F>(x, rm) {
        return Some(result);
    }
    let arg_ext = E::from_format(x).mul(E::ln2());
    exp_from_extended_body::<F, E>(arg_ext, rm, &ladder::EXP2)
}

/// Compute `exp(x_ext)` and round to the format. Used by the public
/// `exp` wrapper and by `pow`'s general `exp(y · ln(x))` path.
///
/// Caller is responsible for filtering NaN / Inf / Zero inputs (those
/// have shortcuts that don't go through Taylor). For finite inputs of
/// any magnitude this routine handles the OVERFLOW / UNDERFLOW
/// thresholds internally.
pub fn exp_from_extended<F: DecimalFormat>(x_ext: Extended, rm: RoundingMode) -> (F, Status) {
    ladder::run(
        || exp_from_extended_body::<F, Extended>(x_ext, rm, &ladder::EXP),
        || {
            exp_from_extended_body::<F, Extended2>(
                Extended2::from_extended(x_ext),
                rm,
                &ladder::EXP,
            )
        },
    )
}

/// Generic body of [`exp_from_extended`] (M4, ADR-0059); `None`
/// escalates (M8 ladder). The budget is the caller's: `exp` and this
/// function's own wrapper pass [`ladder::EXP`], `exp2` passes
/// [`ladder::EXP2`], and the composed kernels (`pow`, `cbrt`) pass
/// their own composition budgets, so the one guarded delivery site
/// serves every pipeline that ends here with the right total.
pub(crate) fn exp_from_extended_body<F: DecimalFormat, E: ExtNum>(
    x_ext: E,
    rm: RoundingMode,
    budget: &ladder::Budget,
) -> Option<(F, Status)> {
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
    let limit = E::from_extended(if x_ext.sign() {
        F::exp_underflow_limit()
    } else {
        F::exp_overflow_limit()
    });
    if abs.cmp(limit) == core::cmp::Ordering::Greater {
        // Saturate through the format rounder rather than returning a
        // hardwired `+∞` / `+0`, so the IEEE 754-2019 §7.4 disposition
        // applies per rounding direction: overflow delivers the largest
        // finite number toward zero and `-∞`, and underflow-to-zero
        // delivers the smallest subnormal toward `+∞`. The gate
        // thresholds guarantee the true result is past the largest
        // finite magnitude (overflow side) or below half the smallest
        // subnormal (underflow side), so every mode's answer is decided
        // by the saturated proxy exactly as by the true value. The
        // `pre_sticky = true` residue marks the proxy inexact; the
        // rounder raises OVERFLOW / UNDERFLOW itself (fd-aqs.5).
        let sat = if x_ext.sign() {
            Extended::saturate_underflow()
        } else {
            Extended::saturate_overflow(false)
        };
        let (result, status) =
            F::round_and_pack_finite(sat.coef, sat.exp, 0, sat.sign, true, rm, Status::OK);
        // Unguarded delivery: the gate thresholds prove the true
        // result past the last boundary with margin, so no rung can
        // change any mode's answer.
        return Some((result, status | Status::INEXACT));
    }

    let result_ext = exp_extended_body(x_ext);
    // Grid-stuck at the 1 anchor (ADR-0051): for `|x|` below the
    // working resolution the series absorbs every term and the
    // result is exactly 1, a format grid point at every precision;
    // the directed modes then need the side, which is the sign of
    // `x` (`e^x > 1` iff `x > 0`). The residual seam carries it.
    // `exp2`, `pow`, and `cbrt` inherit the path through this
    // function. An exactly-zero argument is excluded: there the true
    // result IS 1 (`cbrt(1)` arrives here as `ln(1)/3 = 0`), and the
    // plain path plus the caller's exactness machinery handle it.
    // Unguarded delivery: the anchor leg runs before the ladder's
    // predicate by the ADR-0059 tripod (no finite rung separates a
    // grid-hugging residual; the theorem-backed side does).
    if !x_ext.is_zero() && result_ext.sticks_to(E::ONE) {
        let (result, status) = E::ONE.to_format_with_residual::<F>(!x_ext.sign(), rm);
        return Some((result, status | Status::INEXACT));
    }
    ladder::round_guarded::<F, E>(result_ext, rm, budget)
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
    exp_extended_body(x_ext)
}

/// Generic body of [`exp_extended`] (M4, ADR-0059).
pub(crate) fn exp_extended_body<E: ExtNum>(x_ext: E) -> E {
    // Reduction: x = k · ln(10) + r, with |r| ≤ ln(10)/2.
    let q = x_ext.mul(E::inv_ln10());
    let k = round_to_i32(q);
    let r = x_ext.sub(E::from_i32(k).mul(E::ln10()));

    // Taylor series at working precision.
    let exp_r = taylor_exp_ext(r);

    // exp(x) = exp(r) · 10^k.
    exp_r.mul_pow10_exp(k)
}

/// Round a working-precision value to the nearest `i32`. Used to
/// recover the reduction integer `k` from `q = x / ln(10)`. The
/// truncation itself lives on the [`ExtNum`] seam
/// (`ExtNum::trunc_to_i32`).
fn round_to_i32<E: ExtNum>(q: E) -> i32 {
    if q.is_zero() {
        return 0;
    }
    // Add ±0.5 (depending on sign), then truncate toward zero.
    let nudged = if q.sign() {
        q.sub(E::HALF)
    } else {
        q.add(E::HALF)
    };
    nudged.trunc_to_i32()
}

/// `exp(r) = Σ r^n / n!` evaluated at working precision.
///
/// Convergence: `|r| ≤ ln(10)/2 ≈ 1.151`, and `|r|^n / n!` decays
/// faster than geometrically once `n > |r|`. ~36 terms drives the
/// term magnitude below `10^{-49}`, well past `EXT_PRECISION = 50`;
/// the rung's cap ([`ExtNum::EXP_SERIES_TERMS`]) scales with its
/// digit count.
fn taylor_exp_ext<E: ExtNum>(r: E) -> E {
    let mut sum = E::ONE;
    let mut term = E::ONE;
    // Halt early if `term` falls below the working significance.
    for n in 1u32..=E::EXP_SERIES_TERMS {
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
