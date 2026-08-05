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
//!
//! ## The `expm1` family (ADR-0059 Track D)
//!
//! The IEEE 754-2019 §9.2 `expm1` family (`expm1` / `exp2m1` /
//! `exp10m1`, public `exp_m1` / `exp2_m1` / `exp10_m1`) shares this
//! module's reduction, series, and decade machinery through
//! `expm1_ext`, and its §9.2.1 dispositions through
//! `expm1_special_cases` (`f(+∞) = +∞`, `f(−∞) = −1` exactly with
//! no exception, `f(±0) = ±0` sign preserved). Each member runs on
//! the ADR-0059 escalation ladder from its first release, with its
//! own budget in `ladder.rs`.
//!
//! Two gates precede the core (`expm1_gates`, on the base scaled
//! working argument `u`). Past the format's `exp_overflow_limit` the
//! true value is whole decades beyond the last finite boundary, so
//! the saturation proxy feeds the format rounder directly exactly as
//! `exp`'s own gate does. Below `u = −120` the true value sits
//! strictly inside `(−1, −1 + 10^−52)`, closer to `−1` than any
//! format's first boundary toward zero, so every mode's answer is the
//! `−1` anchor's; that gate also keeps the reduction's
//! `trunc_to_i32` away from arguments whose reduction integer would
//! not fit.
//!
//! The core splits at `|u| ≤ 1.1513`, the reduction's own `k = 0`
//! window. Inside it the direct `expm1` series keeps the result's
//! accuracy relative to `e^u − 1` however small that is, which the
//! `exp` pipeline followed by a subtraction cannot do; outside it the
//! `exp` pipeline runs and the closing subtraction of 1 amplifies by
//! `e^u/(e^u − 1) ≤ 1.47` at the band edge.
//!
//! Both grid hugging bands are decided by ADR-0051 anchor seams
//! rather than by a wider rung. The `−1` anchor catches the working
//! collapse just above the deep negative gate (the subtraction rounds
//! to 1 at working width), on the side theorem `e^u − 1 > −1`.
//! `expm1` carries a second seam at its argument: `e^x − 1 > x`
//! strictly, so once `|x|` drops below roughly `10^−47` and the
//! series collapses onto `x` itself the seam supplies the side, the
//! mirror of `logp1`'s seam in `ln.rs` with the direction reversed.
//! The base variants take the other legs instead: their slope at 0
//! is `ln 2` or `ln 10`, so their tiny results land off grid.

use crate::extended::{ExtNum, Extended};
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
    ladder::ladder_run!(|ex| exp_kernel_body::<F, _>(ex, x, rm))
}

/// Generic body of [`exp_kernel`] (M4, ADR-0059); `None` escalates
/// (M8 ladder). `ex` is the working-precision exemplar (M8b): the
/// receiver the constant and constructor surface reads its width from,
/// never a value the result depends on.
pub(crate) fn exp_kernel_body<F: DecimalFormat, E: ExtNum>(
    ex: E,
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

    let x_ext = ex.from_format(x);
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
    ladder::ladder_run!(|ex| exp2_kernel_body::<F, _>(ex, x, rm))
}

/// Generic body of [`exp2_kernel`] (M4, ADR-0059); `None` escalates
/// (M8 ladder). `ex` is the working-precision exemplar (M8b).
pub(crate) fn exp2_kernel_body<F: DecimalFormat, E: ExtNum>(
    ex: E,
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
    let arg_ext = ex.from_format(x).mul(ex.ln2());
    exp_from_extended_body::<F, E>(arg_ext, rm, &ladder::EXP2)
}

/// Short-circuit the special values shared by the `expm1` family
/// (`expm1` / `exp2m1` / `exp10m1`, IEEE 754-2019 §9.2 and §9.2.1):
/// NaN propagation (sNaN raises `INVALID`), `f(+∞) = +∞`,
/// `f(−∞) = −1` exactly with no exception, and `f(±0) = ±0` (sign
/// preserved, no exception). `None` means finite nonzero: the
/// kernels' domain.
pub(crate) fn expm1_special_cases<F: DecimalFormat>(x: F) -> Option<(F, Status)> {
    match x.classify() {
        Class::SignalingNaN { .. } => Some((x.nan_from(), Status::INVALID)),
        Class::QuietNaN { .. } => Some((x, Status::OK)),
        Class::Infinity { sign } => Some(if sign {
            (F::NEG_ONE, Status::OK)
        } else {
            (F::INFINITY, Status::OK)
        }),
        Class::Zero { .. } => Some((x, Status::OK)),
        Class::Finite { .. } => None,
    }
}

/// The gated deliveries shared by the `expm1` family, on the working
/// argument `u` (already base-scaled by the caller): `Some` is a
/// finished delivery, `None` falls through to the series core.
///
/// * Overflow: `u` past the format's `exp_overflow_limit` makes
///   `e^u − 1` overflow with margin (the gate threshold puts `e^u`
///   a factor of 1.66 to 1.91 past the largest finite magnitude,
///   measured per format at the integer thresholds, and subtracting
///   1 from a value at the 10^emax scale cannot bring it back); the
///   saturation proxy feeds the format rounder directly, exactly as
///   `exp_from_extended_body`'s gate does, and per the 9f30a98
///   lesson it must never reach a guarded delivery.
/// * The −1 band: for `u ≤ −120`, `0 < e^u < 10^−52`, so the true
///   value sits strictly inside `(−1, −1 + 10^−52)` while the
///   ADR-0051 residual channel's denoted interval is
///   `(−1, −1 + 10^−49)`-scaled: both lie strictly between `−1` and
///   the first boundary toward zero at every format (the nearest is
///   `5·10^−35` away at the widest), so every mode's answer is the
///   anchor's (`NearestEven`/`NearestAway`/`TowardNegative` deliver
///   `−1`, the other two its toward-zero neighbor). The gate sits
///   BEFORE the reduction, which also keeps `trunc_to_i32` away
///   from arguments whose reduction integer would not fit.
pub(crate) fn expm1_gates<F: DecimalFormat, E: ExtNum>(
    ex: E,
    u: E,
    rm: RoundingMode,
) -> Option<(F, Status)> {
    if !u.sign() {
        let limit = ex.from_extended(F::exp_overflow_limit());
        if u.cmp(limit) == core::cmp::Ordering::Greater {
            let sat = Extended::saturate_overflow(false);
            let (result, status) =
                F::round_and_pack_finite(sat.coef, sat.exp, 0, sat.sign, true, rm, Status::OK);
            return Some((result, status | Status::INEXACT));
        }
        return None;
    }
    if u.abs().cmp(ex.from_i32(120)) == core::cmp::Ordering::Greater {
        let (result, status) = ex.one().neg().to_format_with_residual::<F>(false, rm);
        return Some((result, status | Status::INEXACT));
    }
    None
}

/// `e^u − 1` at working precision for a finite nonzero `u` inside the
/// gates (`|u| ≤ max(overflow limit, 120)`). Two bands:
///
/// * `|u| ≤ 1.1513` (the reduction's own `k = 0` window): the direct
///   `expm1` series `u + u²/2! + u³/3! + …`, which keeps the result's
///   accuracy relative to `e^u − 1` however small `u` is; on the
///   negative side the alternating terms cancel by at most
///   `e^{|u|} ≤ 3.17`, priced in the family budgets.
/// * Otherwise the `exp` pipeline (`k·ln 10` split, Taylor, decade
///   recomposition) followed by the subtraction of 1, whose
///   cancellation factor `e^u/(e^u − 1)` is at most `1.47` once
///   `|u| > 1.1513` (and at most 1 on the negative side).
pub(crate) fn expm1_ext<E: ExtNum>(ex: E, u: E) -> E {
    if u.abs().cmp(ex.parse_str("1.1513")) != core::cmp::Ordering::Greater {
        let mut sum = u;
        let mut term = u;
        for n in 2u32..=u.exp_series_terms() {
            term = term.mul(u).div_u32(n);
            let next_sum = sum.add(term);
            if next_sum.cmp(sum) == core::cmp::Ordering::Equal {
                sum = next_sum;
                break;
            }
            sum = next_sum;
            if term.is_zero() {
                break;
            }
        }
        return sum;
    }
    exp_extended_body(u).sub(ex.one())
}

/// `expm1(x) = e^x − 1` (IEEE 754-2019 §9.2 `expm1`). The public
/// wrappers are `Decimal128::exp_m1` and the `Decimal64` /
/// `Decimal32` siblings.
///
/// ## Exactness and ties (ADR-0059 classification leg)
///
/// Suppose `e^x − 1 = r` with `r` rational and `x` representable.
/// Then `e^x = 1 + r` is rational too, which for rational `x ≠ 0` is
/// impossible: `e^x` is transcendental there (Lindemann;
/// docs/references/shidlovskii-transcendence.md,
/// docs/references/niven-irrational-numbers.md). So `x = ±0` is the
/// whole exact set, and `expm1_special_cases` delivers it sign
/// preserved and exception free per §9.2.1. A nearest mode tie value
/// is rational, so the same argument rules every tie out. The
/// kernel's unconditional `INEXACT` past the special values is
/// therefore correct in every mode, and every input the ladder rounds
/// sits a finite distance from its rounding boundary (the ladder's
/// standing assumption).
///
/// ## Accuracy
///
/// Correctly rounded on the ADR-0059 escalation ladder: rung 1
/// evaluates at 50 digits and delivers only when the `ladder::EXPM1`
/// budget clears every rounding boundary of the format, otherwise the
/// identical body re-runs at rung 2 (and, under the
/// `unbounded-ladder` feature, at a dynamic rung that widens until
/// the boundary is decided). The budget's itemization lives on
/// `ladder::EXPM1`; the module doc above derives the gates and the
/// two bands.
///
/// Two ADR-0051 anchor seams run before the guard, each on a strict
/// side theorem no finite rung can supply: `e^x − 1 > x` at the
/// argument (the tiny band, where the series collapses onto `x`
/// itself) and `e^x − 1 > −1` at the deep negative end (where the
/// subtraction collapses onto `−1`). `UNDERFLOW` rides the format
/// rounder for subnormal results, which the tiny band reaches because
/// the result hugs the argument (Table 9.1 lists it for this family).
pub fn expm1_kernel<F: DecimalFormat>(x: F, rm: RoundingMode) -> (F, Status) {
    ladder::ladder_run!(|ex| expm1_kernel_body::<F, _>(ex, x, rm))
}

/// Generic body of [`expm1_kernel`] (M4, ADR-0059); `None` escalates
/// (M8 ladder). `ex` is the working-precision exemplar (M8b): the
/// receiver the constant and constructor surface reads its width from,
/// never a value the result depends on.
pub(crate) fn expm1_kernel_body<F: DecimalFormat, E: ExtNum>(
    ex: E,
    x: F,
    rm: RoundingMode,
) -> Option<(F, Status)> {
    if let Some(early) = expm1_special_cases(x) {
        return Some(early);
    }
    let x_ext = ex.from_format(x);
    if let Some(gated) = expm1_gates::<F, E>(ex, x_ext, rm) {
        return Some(gated);
    }
    let result_ext = expm1_ext(ex, x_ext);
    // Grid-stuck at the input (ADR-0051): `e^x − 1 > x` strictly, so
    // the residual side is above x: away from zero for positive x,
    // toward zero for negative x (the mirror of logp1's seam). For
    // |x| ≲ 1e-47 the series collapses to exactly x and this seam is
    // what terminates the unbounded rung there.
    if result_ext.sticks_to(x_ext) {
        let (result, status) = x_ext.to_format_with_residual::<F>(!x_ext.sign(), rm);
        return Some((result, status | Status::INEXACT));
    }
    // Collapse onto the −1 anchor (the deep negative band; shared
    // side theorem `e^x − 1 > −1`).
    if result_ext.sticks_to(ex.one().neg()) {
        let (result, status) = ex.one().neg().to_format_with_residual::<F>(false, rm);
        return Some((result, status | Status::INEXACT));
    }
    ladder::round_guarded::<F, E>(result_ext, rm, &ladder::EXPM1)
}

/// Base 2 exponential minus one: the IEEE 754-2019 §9.2 `exp2m1`
/// operation, `2^x − 1`, public as `Decimal128::exp2_m1` and the
/// sibling `exp2_m1` methods. Evaluated as `expm1(x · ln 2)` so an
/// argument near zero keeps its full relative accuracy instead of
/// losing it to the cancellation `2^x ⊖ 1` would suffer.
///
/// ## Exactness and ties (ADR-0059 classification leg)
///
/// `2^x − 1 = r` rational makes `2^x = 1 + r` rational, and unique
/// factorization then forces a representable `x = a/b` to have
/// `b = 1`: every exact result and every nearest mode tie sits at an
/// integer `x = n`, value `2^n − 1`. The exact family is `2^n − 1`
/// for `1 ≤ n ≤ 112` (34 digits; 53 and 23 at the siblings) and
/// `−(10^m − 5^m)·10^−m` for `n = −m` with `1 ≤ m ≤ PRECISION`;
/// `n = 0` is `x = ±0`, delivered by `expm1_special_cases`.
///
/// Unlike the `logp1` family, this one has real ties, six of them:
/// the positive side's `2^n − 1` ends in 5 exactly when `4 | n`, and
/// the `PRECISION + 1`-digit window holds exactly one such `n`
/// (`116` / `56` / `24`); the negative side's `10^m − 5^m` always
/// ends in 5, so `m = PRECISION + 1` (`35` / `17` / `8`) is a tie at
/// every format. `exact::exp2m1_exact_or_tie` catches all of them
/// input side and delivers through the format rounder, whose own tie
/// rule decides a value the approximation kernel cannot: the true
/// value IS the boundary. Past the classifier the value is
/// irrational, so the unconditional `INEXACT` is correct in every
/// mode and every rounded input sits a finite distance from its
/// rounding boundary (the ladder's standing assumption).
///
/// ## Accuracy
///
/// Correctly rounded on the ADR-0059 escalation ladder from this
/// operation's first release: rung 1 evaluates at 50 digits and
/// delivers only when the `ladder::EXP2M1` budget clears every
/// rounding boundary of the format, otherwise the identical body
/// re-runs at rung 2's 110 digits, and under the `unbounded-ladder`
/// feature at a dynamic rung that widens until the rounding is
/// decided. The budget's itemization lives on `ladder::EXP2M1`.
pub fn exp2m1_kernel<F: DecimalFormat>(x: F, rm: RoundingMode) -> (F, Status) {
    ladder::ladder_run!(|ex| exp2m1_kernel_body::<F, _>(ex, x, rm))
}

/// Generic body of [`exp2m1_kernel`] (M4, ADR-0059); `None` escalates
/// (M8 ladder). `ex` is the working-precision exemplar (M8b): the
/// receiver the constant and constructor surface reads its width from,
/// never a value the result depends on.
pub(crate) fn exp2m1_kernel_body<F: DecimalFormat, E: ExtNum>(
    ex: E,
    x: F,
    rm: RoundingMode,
) -> Option<(F, Status)> {
    if let Some(early) = expm1_special_cases(x) {
        return Some(early);
    }
    // Exact values and the six nearest-mode ties on integer inputs
    // (ADR-0059 Track D): every one at every rounding direction, with
    // no INEXACT on the exact ones (§7.5) and the mode's own tie rule
    // on the midpoints.
    if let Some(exact) = crate::exact::exp2m1_exact_or_tie::<F>(x, rm) {
        return Some(exact);
    }
    let u = ex.from_format(x).mul(ex.ln2());
    if let Some(gated) = expm1_gates::<F, E>(ex, u, rm) {
        return Some(gated);
    }
    let result_ext = expm1_ext(ex, u);
    // No x anchor (slope ln 2 ≠ 1 lands tiny results off grid); the
    // −1 collapse seam per the shared spec. For `u` in roughly
    // `(−120, −107)` the subtraction rounds `e^u` away at working
    // width and the result collapses onto `−1` exactly, a format grid
    // point no rung separates from the true value; `e^u − 1 > −1` for
    // every finite `u`, so the true value lies toward zero from the
    // anchor and `magnitude_grows = false`. Unguarded by design: the
    // ADR-0051 residual leg runs before the ladder's predicate.
    if result_ext.sticks_to(ex.one().neg()) {
        let (result, status) = ex.one().neg().to_format_with_residual::<F>(false, rm);
        return Some((result, status | Status::INEXACT));
    }
    ladder::round_guarded::<F, E>(result_ext, rm, &ladder::EXP2M1)
}

/// Compute `exp(x_ext)` and round to the format. Used by the public
/// `exp` wrapper and by `pow`'s general `exp(y · ln(x))` path.
///
/// Caller is responsible for filtering NaN / Inf / Zero inputs (those
/// have shortcuts that don't go through Taylor). For finite inputs of
/// any magnitude this routine handles the OVERFLOW / UNDERFLOW
/// thresholds internally.
pub fn exp_from_extended<F: DecimalFormat>(x_ext: Extended, rm: RoundingMode) -> (F, Status) {
    // The exemplar slot doubles as the widening seam: `from_extended`
    // is the identity on rung 1 and the width lift on the others.
    ladder::ladder_run!(|ex| exp_from_extended_body::<F, _>(
        ex.from_extended(x_ext),
        rm,
        &ladder::EXP
    ))
}

/// Generic body of [`exp_from_extended`] (M4, ADR-0059); `None`
/// escalates (M8 ladder). The budget is the caller's: `exp` and this
/// function's own wrapper pass [`ladder::EXP`], `exp2` passes
/// [`ladder::EXP2`], and the composed kernels (`pow`, `cbrt`) pass
/// their own composition budgets, so the one guarded delivery site
/// serves every pipeline that ends here with the right total.
///
/// `x_ext` doubles as the working-precision exemplar (M8b): it is a
/// value at the running rung's width, which is all the constant and
/// constructor surface reads off a receiver.
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
    let limit = x_ext.from_extended(if x_ext.sign() {
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
    if !x_ext.is_zero() && result_ext.sticks_to(x_ext.one()) {
        let (result, status) = x_ext.one().to_format_with_residual::<F>(!x_ext.sign(), rm);
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
    let q = x_ext.mul(x_ext.inv_ln10());
    let k = round_to_i32(q);
    let r = x_ext.sub(x_ext.from_i32(k).mul(x_ext.ln10()));

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
        q.sub(q.half())
    } else {
        q.add(q.half())
    };
    nudged.trunc_to_i32()
}

/// `exp(r) = Σ r^n / n!` evaluated at working precision.
///
/// Convergence: `|r| ≤ ln(10)/2 ≈ 1.151`, and `|r|^n / n!` decays
/// faster than geometrically once `n > |r|`. ~36 terms drives the
/// term magnitude below `10^{-49}`, well past `EXT_PRECISION = 50`;
/// the rung's cap ([`ExtNum::exp_series_terms`], read off `r` as the
/// exemplar) scales with its digit count.
fn taylor_exp_ext<E: ExtNum>(r: E) -> E {
    let mut sum = r.one();
    let mut term = r.one();
    // Halt early if `term` falls below the working significance.
    for n in 1u32..=r.exp_series_terms() {
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
