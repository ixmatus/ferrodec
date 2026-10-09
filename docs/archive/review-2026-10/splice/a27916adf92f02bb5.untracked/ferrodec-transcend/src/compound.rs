//! `compound(x, n) = (1 + x)^n` — the IEEE 754-2019 §9.2 `compound`
//! operation, with an integral second operand (ADR-0059 Track D group
//! D3, fd-4zo.25).
//!
//! ## Why it is its own kernel rather than `pow(1 + x, n)`
//!
//! The whole point of the operation is that `1 ⊕ x` must never be
//! formed at the *destination* precision: for a small `x` that addition
//! throws away exactly the digits the result depends on, which is why
//! §9.2 lists `compound` beside `logp1` and `expm1` in the
//! accuracy-preserving family. The base is built at working precision
//! instead, through `ln::logp1_extended_core`, the same core
//! the `logp1` family runs on: below half it feeds `u = x` straight to
//! the `log1p` series (`from_format` is exact at every rung width), and
//! at or above half it forms `t = 1 ⊕ x` where that sum is exact or
//! costs at most one working rounding.
//!
//! ## Algorithm
//!
//! 1. Special values, §9.2.1 ([`compound_special_cases`]).
//! 2. Input-side exact and tie classification
//!    (`exact::compound_exact_input`). Unlike the transcendental
//!    kernels, `compound`'s value is **always rational**: `1 + x` is an
//!    exact rational for every in-domain representable `x`, and an
//!    integer power of a rational is rational. So this classifier is
//!    not a rare-case filter but the operation's whole §7.5 story, and
//!    it owns the on-grid families no rung can decide.
//! 3. The 1-anchor arm (`compound_anchor`): when `|n · log1p(x)|` is
//!    provably tiny the value hugs 1 from a side no finite rung
//!    resolves, and the ADR-0051 residual channel delivers it from the
//!    strict side theorem.
//! 4. Otherwise `exp(n · log1p(x))` at working precision on the
//!    ADR-0059 escalation ladder, with the `ladder::COMPOUND` budget.
//!
//! ## Where the on-grid hazard lives
//!
//! Two families would otherwise sit exactly on a format grid point,
//! which the escalation predicate reads as distance zero and no rung
//! improves on (the D1 `log10p1` and D2 `exp10` lesson, third
//! sighting):
//!
//! * `1 + x = 10^k` — the nines patterns `x = 9, 99, …` and
//!   `x = −0.9, −0.99, …`. Then `compound(x, n) = 10^(k·n)` at *any*
//!   magnitude, in the format's exponent range or far outside it. The
//!   classifier owns this family whole, exactly as `exp10_integer` owns
//!   its own, and hands the rounder the coefficient-1 form so every
//!   §7.4 disposition is correct by construction.
//! * The tiny-`x` band, where the value hugs 1. The anchor arm owns it.
//!
//! Everything else past the `exp` gates is off-grid, so the shared
//! saturation proxy in `exp_from_extended_body` — whose margin argument
//! is the same one `exp` itself relies on — answers it unguarded. That
//! coherence is what lets this kernel reuse `exp`'s gates unchanged
//! instead of carrying its own.
//!
//! ## Accuracy and the claim
//!
//! Correctly rounded on the ADR-0059 escalation ladder from this
//! operation's first release: rung 1 evaluates at 50 digits and
//! delivers only when the `ladder::COMPOUND` budget clears every
//! rounding boundary of the format, otherwise the identical body
//! re-runs at rung 2's 110 digits, and under the `unbounded-ladder`
//! feature at a dynamic rung that widens until the rounding is decided.
//! The budget's itemization lives on `ladder::COMPOUND`.
//!
//! ADR-0060 derives this operation's Liouville floor by its Engine A
//! (rational true values): a non-classified input's value sits at
//! relative distance at least `10^−D` from every boundary with
//! `D ≤ n·w + 36`, `w` the digit width of the exact `1 + x`. Turning
//! that floor into an *unconditional* two-rung claim over the stated
//! operand range (`n · w ≤ 196`) needs the exact integer adjudicator
//! ADR-0060 mandates for the whole algebraic group; that mechanism is a
//! separate slice, and until it lands `compound` carries the standing
//! ADR-0059 tier statement (Tier 1 by construction, Tier 2 model) like
//! the rest of the surface, not the upgraded one.

use crate::extended::ExtNum;
use crate::format::DecimalFormat;
use crate::ladder;
use ferrodec_ieee::{RoundingMode, Status};

/// Apply the IEEE 754-2019 §9.2.1 `compound` rules without touching the
/// working-precision path.
///
/// Returns `Some((result, status))` whenever a §9.2.1 row fires,
/// `None` for the general path (finite nonzero `x > −1` with `n ≠ 0`).
///
/// Loop-free and self-contained, following the ADR-0016 shape
/// [`crate::pow::pow_special_cases`] holds to, so a Kani special-case
/// harness can exhaust the table against this routine alone.
///
/// ## The table, and why the order is what it is
///
/// * A **signaling** NaN raises `INVALID` and quiets, at *every* `n`
///   including `n = 0`. The table's `n = 0` row carves out quiet NaNs
///   only, so the signaling case is tested first rather than folded
///   into it.
/// * `compound(x, 0)` is 1 "for `x ≥ −1` or quiet NaN" — so a quiet NaN
///   at `n = 0` yields 1 rather than propagating, and an `x < −1`
///   (`−∞` included) at `n = 0` takes the invalid-operation row instead
///   of the 1 row. Those two carve-outs are why `n = 0` is decided
///   before NaN propagation and after the `x < −1` test.
/// * `compound(qNaN, n)` propagates for `n ≠ 0`.
/// * `compound(x, n)` is qNaN with `INVALID` for `x < −1`, any `n`.
/// * `compound(−1, n)` is `+∞` with `divideByZero` for `n < 0` and `+0`
///   for `n > 0` (`0^n` at the domain edge).
/// * `compound(±0, n)` is 1, sign of the zero irrelevant.
/// * `compound(+∞, n)` is `+∞` for `n > 0` and `+0` for `n < 0`.
///
/// The result is positive throughout: `1 + x > 0` on the domain and
/// integer powers preserve that, so no sign reconstruction is needed
/// anywhere in this kernel.
pub fn compound_special_cases<F: DecimalFormat>(x: F, n: i32) -> Option<(F, Status)> {
    // Signaling NaN first: the `n = 0` row's carve-out is for *quiet*
    // NaNs, so a signaling operand never reaches it.
    if x.is_signaling_nan() {
        return Some((x.nan_from(), Status::INVALID));
    }
    // `partial_cmp_fmt` answers `None` on a quiet NaN, which reads as
    // "not below −1" — exactly the disposition both rows below want.
    let below_neg_one = matches!(
        x.partial_cmp_fmt(F::NEG_ONE).0,
        Some(core::cmp::Ordering::Less)
    );
    if n == 0 {
        if below_neg_one {
            return Some((F::NAN, Status::INVALID));
        }
        return Some((F::ONE, Status::OK));
    }
    if x.is_nan() {
        return Some((x, Status::OK));
    }
    if below_neg_one {
        return Some((F::NAN, Status::INVALID));
    }
    if matches!(
        x.partial_cmp_fmt(F::NEG_ONE).0,
        Some(core::cmp::Ordering::Equal)
    ) {
        return Some(if n < 0 {
            (F::INFINITY, Status::DIV_BY_ZERO)
        } else {
            (F::ZERO, Status::OK)
        });
    }
    if x.is_zero() {
        return Some((F::ONE, Status::OK));
    }
    if x.is_infinite() {
        // Not below −1 and not finite, so `x = +∞`.
        return Some(if n < 0 {
            (F::ZERO, Status::OK)
        } else {
            (F::INFINITY, Status::OK)
        });
    }
    None
}

/// `compound(x, n) = (1 + x)^n` (IEEE 754-2019 §9.2 `compound`). The
/// public wrappers are `Decimal128::compound` and the `Decimal64` /
/// `Decimal32` siblings.
///
/// ## Exactness and ties (ADR-0059 classification leg)
///
/// `compound`'s value is rational at every in-domain representable
/// input, so the classification question is not "is this one of the
/// rare algebraic cases" but "is this rational representable, a
/// nearest-mode midpoint, or neither". `exact::compound_exact_input`
/// answers it in bounded integer arithmetic from the factorization
/// `1 + x = 2^α · 5^β · t`, and its bail sites carry the completeness
/// proofs the kernel's unconditional `INEXACT` leans on. The ties are
/// real and reachable from both signs of `n` (`compound(4, 49)` and
/// `compound(1, −49)` at `Decimal128`); the format rounder's own tie
/// rule resolves them, which no approximation kernel can do, since the
/// true value IS the rounding boundary.
///
/// ## Preferred exponent (IEEE 754-2019 §9.2.2)
///
/// `Q(compound(x, n))` is `floor(n × min(0, Q(x)))`, read off `x`'s
/// *stored* quantum. The classifier passes it to the format rounder on
/// every exact delivery; §6.3's "as close as the coefficient allows"
/// rule then does the rest, which is why an unattainable preference
/// (`compound(0.25, −1) = 0.8` prefers `+2`) delivers the nearest
/// attainable quantum instead. Inexact deliveries take the shared
/// guarded path's default, exactly as the rest of the §9.2 surface
/// does: their coefficient is already at full precision, so the
/// exponent is fixed by the rounding.
pub fn compound_kernel<F: DecimalFormat>(x: F, n: i32, rm: RoundingMode) -> (F, Status) {
    ladder::ladder_run!(|ex| compound_kernel_body::<F, _>(ex, x, n, rm))
}

/// Generic body of [`compound_kernel`] (M4, ADR-0059); `None` escalates
/// (M8 ladder). `ex` is the working-precision exemplar (M8b): the
/// receiver the constant and constructor surface reads its width from,
/// never a value the result depends on.
pub(crate) fn compound_kernel_body<F: DecimalFormat, E: ExtNum>(
    ex: E,
    x: F,
    n: i32,
    rm: RoundingMode,
) -> Option<(F, Status)> {
    if let Some(early) = compound_special_cases(x, n) {
        return Some(early);
    }
    // Exact values, the nearest-mode ties, and the whole-range
    // power-of-ten family, all decided input side (ADR-0059 M7 /
    // ADR-0060 Engine A). Unguarded by design: an exact coefficient
    // through the format rounder is correct by construction at every
    // rounding direction, and the power-of-ten family sits ON the grid
    // where the ladder's predicate has nothing to measure.
    if let Some(exact) = crate::exact::compound_exact_input::<F>(x, n, rm) {
        return Some(exact);
    }
    // The two ADR-0051 anchor bands, mirror images of each other. Both
    // run before the predicate, per the ADR-0059 tripod: no finite rung
    // separates a grid-hugging residual, the theorem-backed side does.
    //
    // Tiny `x`: the value hugs 1.
    if let Some(anchored) = compound_anchor::<F, E>(ex, x, n, rm) {
        return Some(anchored);
    }
    // Huge `x`: the value hugs `x^n`, because `logp1`'s wide band
    // absorbs the `1` of `1 ⊕ x` once `x` outgrows the working width
    // and the kernel is then evaluating `x^n` instead. ADR-0060 names
    // this `compound`'s second whole-range on-grid family; the
    // derivation, the gate's margin, and the side theorem live on
    // `exact::compound_huge_x_anchor`.
    if let Some((coef, exp)) = crate::exact::compound_huge_x_anchor::<F>(x, n) {
        // The classifier's width gate keeps the coefficient inside
        // `PRECISION + 1 ≤ 35` digits, so it fits `u128`.
        let anchor = ex.from_parts_u128(coef.lo, exp, false);
        let (result, status) = anchor.to_format_with_residual::<F>(n > 0, rm);
        return Some((result, status | Status::INEXACT));
    }
    // `(1 + x)^n = exp(n · log1p(x))`, the base built at working
    // precision so a small `x` never loses its digits to `1 ⊕ x` at the
    // destination width. `exp_from_extended_body` carries the shared
    // over/underflow gates and the `exp` 1-anchor seam; its saturation
    // proxy is sound here because the classifier above already owns the
    // only on-grid family that reaches past those gates.
    let log1p_x = crate::ln::logp1_extended_core::<F, E>(ex, x);
    let v = ex.from_i32(n).mul(log1p_x);
    crate::exp::exp_from_extended_body::<F, E>(v, rm, &ladder::COMPOUND)
}

/// The ADR-0051 anchor arm: deliver `(1 + x)^n` from the grid point 1
/// when the true value provably hugs it, `None` to fall through to the
/// working-precision path.
///
/// ## The gate, and its soundness
///
/// Let `adj` be `x`'s adjusted exponent (`exponent + digit_count − 1`,
/// cohort invariant) and `dn` the decimal digit count of `|n|`. The arm
/// fires when
///
/// > `adj ≤ −(F::PRECISION + dn + 4)`.
///
/// Soundness, in integer-only steps. From the digit counts,
/// `|x| < 10^(adj+1)` and `|n| < 10^dn`. The gate puts `adj ≤ −6`, so
/// `|x| < 10^−5 < 1/2`, and on that range `|log1p(x)| ≤ |x|/(1 − |x|)
/// ≤ 2|x|` (above zero `ln(1+x) ≤ x`; below it
/// `|ln(1+x)| = −ln(1 − |x|) ≤ |x|/(1 − |x|)`). Writing
/// `v = n · log1p(x)`,
///
/// > `|v| ≤ 2|n||x| < 2 · 10^(dn + adj + 1) ≤ 2 · 10^(−P−3)`,
///
/// and `|(1+x)^n − 1| = |e^v − 1| ≤ |v| e^|v| ≤ 1.01|v| ≤
/// 2.02 · 10^(−P−3)`.
///
/// ## Why 1 needs the residual channel at all
///
/// 1 is a format grid point at every precision, and the rounding
/// boundaries beside it are asymmetric: the midpoint above sits at
/// `1 + 5·10^−P`, the midpoint below at `1 − 5·10^(−P−1)`. The binding
/// gap is therefore `5·10^(−P−1)`, and the bound above clears it by
///
/// > `5·10^(−P−1) / (2.02·10^(−P−3)) ≈ 247`,
///
/// a factor of 247 where the discipline asks for ten. Inside that band
/// the true value and the residual channel's denoted interval lie
/// strictly between the same two boundaries, so every rounding
/// direction agrees: the nearest modes deliver 1, and the directed
/// modes are decided by the *side*, which no finite rung supplies and
/// the side theorem does.
///
/// ## The side theorem (strict monotonicity)
///
/// `1 + x > 0` on the domain, so `(1+x)^n = e^(n·ln(1+x))` and
/// `(1+x)^n > 1` exactly when `n · ln(1+x) > 0`. `ln(1+x)` is strictly
/// increasing through `ln(1) = 0`, so it carries the sign of `x`
/// strictly for `x ≠ 0`; with `n ≠ 0` the product's sign is
/// `sign(n) · sign(x)` and never zero. So the value lies strictly above
/// 1 when `x` and `n` share a sign and strictly below it otherwise —
/// which is `magnitude_grows` for a positive anchor.
///
/// Both preconditions hold by construction: the caller's special cases
/// removed `x = 0` and `n = 0`, and the classifier ran first, so no
/// exact or tie value can be diverted into this arm.
///
/// The gate is deliberately conservative — it fires from `adj ≈ −(P+5)`
/// while the working value only collapses onto 1 near `adj ≈ −47` — so
/// it decides a band the ladder would otherwise escalate through, at
/// the cost of nothing, since inside the band every rung would deliver
/// the same answer anyway. `exp_from_extended_body`'s own 1-anchor sits
/// below it on the same side theorem (its side is the sign of `v`,
/// which is this arm's `sign(n)·sign(x)`), so the two agree wherever
/// they overlap.
fn compound_anchor<F: DecimalFormat, E: ExtNum>(
    ex: E,
    x: F,
    n: i32,
    rm: RoundingMode,
) -> Option<(F, Status)> {
    let (coef, exp, sign) = x.to_extended_parts()?;
    if coef.is_zero() {
        return None; // ±0 was disposed of by the special cases
    }
    let adj = exp + coef.decimal_digit_count() as i32 - 1;
    // `n ≠ 0` here, so `ilog10` is defined.
    let n_digits = n.unsigned_abs().ilog10() as i32 + 1;
    if adj > -(F::PRECISION as i32 + n_digits + 4) {
        return None;
    }
    let magnitude_grows = sign == (n < 0);
    let (result, status) = ex.one().to_format_with_residual::<F>(magnitude_grows, rm);
    Some((result, status | Status::INEXACT))
}
