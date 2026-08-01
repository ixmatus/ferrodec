//! The ADR-0059 escalation ladder: per-function error budgets and the
//! guarded format delivery that decides, per call, whether rung 1's
//! 50-digit result is far enough from every rounding boundary to be
//! trusted, or whether the whole kernel re-runs at rung 2's 110
//! digits.
//!
//! ## The mechanism
//!
//! Every kernel body is generic over [`ExtNum`] (M3/M4) and finishes
//! through [`round_guarded`]. On rung 1 ([`Extended`]) the M2
//! predicate [`Extended::near_rounding_boundary`] asks whether the
//! working value lies within the function's *total error budget* of a
//! grid point or a midpoint of the destination format; if it does,
//! the body reports "escalate" (`None`) and the public wrapper
//! re-runs the identical algorithm at rung 2
//! ([`Extended2`](crate::extended2::Extended2)), whose
//! working error is ~60 decimal orders smaller than rung 1's and
//! whose trig reduction replaces rung 1's empirically discharged
//! 38-digit `π/2` truncation with an analytic `< 10^-114` bound
//! (M6). On rung 2 the delivery is unconditional (the Tier 2 model of
//! ADR-0059: expected residual ambiguity is negligible and a build
//! with `--cfg ladder_audit` panics if it is ever observed).
//!
//! Escalation is a deterministic function of the input alone: the
//! predicate is mode-independent and tests both boundary families
//! (grid and midpoint) unconditionally, so a single escalation
//! decision serves every rounding direction identically.
//!
//! What does NOT escalate, by design:
//!
//! * exact and tie deliveries from the input-side classifiers (M7) —
//!   correct by construction;
//! * the ADR-0051 anchor residual paths (`sticks_to` +
//!   `to_format_with_residual`) — they handle asymptotic grid-hugging
//!   distances (e.g. `10^-6000` relative) that no finite rung
//!   separates, and run *before* the predicate;
//! * the saturation proxies (`exp` overflow/underflow, `tanh` nines)
//!   — the true value is provably past the last boundary with margin
//!   recorded at each site;
//! * the direct π/2 and π constant deliveries — certified offline,
//!   once, by `consts::tests` against the 115-digit rung 2 constants
//!   (their boundary distance exceeds the 50-digit representation
//!   error by many orders), instead of paying the predicate per call.
//!
//! ## The budget discipline (the ADR-0050 lesson)
//!
//! Each [`Budget`] below is *rederived from the kernel's operation
//! structure*, itemized in its rustdoc, and padded by a factor of ten
//! on top of the itemized sum. Budgets are deliberately sound rather
//! than tight: the escalation rate is linear in the budget and rung 2
//! is a bounded constant cost, so overshooting a budget by 10× costs
//! ~10× a small escalation probability, while undershooting reopens
//! the correctness exposure this lane exists to close. The campaign
//! audit harness (`argred::tests` + the S1 witness bands) empirically
//! asserts rung 1's observed error stays under a tenth of each
//! audited budget.
//!
//! ## Units
//!
//! A budget is denominated in the M2 predicate's unit: one ULP of the
//! working value widened to the rung's full precision. On rung 1
//! (50 digits) one unit is between `10^-50` and `10^-49` of the
//! value; a relative error bound `R` therefore converts soundly to
//! `R × 10^50` units (the worst case, coefficient at the wide end).
//! Rung 2 budgets use the same rule at `10^110`. Per-op accounting
//! below charges one unit per `ExtNum` rounding operation (each op
//! rounds half-even at working width, ≤ 0.5 ULP, counted as 1),
//! `SERIES(cap)` charges `3 × cap` (multiply, divide, accumulate per
//! iteration — an overcount, since executed iterations stop well
//! short of the cap), and Newton-seeded `div` / `recip` / `sqrt`
//! charge 15 each (seed plus three iterations of a few ops).

use crate::extended::ExtNum;
#[cfg(doc)]
use crate::extended::Extended;
use crate::format::DecimalFormat;
use ferrodec_ieee::{RoundingMode, Status};

/// One function's escalation budget, per rung, in that rung's
/// predicate units (see the module doc for the unit and the ×10 pad
/// discipline; every constant below carries its itemization).
pub(crate) struct Budget {
    /// Rung 1 (50-digit `Extended`) total-error budget.
    pub rung1: u128,
    /// Rung 2 (110-digit `Extended2`) total-error budget, used only
    /// by the `ladder_audit` residual-ambiguity check (delivery on
    /// the top fixed rung is unconditional).
    pub rung2: u128,
}

/// Deliver a working-precision result through the format rounder,
/// guarded by the rung's escalation policy.
///
/// * Rung 1 (`E::ESCALATES`): `None` when the value lies within
///   `budget.rung1` units of any rounding boundary of `F` (the
///   caller's wrapper re-runs the kernel at rung 2). Under
///   `--cfg force_escalate` every call reports `None`, which routes
///   the entire test corpus through rung 2 — the anti-rot
///   byte-identity differential (every existing pin doubles as the
///   reference).
/// * Rung 2 (top fixed rung): delivery is unconditional; under
///   `--cfg ladder_audit` a residual ambiguity (the value still
///   within `budget.rung2` units of a boundary) panics instead of
///   delivering — the Tier 2 exception set made loud.
///
/// The `| INEXACT` matches every guarded call site's contract: by the
/// M7 classification the true result at these sites is irrational,
/// so the flag is unconditionally correct.
pub(crate) fn round_guarded<F: DecimalFormat, E: ExtNum>(
    v: E,
    rm: RoundingMode,
    budget: &Budget,
) -> Option<(F, Status)> {
    if E::ESCALATES {
        #[cfg(force_escalate)]
        {
            return None;
        }
        if v.near_rounding_boundary::<F>(E::rung_budget(budget)) {
            return None;
        }
    } else {
        #[cfg(ladder_audit)]
        assert!(
            !v.near_rounding_boundary::<F>(E::rung_budget(budget)),
            "ladder_audit: top-rung residual ambiguity (value within \
             the rung 2 budget of a rounding boundary)"
        );
    }
    let (result, status) = v.to_format::<F>(0, rm);
    Some((result, status | Status::INEXACT))
}

/// Run the two-rung ladder for a kernel body: rung 1, and on
/// escalation the identical body at rung 2. The top rung's
/// [`round_guarded`] delivery is unconditional, so the second run
/// cannot itself report `None`.
///
/// A plain function (not a method) so the wrappers stay one
/// expression; the closures monomorphize per rung with zero dispatch.
pub(crate) fn run<F: DecimalFormat>(
    rung1: impl FnOnce() -> Option<(F, Status)>,
    rung2: impl FnOnce() -> Option<(F, Status)>,
) -> (F, Status) {
    match rung1() {
        Some(result) => result,
        None => rung2().expect("top rung delivers unconditionally"),
    }
}

// ----------------------------------------------------------------------------
// The budget catalog. Derivation constants used throughout:
//
// * `K_EXP = 6146`: the largest decade-split integer in `exp`'s
//   reduction (`|x| ≤ 14150` over `ln 10`), so `|k·ln10| ≤ 14151`.
// * The constants (`ln 10`, `ln 2`, `π/2`, …) carry ≤ 0.5 unit of
//   representation error at the rung's width; a product with an exact
//   integer or format-sourced value adds 1 unit of rounding, so a
//   "const-multiply" item charges 1.5 units *of the product's own
//   magnitude*, converted to result-relative units at the site's
//   documented magnitude floor.
// * Series items are `3 × cap` per the module doc; rung 1 caps are
//   EXP 60, SIN_COS 120, SINH_COSH 120, LOG1P 250, ATAN 200, and the
//   rung 2 caps 120 / 240 / 240 / 550 / 450 (M5).

/// `exp`. Itemization (rung 1):
///
/// * Reduction `r = x − k·ln10`: the `k·ln10` const-multiply carries
///   ≤ 1.5 units of its own ≤ 14151 magnitude, an *absolute* error of
///   ≤ `14151 × 1.5 × 10^-50 ≈ 2.2e-46`, and `d(e^r)/e^r = dr` maps
///   it 1:1 into result-relative error: ≤ 22,200 units. The closing
///   subtraction adds 1 unit.
/// * Taylor at cap 60: ≤ 180 units.
/// * Decade recomposition `mul_pow10_exp`: exact, 0 units.
///
/// Sum ≈ 22,400; ×10 pad → 250,000 (rounded up). Rung 2: identical
/// structure (the amplification is precision-independent), cap 120
/// series → ≈ 22,600; padded → 250,000 in rung 2 units.
pub(crate) const EXP: Budget = Budget {
    rung1: 250_000,
    rung2: 250_000,
};

/// `exp2 = exp(x·ln2)`. Itemization: the argument const-multiply
/// `x·ln2` (magnitude ≤ 14151 by the overflow gate) adds another
/// ≤ 22,200 units of absolute-in-argument error on top of [`EXP`]'s
/// items. Sum ≈ 44,600; ×10 → 500,000. Same shape both rungs.
pub(crate) const EXP2: Budget = Budget {
    rung1: 500_000,
    rung2: 500_000,
};

/// `ln`. Itemization (rung 1):
///
/// * Near-1 direct path (`|x−1| < 0.5`): `u` exact, `log1p` series at
///   cap 250 → ≤ 750 units, relative to the result by the series'
///   construction (fd-aqs.6). An input whose working result lands
///   *on* a format grid point (e.g. full-width `u` where `u − u²/2`
///   rounds back to `u` at 50 digits) sits at predicate distance
///   zero and escalates on any positive budget.
/// * Decade path (`|x−1| ≥ 0.5`, so `|ln x| ≥ 0.405`): `q·ln10`
///   const-multiply, absolute ≤ `|q|·2.303·1.5e-50` against a result
///   magnitude ≥ `max(0.405, 2.303|q| − 2.9)` → ≤ ~120 units at the
///   worst small-`|q|` ratio; halve/double loop ≤ 20 × 2 = 40; the
///   `log1p` core ≤ 750; closing adds ≤ 3.
///
/// Sum ≤ ~950; ×10 → 15,000 (rounded up). Rung 2: cap 550 series →
/// ≤ 1,650 + 160 ≈ 1,850; ×10 → 25,000.
pub(crate) const LN: Budget = Budget {
    rung1: 15_000,
    rung2: 25_000,
};

/// `log10 = ln(x) · (1/ln10)`: [`LN`] plus one const-multiply on the
/// *result* (relative, ≤ 1.5 units). Same constants as [`LN`] after
/// the pad absorbs it.
pub(crate) const LOG10: Budget = Budget {
    rung1: 15_000,
    rung2: 25_000,
};

/// `log2 = ln(x) · (1/ln2)`: as [`LOG10`].
pub(crate) const LOG2: Budget = Budget {
    rung1: 15_000,
    rung2: 25_000,
};

/// `sin`. Itemization (rung 1):
///
/// * Payne–Hanek `π/2` truncation: the 38-digit `PI_OVER_TWO_COEF_38`
///   carries ≤ `10^-37` relative error into the reduced argument
///   (argred's fd-aqs.10 discharge — this is the *analytic* bound;
///   the measured contribution is ~6.3e-38), and the reduced range
///   `|y| ≤ π/4` maps argument-relative into result-relative with
///   factor `|y·cot y| ≤ 1` (sin) — ≤ 10^13 units. THE dominant item,
///   and the budget's honest carrier of the S1-falsified thin spot:
///   rung 1 cannot resolve boundaries closer than its own reduction
///   error, so this term is what routes the witness inputs to rung 2.
/// * Window truncation (43 guaranteed surviving digits at the deepest
///   cancellation): ≤ 10^-43 relative → 10^7 units.
/// * Taylor at cap 120 → ≤ 360; octant recomposition ≤ 10.
///
/// Sum ≈ 1.0e13; ×10 → 1.5e14 (rounded up; escalation ≈ 2 × budget ×
/// 10^-16 ≈ 3% of Decimal128 calls, negligible at the narrower
/// formats — the price of an analytic rather than empirical
/// discharge, until the reduction bound tightens). Rung 2: the
/// `reduce_wide` truncation is `< 10^-114` relative (< 10^-4 units,
/// M6), window ≥ 110 surviving digits (~1 unit), series at cap 240 →
/// ≤ 720, recomposition ≤ 10 → sum ≈ 740; ×10 → 10,000.
pub(crate) const SIN: Budget = Budget {
    rung1: 150_000_000_000_000,
    rung2: 10_000,
};

/// `cos`: identical pipeline to [`SIN`]; the reduced-range map factor
/// is `|y·tan y| ≤ π²/16 < 1`.
pub(crate) const COS: Budget = Budget {
    rung1: 150_000_000_000_000,
    rung2: 10_000,
};

/// `tan = sin/cos` on the shared reduction: both operands' relative
/// errors add through the quotient (≤ 2 × the [`SIN`] sum) plus a
/// Newton division (15). ×10 → 3e14 / 25,000.
pub(crate) const TAN: Budget = Budget {
    rung1: 300_000_000_000_000,
    rung2: 25_000,
};

/// `atan`. Itemization (rung 1): outer `recip` inversion ≤ 15, inner
/// `tan(π/8)` reduction (sub + div) ≤ 16, Taylor at cap 200 → ≤ 600,
/// `π/2 − result` recomposition against a result ≥ π/8 in that branch
/// ≤ 10. Sum ≈ 640; ×10 → 10,000. Rung 2: cap 450 → ≈ 1,400; ×10 →
/// 20,000.
pub(crate) const ATAN: Budget = Budget {
    rung1: 10_000,
    rung2: 20_000,
};

/// `asin = 2·atan(x / (1 + sqrt((1−|x|)(1+|x|))))` (fd-aqs.6, exact
/// factors for format-sourced `x`): sqrt 15 + div 15 + the [`ATAN`]
/// core ≈ 640 + doubling 1. Sum ≈ 680; ×10 → 10,000 / rung 2 20,000.
pub(crate) const ASIN: Budget = Budget {
    rung1: 10_000,
    rung2: 20_000,
};

/// `acos = 2·atan(sqrt((1−x)/(1+x)))` (fd-aqs.6, exact factors):
/// div 15 + sqrt 15 + [`ATAN`] core ≈ 640 + doubling 1; the large-`t`
/// branch's `π/2 − atan(1/t)` recomposition is relative-safe against
/// its ≥ π/2 result. Sum ≈ 680; ×10 → 10,000 / 20,000.
pub(crate) const ACOS: Budget = Budget {
    rung1: 10_000,
    rung2: 20_000,
};

/// `atan2`: quotient `y/x` (15) + [`ATAN`] core (≈ 640) + quadrant
/// `±π` adjustment (absolute ≤ 1.5 units of π against a result
/// magnitude ≥ π/2 in the adjusted quadrants → ≤ 4). Sum ≈ 660;
/// ×10 → 10,000 / 20,000. The exact-axis and constant deliveries
/// bypass the guard.
pub(crate) const ATAN2: Budget = Budget {
    rung1: 10_000,
    rung2: 20_000,
};

/// `sinh`. Itemization (rung 1): small band (`|x| < 0.5`) is the
/// all-positive Taylor at cap 120 → ≤ 360 relative units, no
/// cancellation. Else `(e^x − e^{-x})/2`: two [`EXP`]-core runs
/// (≤ 22,400 relative each) through a subtraction whose cancellation
/// factor is `coth|x| ≤ coth(0.5) ≈ 2.17` at the band edge →
/// ≤ 98,000; halving is exact-adjacent (≤ 1). Sum ≈ 98,000; ×10 →
/// 1,000,000. Same shape both rungs.
pub(crate) const SINH: Budget = Budget {
    rung1: 1_000_000,
    rung2: 1_000_000,
};

/// `cosh = (e^x + e^{-x})/2`: two [`EXP`]-core runs through an
/// *addition* (no cancellation, factor ≤ 1) → ≤ 45,000; small band
/// Taylor ≤ 360. ×10 → 500,000. Both rungs.
pub(crate) const COSH: Budget = Budget {
    rung1: 500_000,
    rung2: 500_000,
};

/// `tanh = sinh/cosh` below the nines-saturation band: the two cores'
/// relative errors add through the quotient (≤ [`SINH`]'s 98,000 +
/// [`COSH`]'s 45,000) + division 15. ×10 → 1,500,000. Both rungs.
pub(crate) const TANH: Budget = Budget {
    rung1: 1_500_000,
    rung2: 1_500_000,
};

/// `asinh`. Small band (`|x| < 0.3`, fd-aqs.6):
/// `log1p(|x| + x²/(1 + sqrt(1+x²)))` — square 1 + sqrt 15 + div 15 +
/// adds 2, amplified through `log1p` by
/// `u/((1+u)·log1p(u)) ≤ ~1.2` → ≤ 50, plus the series ≤ 750. Large
/// band: `ln(|x| + sqrt(x²+1))` with the argument ≥ 2.3 (so
/// `|result| ≥ 0.85`): argument ops ≤ 33 against that floor ≤ 40,
/// plus [`LN`]'s ≈ 950. Sum ≤ ~1,000; ×10 → 15,000. Rung 2 series
/// caps → ≤ ~2,000; ×10 → 30,000.
pub(crate) const ASINH: Budget = Budget {
    rung1: 15_000,
    rung2: 30_000,
};

/// `acosh`. Near-1 band (`x − 1 < 0.01`, fd-aqs.6): exact factors
/// `(x−1)(x+1)`, sqrt 15, adds 2, `log1p` amplification ≤ 1.2 and
/// series ≤ 750 → ≤ 800. Direct band: `x² − 1` loses ≤ 2 digits at
/// the 0.01 threshold (documented at the site) → the square + sub
/// carry ≤ 100× of 2 units ≈ 200, sqrt halves it + 15, and the `ln`
/// amplification against `|result| ≥ 0.14` ≈ ×7 → ≤ 1,700 + [`LN`]'s
/// ≈ 950. Sum ≤ ~2,700; ×10 → 30,000. Rung 2 → 50,000.
pub(crate) const ACOSH: Budget = Budget {
    rung1: 30_000,
    rung2: 50_000,
};

/// `atanh`. Small band (`|x| < 0.15`): `½·log1p(2x/(1−x))` with
/// exact `1−x` — div 15, `log1p` amplification ≤ 1.2, series ≤ 750,
/// halving 1 → ≤ 800. Ratio band: `½·ln((1+x)/(1−x))` with exact
/// numerator and denominator, div 15, `ln` against
/// `|result| ≥ atanh(0.15) ≈ 0.151`… the ratio path's `ln` argument
/// is ≥ 1.35 so `|ln| ≥ 0.30`: ops ≤ 17 against that floor ≤ 60 +
/// [`LN`]'s ≈ 950. Sum ≤ ~1,050; ×10 → 15,000. Rung 2 → 30,000.
pub(crate) const ATANH: Budget = Budget {
    rung1: 15_000,
    rung2: 30_000,
};

/// `cbrt = exp(ln|x|/3)`: `ln`'s relative error (≤ 950 units,
/// ≈ 9.5e-48) becomes absolute through `|ln x| ≤ 14151` →
/// ≤ `1.35e-43` absolute in the exp argument after the exact-ish
/// `div_u32(3)` (÷3, +1 unit), i.e. ≤ 4.5e6 result-relative units,
/// plus the [`EXP`] Taylor ≤ 180. Sum ≈ 4.6e6; ×10 → 5e7. Rung 2:
/// `ln` ≤ 1,850 → ≤ 8.8e6; ×10 → 1e8.
pub(crate) const CBRT: Budget = Budget {
    rung1: 50_000_000,
    rung2: 100_000_000,
};

/// `pow = exp(y·ln|x|)`: `ln`'s relative error (≤ 950 units) plus the
/// product rounding (1 unit) become absolute through
/// `|y·ln x| ≤ 14151` (the overflow gate) → ≤ `1.35e-43` absolute →
/// ≤ 1.35e7 result-relative units, plus [`EXP`]'s Taylor ≤ 180. The
/// near-1-base hazard (ADR-0050) is inside the `ln` relative model by
/// the fd-aqs.6 direct path, so no separate item. Sum ≈ 1.4e7;
/// ×10 → 1.5e8. Rung 2: `ln` ≤ 1,850 → ≤ 2.6e7; ×10 → 3e8.
pub(crate) const POW: Budget = Budget {
    rung1: 150_000_000,
    rung2: 300_000_000,
};

// Escalation-rate summary (Decimal128, the widest exposure; rate ≈
// 2 × rung1 × 10^-16 for a random input): trig ≈ 3%, tan ≈ 6%,
// pow ≈ 3e-8, cbrt ≈ 1e-8, exp family ≈ 1e-10, everything else
// ≤ 3e-12. Decimal64 rates are 10^18× smaller, Decimal32 smaller
// still. The trig rate is the cost of carrying argred's analytic
// (not empirical) truncation bound; tightening it means moving the
// reduction bound, not shaving the pad.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extended::Extended;
    use crate::extended2::Extended2;

    /// The ADR-0059 budget audit over the historically falsifying
    /// bands: for every S1 witness input (the Arb-certified
    /// high-decade Decimal128 trig misrounds), the observed rung 1
    /// working error must stay under a **tenth** of the trig budgets.
    /// Rung 2 serves as the oracle — its own error over these bands
    /// is bounded by `reduce_wide`'s analytic `< 10^-114` truncation
    /// plus series noise, ~70 decimal orders below the quantity under
    /// audit, and its verdicts are independently pinned by the
    /// witness replay against Arb (`tests/transcend_campaign_s1.rs`)
    /// and the `force_escalate` corpus differential.
    ///
    /// `tan`'s quotient is audited through its components: sin and
    /// cos each within `B/10 = 1.5e-37` relative bounds the quotient
    /// within `~3e-37 ≤ TAN.rung1/10`. An unsound budget here is the
    /// ADR-0050 failure shape and must stop the lane, not shrink the
    /// assertion.
    #[cfg(feature = "trig")]
    #[test]
    fn s1_witness_bands_stay_under_a_tenth_of_the_trig_budget() {
        use crate::mock_format::ValueFmt128;
        use crate::sincos::sincos_extended_body;
        use std::path::PathBuf;
        use std::vec::Vec;

        // B/10 for SIN and COS rung 1: budget 1.5e14 units of 1e-50
        // → 1.5e-36 relative; a tenth is 1.5e-37. Compared as values
        // (`|Δ| ≤ |v| · 1.5e-37`), not decades, because the observed
        // worst rows sit in the 1e-37 decade — at the itemized sum,
        // exactly where the audit must resolve finer than a decade.
        let tenth = Extended2::parse_str("1.5e-37");

        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/vectors/transcend/campaign/s1");
        let mut audited = 0usize;
        for file in [
            "sin_misrounds.tsv",
            "cos_misrounds.tsv",
            "tan_misrounds.tsv",
        ] {
            let text = std::fs::read_to_string(dir.join(file))
                .unwrap_or_else(|e| panic!("read {file}: {e}"));
            for line in text.lines() {
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                let fields: Vec<&str> = line.split('\t').collect();
                let x_raw = fields[4];
                // `<coef>E<exp>`: 34-digit coefficient, decimal
                // exponent (the S1 sweep's canonical shape).
                let (coef_s, exp_s) = x_raw
                    .split_once('E')
                    .unwrap_or_else(|| panic!("{file}: malformed input {x_raw}"));
                let x = ValueFmt128 {
                    coef: coef_s.parse::<u128>().unwrap(),
                    exp: exp_s.parse::<i32>().unwrap(),
                    sign: false,
                };
                let (s1, c1, _) = sincos_extended_body::<ValueFmt128, Extended>(Extended::ZERO, x);
                let (s2, c2, _) =
                    sincos_extended_body::<ValueFmt128, Extended2>(Extended2::ZERO, x);
                for (name, v1, v2) in [("sin", s1, s2), ("cos", c1, c2)] {
                    let d = Extended2::from_extended(v1).sub(v2).abs();
                    if d.is_zero() {
                        continue;
                    }
                    let bound = v2.abs().mul(tenth);
                    assert!(
                        d.cmp(bound) != core::cmp::Ordering::Greater,
                        "{file} {name}({x_raw}): rung 1 error {d:?} \
                         exceeds a tenth of the budget ({bound:?}) — \
                         the budget model is unsound over its \
                         falsifying band (the ADR-0050 shape); stop \
                         the lane",
                    );
                }
                audited += 1;
            }
        }
        // The corpus rows all audited (sin 643 + cos 570 + tan 606).
        assert_eq!(audited, 1819, "witness corpus row count drifted");
    }

    /// Every budget is nonzero on both rungs (a zero budget would
    /// silently disable the guard) and the rung-1 side stays far
    /// below the predicate's structural ceiling (boundaries are
    /// ≥ 10^49 units apart, so a budget approaching that would
    /// escalate everything).
    #[test]
    fn budgets_are_positive_and_sane() {
        let all: [(&str, &Budget); 20] = [
            ("exp", &EXP),
            ("exp2", &EXP2),
            ("ln", &LN),
            ("log10", &LOG10),
            ("log2", &LOG2),
            ("sin", &SIN),
            ("cos", &COS),
            ("tan", &TAN),
            ("atan", &ATAN),
            ("asin", &ASIN),
            ("acos", &ACOS),
            ("atan2", &ATAN2),
            ("sinh", &SINH),
            ("cosh", &COSH),
            ("tanh", &TANH),
            ("asinh", &ASINH),
            ("acosh", &ACOSH),
            ("atanh", &ATANH),
            ("cbrt", &CBRT),
            ("pow", &POW),
        ];
        for (name, b) in all {
            assert!(b.rung1 > 0 && b.rung2 > 0, "{name}: zero budget");
            assert!(
                b.rung1 < 10u128.pow(20),
                "{name}: rung 1 budget {} implausibly wide",
                b.rung1
            );
            assert!(
                b.rung2 < 10u128.pow(20),
                "{name}: rung 2 budget {} implausibly wide",
                b.rung2
            );
        }
    }

    /// The rung hooks pick their own side of the pair.
    #[test]
    fn rung_budget_selects_by_rung() {
        let b = Budget {
            rung1: 7,
            rung2: 11,
        };
        assert_eq!(<Extended as ExtNum>::rung_budget(&b), 7);
        assert_eq!(<Extended2 as ExtNum>::rung_budget(&b), 11);
        assert!(<Extended as ExtNum>::ESCALATES);
        assert!(!<Extended2 as ExtNum>::ESCALATES);
    }
}
