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
//! 2. Near-1 direct path (ADR-0050): for `x ∈ (0.5, 1.5)`, feed
//!    `u = x − 1` (exact at Extended width) straight to the log1p
//!    series. This keeps the result's accuracy *relative* to
//!    `ln x` however small that is; the decade route below would
//!    reconstruct a near-zero result from ~2-magnitude addends and
//!    turn the kernel's relative error model absolute (the
//!    2026-06-09 review measured up to ~4e7 ULP just below 1 at
//!    `Decimal128`).
//! 3. Otherwise decompose `x = m · 10^q` with `m ∈ [1, 10)`. Then
//!
//!    ```text
//!    ln(x) = ln(m) + q · ln(10)
//!    ```
//!
//! 4. Reduce `m` further: while `m > 1.5`, divide by 2 and add `ln(2)`
//!    (and below `2/3` for the symmetric branch). After this,
//!    `m ∈ [2/3, 3/2]`, so the Taylor series for
//!    `ln(1 + u)` (`u = m − 1`, `|u| ≤ 1/2`) converges to
//!    `EXT_PRECISION` = 50 digits in well under 200 terms.
//! 5. `ln(1 + u) = u − u²/2 + u³/3 − u⁴/4 + …`. Halt when terms fall
//!    below `EXT_PRECISION` significance.
//!
//! All intermediate work runs at extended precision (`Extended`, see
//! [`Extended`]). The final rounding to the format happens
//! once at the end via `to_format`.
//!
//! ## Accuracy
//!
//! Correctly rounded across the function's domain (ADR-0032;
//! supersedes ADR-0024's faithful contract). The worst case half ULP
//! margins per format precision are `8.119691e-11` at `Decimal32`
//! (proven across the full canonical Decimal32 input set by the
//! ADR-0033 Plan C4 exhaustive Arb sweep at input `6.436357e-29`;
//! `tests/vectors/transcend/exhaustive/ln.txt`), `2.037e-3` at
//! `Decimal64`, and `4.227e-4` at `Decimal128` (both sampled corpus
//! minima from `tests/vectors/transcend/ln.prov`, ADR-0026 fd-97a;
//! `Decimal64` and `Decimal128`'s ~10^18 and ~10^36 canonical input
//! cardinalities are beyond exhaustive reach). The 50 digit kernel
//! clears the smallest margin by more than thirty orders of magnitude
//! on every format. The shared error model lives in ADR-0032
//! §Decision; the inference from margin to every input relies on the
//! error model's *relative* form, which the near-1 direct path
//! restores after the 2026-06-09 review falsified it in the anchor
//! band (ADR-0050; the band corpus
//! `tests/vectors/transcend/anchor_bands/` is the standing witness
//! there). The sampled corpus test, the ADR-0033 exhaustive
//! worst case kernel verification test
//! (`ferrodec-decimal32/tests/transcend_vectors_exhaustive.rs`,
//! 18/18 exact), and the MPFR cross-validation gate
//! (`ferrodec-test-support/tests/mpfr_gate.rs`, 0 disagreements) are
//! the empirical witnesses.
//!
//! ADR-0033 Plan C4 records one TMD hard candidate at input `1`
//! (ln(1) = 0 exactly): the certified Arb ball around the true value
//! 0 has nonzero radius at every Arb precision and straddles the
//! format's underflow boundary, so `_decisive` cannot resolve. The
//! kernel short circuits `ln(1)` to 0 exactly; this is an oracle
//! side limitation, not a kernel defect.
//!
//! `log10` (`log10_kernel`) and `log2` (`log2_kernel`) are derived
//! as `ln(x) · (1 / ln(10))` and `ln(x) · (1 / ln(2))`; their bound
//! is `ln`'s bound plus one composition rounding. The corresponding
//! ADR-0033 Plan C4 exhaustive `Decimal32` worst case margins are
//! `5.258429e-08` at `log10(4.401241)` and `6.316104e-10` at
//! `log2(3.035871e37)`. The sampled corpus minima for `Decimal64`
//! and `Decimal128` are `6.859e-4` / `5.147e-4` (log10) and
//! `2.709e-3` / `8.820e-5` (log2), all far above the kernel error
//! at 50 digit working precision. Both `log10(1) = 0` and
//! `log2(1) = 0` are TMD hard at `CAP_BITS = 65536` for the same
//! reason as `ln(1) = 0`; the kernel short circuits each.
//!
//! ## `logp1`
//!
//! `logp1(x) = ln(1 + x)` (IEEE 754-2019 §9.2; the public wrappers
//! spell it `ln_1p`) reuses this module's series and decade machinery
//! through two bands. Below `0.5` in magnitude the kernel feeds
//! `u = x` straight to the `log1p` series: `from_format` is exact at
//! every rung width, so the series keeps its accuracy *relative* to
//! `ln(1 + x)` however small that value is, the same fd-aqs.6 argument
//! the near-1 path above rests on. At or above `0.5` it forms
//! `t = 1 ⊕ x` and hands `t` to the `ln` core. One seam separates
//! `logp1` from `ln`: `u = x` reaches all the way down to the
//! format's smallest subnormal, and once `|x|` drops below roughly
//! `10^-47` the series sum collapses onto `x` itself, a format grid
//! point no rung can separate from the true value. The ADR-0051
//! anchor seam decides those cases from the side theorem
//! `ln(1 + x) < x` instead, which is what terminates the unbounded
//! rung there. `logp1` runs on the ADR-0059 escalation ladder from
//! its first release; its budget is `LOGP1` in `ladder.rs`.

use crate::extended::{ExtNum, Extended};
use crate::format::DecimalFormat;
use crate::ladder;
use ferrodec_ieee::IeeeDecodedClass as Class;
use ferrodec_ieee::{RoundingMode, Status};

/// Natural logarithm.
///
/// ## Exactness and ties (ADR-0059 classification leg)
///
/// `ln(x) = r` with `r` rational and `x` representable forces
/// `x = e^r`, transcendental for `r ≠ 0` (Lindemann;
/// docs/references/shidlovskii-transcendence.md,
/// docs/references/niven-irrational-numbers.md): only `ln(1) = 0` is
/// exact, and it short-circuits. A nearest-mode tie value is rational,
/// so the same argument rules ties out; the unconditional `INEXACT` is
/// correct in every mode.
pub fn ln_kernel<F: DecimalFormat>(x: F, rm: RoundingMode) -> (F, Status) {
    ladder::ladder_run!(|ex| ln_kernel_body::<F, _>(ex, x, rm))
}

/// Generic body of [`ln_kernel`] (M4, ADR-0059); `None` escalates
/// (M8 ladder). `ex` is the working-precision exemplar (M8b): the
/// receiver the constant and constructor surface reads its width from,
/// never a value the result depends on.
pub(crate) fn ln_kernel_body<F: DecimalFormat, E: ExtNum>(
    ex: E,
    x: F,
    rm: RoundingMode,
) -> Option<(F, Status)> {
    if let Some(early) = ln_special_cases(x) {
        return Some(early);
    }
    if matches!(
        x.partial_cmp_fmt(F::ONE).0,
        Some(core::cmp::Ordering::Equal)
    ) {
        return Some((F::ZERO, Status::OK));
    }
    let result_ext = ln_extended_body::<F, E>(ex, x);
    ladder::round_guarded::<F, E>(result_ext, rm, &ladder::LN)
}

/// Base-10 logarithm.
///
/// ## Exactness and ties (ADR-0059 classification leg)
///
/// `log10(x) = a/b` in lowest terms forces `x^b = 10^a`, and unique
/// factorization then forces `x = 10^k` with `b = 1`: a rational
/// `log10` of a representable `x` is an *integer*, and the exact
/// cases — precisely the powers of ten — are all caught input-side by
/// `exact::log10_exact`. A nearest-mode tie value is rational, hence
/// would be an integer; but a midpoint is never an integer here: a
/// normal-range midpoint's stripped coefficient has `PRECISION + 1`
/// digits ending in 5, needing magnitude ≥ 10^7 while `|log10(x)|`
/// stays below `10^5`, and subnormal-range midpoints are smaller
/// than one. The kernel's unconditional `INEXACT` is therefore
/// correct in every mode.
pub fn log10_kernel<F: DecimalFormat>(x: F, rm: RoundingMode) -> (F, Status) {
    ladder::ladder_run!(|ex| log10_kernel_body::<F, _>(ex, x, rm))
}

/// Generic body of [`log10_kernel`] (M4, ADR-0059); `None` escalates
/// (M8 ladder). `ex` is the working-precision exemplar (M8b).
pub(crate) fn log10_kernel_body<F: DecimalFormat, E: ExtNum>(
    ex: E,
    x: F,
    rm: RoundingMode,
) -> Option<(F, Status)> {
    if let Some(early) = ln_special_cases(x) {
        return Some(early);
    }
    if matches!(
        x.partial_cmp_fmt(F::ONE).0,
        Some(core::cmp::Ordering::Equal)
    ) {
        return Some((F::ZERO, Status::OK));
    }
    // Exact powers of ten (fd-aqs.8): `log10(10^k) = k` exactly, at
    // every rounding direction, with no INEXACT (IEEE 754-2019 §7.5).
    if let Some(exact) = crate::exact::log10_exact::<F>(x, rm) {
        return Some(exact);
    }
    // log10(x) = ln(x) · (1/ln(10)) at working precision.
    let ln_ext = ln_extended_body::<F, E>(ex, x);
    let result_ext = ln_ext.mul(ex.inv_ln10());
    ladder::round_guarded::<F, E>(result_ext, rm, &ladder::LOG10)
}

/// Base-2 logarithm.
///
/// ## Exactness and ties (ADR-0059 classification leg)
///
/// The mirror of [`log10_kernel`]'s argument: `log2(x) = a/b` in
/// lowest terms forces `x^b = 2^a`, so `x = 2^k` and `b = 1` (unique
/// factorization) — a rational `log2` of a representable `x` is an
/// integer, and the exact cases, precisely the powers of two, are all
/// caught input-side by `exact::log2_exact`. A tie value would be a
/// non-exact integer-valued midpoint, which cannot exist
/// (`|log2(x)| < 10^5` while an integer midpoint needs a
/// `PRECISION + 1`-digit coefficient, magnitude ≥ 10^7). The
/// unconditional `INEXACT` is correct in every mode.
pub fn log2_kernel<F: DecimalFormat>(x: F, rm: RoundingMode) -> (F, Status) {
    ladder::ladder_run!(|ex| log2_kernel_body::<F, _>(ex, x, rm))
}

/// Generic body of [`log2_kernel`] (M4, ADR-0059); `None` escalates
/// (M8 ladder). `ex` is the working-precision exemplar (M8b).
pub(crate) fn log2_kernel_body<F: DecimalFormat, E: ExtNum>(
    ex: E,
    x: F,
    rm: RoundingMode,
) -> Option<(F, Status)> {
    if let Some(early) = ln_special_cases(x) {
        return Some(early);
    }
    if matches!(
        x.partial_cmp_fmt(F::ONE).0,
        Some(core::cmp::Ordering::Equal)
    ) {
        return Some((F::ZERO, Status::OK));
    }
    // Exact powers of two (fd-aqs.8): `log2(2^k) = k` exactly, at
    // every rounding direction, with no INEXACT (IEEE 754-2019 §7.5).
    if let Some(exact) = crate::exact::log2_exact::<F>(x, rm) {
        return Some(exact);
    }
    let ln_ext = ln_extended_body::<F, E>(ex, x);
    let result_ext = ln_ext.mul(ex.inv_ln2());
    ladder::round_guarded::<F, E>(result_ext, rm, &ladder::LOG2)
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
    ln_extended_body::<F, Extended>(Extended::ZERO, x)
}

/// Generic body of [`ln_extended`] (M4, ADR-0059); `ex` is the
/// working-precision exemplar (M8b).
pub(crate) fn ln_extended_body<F: DecimalFormat, E: ExtNum>(ex: E, x: F) -> E {
    ln_from_extended_body(ex.from_format(x))
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
    ln_from_extended_body(x_ext)
}

/// Generic body of [`ln_from_extended`] (M4, ADR-0059). `x_ext`
/// doubles as the working-precision exemplar (M8b).
pub(crate) fn ln_from_extended_body<E: ExtNum>(x_ext: E) -> E {
    // Near-1 direct path (fd-aqs.6): for x ∈ (0.5, 1.5) feed
    // u = x − 1 straight to the log1p series. The subtraction is
    // exact at Extended width (leading-digit cancellation only
    // shortens the coefficient), so the result's accuracy is
    // *relative* to `ln x` however small that is. The decade
    // route below reconstructs a near-zero result as
    // `taylor + k·ln 2 − ln 10`, a cancellation of ~2-magnitude
    // addends each carrying ~1e-49 absolute rounding error, which
    // turned the kernel's relative error model absolute just below
    // 1 and mis-rounded `ln`/`log10`/`log2`/`pow` there (the
    // 2026-06-09 review; the band corpus pins the class). Above
    // 1 the old route happened to stay relative because `u = m − 1`
    // was exact with `q = 0`; routing both sides here makes the
    // near-1 neighbourhood symmetric.
    let u = x_ext.sub(x_ext.one());
    if u.abs().cmp(x_ext.half()) == core::cmp::Ordering::Less {
        return taylor_log1p_ext(u);
    }
    let (m_ext, q) = decompose_extended_to_decade(x_ext);

    // Reduce m into [2/3, 3/2] by halving/doubling.
    let mut m = m_ext;
    let mut additional = x_ext.zero();
    let ln2_v = x_ext.ln2();
    let upper = x_ext.parse_str("1.5");
    let lower = x_ext.parse_str("0.6666666666666666666666666666666666666666666666666667");

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
            m = m.mul(x_ext.from_i32(2));
            additional = additional.sub(ln2_v);
            continue;
        }
        break;
    }

    // u = m − 1, |u| ≤ 0.5.
    let u = m.sub(x_ext.one());
    let ln_m = taylor_log1p_ext(u);

    // ln(original_m) = ln_m + accumulated halve/double corrections.
    let ln_orig_m = ln_m.add(additional);

    // Combine: ln(x) = ln(m) + q · ln(10).
    if q == 0 {
        return ln_orig_m;
    }
    let q_ln10 = x_ext.from_i32(q).mul(x_ext.ln10());
    ln_orig_m.add(q_ln10)
}

/// `x_ext = m_ext × 10^q` with `m_ext ∈ [1, 10)`. Caller guarantees
/// `x_ext > 0` and finite (zero would have no defined decade).
fn decompose_extended_to_decade<E: ExtNum>(x_ext: E) -> (E, i32) {
    debug_assert!(!x_ext.is_zero());
    debug_assert!(!x_ext.sign());
    let digits = x_ext.digit_count() as i32;
    let q = x_ext.exponent() + digits - 1;
    let m_ext = x_ext.with_exponent(-(digits - 1));
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

/// Generic body of [`log1p_extended`] (M4, ADR-0059).
pub(crate) fn log1p_extended_body<E: ExtNum>(u: E) -> E {
    taylor_log1p_ext(u)
}

/// Short-circuit the special values and domain errors shared by the
/// `logp1` family (`logp1` / `log2p1` / `log10p1`, IEEE 754-2019
/// §9.2 and §9.2.1): NaN propagation (sNaN raises `INVALID`),
/// `f(+∞) = +∞`, `f(±0) = ±0` (sign preserved, no exception),
/// `f(−1) = −∞` with `DIV_BY_ZERO`, and `f(x < −1)` (including
/// `−∞`) NaN with `INVALID`. `None` means finite, nonzero, strictly
/// above `−1`: the kernels' domain.
pub(crate) fn logp1_special_cases<F: DecimalFormat>(x: F) -> Option<(F, Status)> {
    match x.classify() {
        Class::SignalingNaN { .. } => Some((x.nan_from(), Status::INVALID)),
        Class::QuietNaN { .. } => Some((x, Status::OK)),
        Class::Infinity { sign } => Some(if sign {
            (F::NAN, Status::INVALID)
        } else {
            (F::INFINITY, Status::OK)
        }),
        Class::Zero { .. } => Some((x, Status::OK)),
        Class::Finite { sign, .. } if sign => match x.partial_cmp_fmt(F::NEG_ONE).0 {
            Some(core::cmp::Ordering::Equal) => Some((F::NEG_INFINITY, Status::DIV_BY_ZERO)),
            Some(core::cmp::Ordering::Less) => Some((F::NAN, Status::INVALID)),
            _ => None,
        },
        Class::Finite { .. } => None,
    }
}

/// `ln(1 + x)` at working precision for a finite nonzero `x > −1`
/// (the `logp1` family core; the caller has already run
/// [`logp1_special_cases`]). Two bands:
///
/// * `|x| < 0.5`: `u = x` feeds the `log1p` series directly.
///   `from_format` is exact at every rung width, so the series'
///   relative accuracy argument (fd-aqs.6, the ADR-0050 lesson)
///   holds with `u` exact all the way down to the anchor band.
/// * `|x| ≥ 0.5`: `t = 1 ⊕ x`, then the `ln` core. On the negative
///   side (`x ∈ (−1, −0.5]`) the sum is exact: `1 − |x|` spans at
///   most `F::PRECISION + 1 ≤ 35` aligned digits, inside every
///   rung's width, so `t > 0` holds exactly and no cancellation
///   error enters. On the positive side the sum is exact until `x`
///   outgrows the rung width (`|x| ≳ 10^49` at rung 1), where
///   absorbing the 1 costs at most one working rounding, priced in
///   the family budgets.
pub(crate) fn logp1_extended_core<F: DecimalFormat, E: ExtNum>(ex: E, x: F) -> E {
    let x_ext = ex.from_format(x);
    if x_ext.abs().cmp(ex.half()) == core::cmp::Ordering::Less {
        return log1p_extended_body(x_ext);
    }
    let t = ex.one().add(x_ext);
    debug_assert!(!t.is_zero(), "1 + x is exact here and x > -1");
    ln_from_extended_body(t)
}

/// `logp1(x) = ln(1 + x)` (IEEE 754-2019 §9.2 `logp1`). The public
/// wrappers are `Decimal128::ln_1p` and the `Decimal64` /
/// `Decimal32` siblings.
///
/// ## Exactness and ties (ADR-0059 classification leg)
///
/// `ln(1 + x) = r` with `r` rational and `x` representable makes
/// `1 + x` rational too, so `1 + x = e^r`; for rational `r ≠ 0` that
/// value is transcendental (Lindemann;
/// docs/references/shidlovskii-transcendence.md,
/// docs/references/niven-irrational-numbers.md), a contradiction.
/// Only `r = 0` survives, i.e. `x = 0`: the sole exact case is
/// `logp1(±0) = ±0`, which `logp1_special_cases` delivers sign
/// preserved and exception free. A nearest mode tie value is
/// rational, so the same argument rules every tie out. The
/// unconditional `INEXACT` past the special cases is therefore
/// correct in every mode, and every input the kernel rounds sits a
/// finite distance from its rounding boundary (the ladder's standing
/// assumption).
///
/// ## Accuracy
///
/// Correctly rounded on the ADR-0059 escalation ladder: rung 1
/// evaluates at 50 digits and delivers only when the `ladder::LOGP1`
/// budget clears every rounding boundary, otherwise the identical body
/// re-runs at rung 2 (and, under the `unbounded-ladder` feature, at a
/// dynamic rung that widens until the boundary is decided). The
/// budget's itemization lives on `ladder::LOGP1`.
pub fn logp1_kernel<F: DecimalFormat>(x: F, rm: RoundingMode) -> (F, Status) {
    ladder::ladder_run!(|ex| logp1_kernel_body::<F, _>(ex, x, rm))
}

/// Generic body of [`logp1_kernel`] (M4, ADR-0059); `None` escalates
/// (M8 ladder). `ex` is the working-precision exemplar (M8b).
pub(crate) fn logp1_kernel_body<F: DecimalFormat, E: ExtNum>(
    ex: E,
    x: F,
    rm: RoundingMode,
) -> Option<(F, Status)> {
    if let Some(early) = logp1_special_cases(x) {
        return Some(early);
    }
    let x_ext = ex.from_format(x);
    let result_ext = logp1_extended_core::<F, E>(ex, x);
    // Grid-stuck at the input (ADR-0051): `ln(1+x) < x` strictly for
    // every in-domain nonzero x, so the residual side is below x:
    // toward zero for positive x, away from zero for negative x.
    // Unguarded: the anchor leg runs before the ladder's predicate;
    // for |x| ≲ 1e-47 the series collapses to exactly x (a grid
    // point no rung separates) and this seam is what terminates the
    // unbounded rung there.
    if result_ext.sticks_to(x_ext) {
        let (result, status) = x_ext.to_format_with_residual::<F>(x_ext.sign(), rm);
        return Some((result, status | Status::INEXACT));
    }
    ladder::round_guarded::<F, E>(result_ext, rm, &ladder::LOGP1)
}

/// Taylor series `ln(1 + u) = u − u²/2 + u³/3 − u⁴/4 + …` at
/// working precision. Halts when adding the next term doesn't change
/// the partial sum at that precision.
fn taylor_log1p_ext<E: ExtNum>(u: E) -> E {
    let mut sum = u.zero();
    let mut power = u.one(); // u^0; updated to u^n inside the loop
    let mut sign_alt = false;

    // |u| ≤ 0.5 → |u^n / n| ≤ 0.5^n / n. To drive the term below
    // 10^{-50} we need n large enough that 0.5^n < 10^{-50} · n,
    // i.e. n ≳ 50 · log2(10) / 1 ≈ 166. The rung 1 cap of 250 carries
    // that safety margin; each rung's cap scales with its digit count.
    for n in 1u32..=u.log1p_series_terms() {
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
