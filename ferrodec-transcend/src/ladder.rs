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
//! Under the `unbounded-ladder` feature (M8b) the ladder has no top
//! fixed rung: rung 2 escalates on its own budget exactly as rung 1
//! does, and [`run3`]'s Ziv loop re-runs the kernel on the dynamic
//! rung (`ExtendedDyn`, per-attempt arena, constants computed at run
//! time) at 220 digits, doubling the width until the predicate clears
//! at that width's `budget.dynamic(p)`. There is then no Tier 2
//! exception set — a near-boundary verdict always widens instead of
//! delivering — and `ladder_audit` is vacuous by construction in such
//! builds (nothing delivers unconditionally for it to audit); the cfg
//! keeps its meaning for default builds, where rung 2 is still the
//! top. The `--cfg force_rung3` test lane routes both fixed rungs'
//! guarded deliveries to the dynamic rung, making every existing pin
//! a byte-identity reference for it, exactly as `force_escalate` does
//! for rung 2.
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
//!   error by many orders), instead of paying the predicate per call;
//! * `log10p1`'s integer-anchor family (`x = 10^n`, `n ≥ 36`) — the
//!   true value sits `10^−n/ln 10` above the representable integer
//!   `n`, a residual the wide band's `1 ⊕ x` absorption lands exactly
//!   ON the grid once `n` passes the rung width, so the ADR-0051
//!   residual channel decides it input side (the side theorem and the
//!   boundary-margin proof live on
//!   `exact::log10p1_power_of_ten_exponent`);
//! * `exp10m1`'s whole integer family (`x = n`) — the mirror class:
//!   `10^n ⊖ 1` keeps every digit of `10^n` past the working width,
//!   landing the working value exactly ON the grid point `1·10^n`, so
//!   `exact::exp10m1_integer` answers every integer input side, the
//!   nines patterns exactly and the rest through an all nines proxy
//!   whose soundness is total digit knowledge rather than a margin
//!   (the proof lives at that classifier)..
//! * `powi`'s exactly-expressible family (`x^n` with at most
//!   `PRECISION + 1` stripped digits), the results *outside* the
//!   format's exponent range included. Such a value is a grid point
//!   or a midpoint at its own exponent whether or not that exponent
//!   is representable, so the predicate reads distance zero and no
//!   rung improves on it; `exact::powi_exact_input` hands the rounder
//!   the true coefficient and the §7.4 disposition decides every
//!   mode. The one family that classifier declines while still
//!   sitting on the grid — an exact value whose decimal exponent
//!   overflows `i32` — is astronomically past the `exp` gates, so the
//!   saturation proxy answers it unguarded (the proof lives on the
//!   classifier); together the two cases are what keeps `ladder_audit`
//!   non-panicking over `powi`.
//! * `exp10`'s integer family (`x = n`), the integers *outside* the
//!   format's exponent range included, where the delivery is an
//!   overflow or underflow rather than an exact value. `10^n` is a
//!   grid point at its own exponent whether or not that exponent is
//!   representable, so the predicate reads distance zero and no rung
//!   improves on it; `exact::exp10_integer` hands the rounder the true
//!   value and the §7.4 disposition decides every mode.
//! * `rootn`'s hug-at-1 arm — for `|ln|x|| / |n|` below a per format
//!   threshold the true value sits strictly between 1 and the first
//!   rounding boundary beside it, at a relative distance that falls
//!   as `1/|n|` and reaches `~10^-43` at `Decimal128`, so the
//!   ADR-0051 residual channel decides it input side on the side
//!   theorem `rootn(x, n) > 1 iff (x > 1) XOR (n < 0)` (the
//!   derivation and the threshold's margin live on
//!   `crate::rootn::rootn_hug_one`);
//! * `compound`'s two on-grid families — the third sighting of the
//!   same class. `1 + x = 10^k` (the nines patterns) makes
//!   `(1 + x)^n = 10^(k·n)` a grid point at its own exponent for every
//!   `n`, in the format's range or far outside it, so
//!   `exact::compound_exact_input` owns the family whole and the §7.4
//!   disposition decides every mode; and the tiny-`n·x` band, where the
//!   value hugs 1 inside a proven fraction of the distance to the first
//!   boundary beside it, is delivered through the ADR-0051 residual
//!   channel on the side theorem `sign((1+x)^n − 1) = sign(n)·sign(x)`
//!   (the derivation lives on `compound::compound_anchor`);
//! * the `expm1` family's shared gates (`expm1` / `exp2m1` /
//!   `exp10m1`) — the overflow saturation proxy (the true value is
//!   provably past the last boundary with measured margin) and the
//!   `−1` band delivery for arguments past `−120`, where
//!   `e^u < 10^-52` sits inside the ADR-0051 residual channel's
//!   snap band; the post-series `−1` collapse seam behind the gate
//!   is the same channel (side theorem `e^u − 1 > −1`), and
//!   `expm1` itself adds the x anchor on `e^x − 1 > x`.
//! * `hypot`'s anchor band (magnitude ratio `≤ 10^−⌈(P+2)/2⌉`) — the
//!   true value hugs the larger operand's own grid point `|w|` from
//!   above by at most `ρ²/2`, which the gate holds two decades inside
//!   the first boundary above `|w|`, so the ADR-0051 residual channel
//!   decides every mode input side on the side theorem
//!   `hypot(w, z) > |w|` for `z ≠ 0`. Here the gate is a *ratio*
//!   test on exponents and digit counts, not a post-hoc `sticks_to`
//!   observation: the band is proven before any arithmetic runs (the
//!   derivation lives on `crate::hypot`, and ADR-0060 is its source).
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

/// The identity of one rounding boundary of a destination format: the
/// exact rational `coef · 10^exp`, tagged with the §7.4 decision
/// family it belongs to. The escalation predicate locates it
/// ([`ExtNum::candidate_boundary`]); the ADR-0060 exact integer
/// adjudicator consumes it, deciding the true value's side of this
/// exact rational in bounded integer arithmetic.
///
/// `coef` fits `u128` structurally. The kept coefficient at the drop
/// position carries at most `F::PRECISION ≤ 34` digits (the drop is at
/// least the precision excess, and a subnormal drop only widens it),
/// so a grid coefficient has at most 34 digits, the all-nines carry
/// lands on `10^34`, and a midpoint has at most 35 digits; every one
/// sits below `u128::MAX ≈ 3.4·10^38`. `coef` is nonzero on every
/// reachable path: the zero grid point could only be flagged on a full
/// drop, where the distance to it is the whole widened coefficient
/// (at least `10^49` units at the narrowest rung), beyond any `u128`
/// budget — [`Boundary::lower_grid`] carries the `debug_assert`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Boundary {
    /// Boundary coefficient: at most `10^P` for a grid point (the
    /// carry case included), `P + 1` digits ending in 5 for a
    /// midpoint. Not necessarily normalized — the carry case `10^P`
    /// denotes the same value as coefficient 1 one decade up, and
    /// consumers treat the pair `(coef, exp)` as the exact rational it
    /// is.
    pub coef: u128,
    /// Decimal exponent: `coef · 10^exp` is the boundary, exactly.
    pub exp: i32,
    /// The decision family.
    pub kind: BoundaryKind,
}

/// The two rounding decision families of the §7.4 disposition: format
/// grid points (deciding the directed modes and `INEXACT`) and the
/// midpoints between adjacent grid points (deciding the nearest
/// modes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BoundaryKind {
    /// A representable value of the destination format.
    Grid,
    /// The exact midpoint between two adjacent grid points.
    Midpoint,
}

impl Boundary {
    /// The grid point just below the working value: the kept
    /// coefficient at the drop exponent. `kept` is nonzero on every
    /// reachable path (see the type doc); the assert keeps the
    /// invariant loud.
    pub(crate) fn lower_grid(kept: u128, exp: i32) -> Self {
        debug_assert!(kept != 0, "the zero grid point is beyond any u128 budget");
        Boundary {
            coef: kept,
            exp,
            kind: BoundaryKind::Grid,
        }
    }

    /// The grid point just above the working value. The all-nines
    /// carry produces `10^P`, the same boundary as coefficient 1 one
    /// decade up; consumers read the exact rational, so the
    /// non-normalized spelling is harmless and keeps this constructor
    /// branch free.
    pub(crate) fn upper_grid(kept: u128, exp: i32) -> Self {
        Boundary {
            coef: kept + 1,
            exp,
            kind: BoundaryKind::Grid,
        }
    }

    /// The midpoint between the two grid points bracketing the
    /// working value: `10·kept + 5` one exponent below the drop.
    pub(crate) fn midpoint(kept: u128, exp: i32) -> Self {
        Boundary {
            coef: 10 * kept + 5,
            exp: exp - 1,
            kind: BoundaryKind::Midpoint,
        }
    }
}

/// One rung's escalation verdict with the boundary identity kept
/// instead of collapsed to a bool. [`ExtNum::near_rounding_boundary`]
/// is the bool view: `Clear` maps to `false`, both other variants to
/// `true`, so the two stay one computation and cannot drift.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BoundaryVerdict {
    /// Every boundary lies beyond the budget: this rung's rounding is
    /// decided and unconditional delivery is sound.
    Clear,
    /// The working value lies within the budget of this boundary; when
    /// the budget sits below a quarter of the drop field the hit is
    /// unique — ADR-0060's "single candidate boundary", which every
    /// adjudicating budget satisfies by sixty decimal orders. On wider
    /// budgets (rung 1's trig scale against a narrow drop) the
    /// reported boundary is the nearest, ties broken grid before
    /// midpoint and lower before upper.
    Near(Boundary),
    /// Near by fiat, with no single boundary identity: the zero
    /// working value (no exponent to define the unit) and the
    /// degenerate no-drop case (the value sits on its own
    /// working-width grid point, distance zero to itself). Neither is
    /// reachable from a real format's guarded delivery; the variant
    /// keeps the predicate total for hypothetical wide formats, and
    /// the adjudication seam treats it as undecidable, exactly as the
    /// bool view treats it as near.
    NearIndeterminate,
}

impl BoundaryVerdict {
    /// The bool view the escalation ladder consumes: is the rounding
    /// undecided at this budget?
    pub(crate) fn is_near(self) -> bool {
        !matches!(self, BoundaryVerdict::Clear)
    }
}

/// One function's escalation budget, per rung, in that rung's
/// predicate units (see the module doc for the unit and the ×10 pad
/// discipline; every constant below carries its itemization).
pub(crate) struct Budget {
    /// Rung 1 (50-digit `Extended`) total-error budget.
    pub rung1: u128,
    /// Rung 2 (110-digit `Extended2`) total-error budget. Without the
    /// `unbounded-ladder` feature it feeds only the `ladder_audit`
    /// residual-ambiguity check (delivery on the top fixed rung is
    /// unconditional); with the feature it is rung 2's escalation
    /// threshold, exactly as `rung1` is rung 1's.
    pub rung2: u128,
    /// Dynamic-rung budget at working precision `p`: the same
    /// itemization as the fixed rungs re-evaluated at `p` (series
    /// items scale with the precision-derived caps, the constant
    /// items are precision-independent, Newton-seeded ops charge a
    /// flat 60 covering the derived step count for every supported
    /// `p`), ×10 pad included by each formula. A plain fn pointer so
    /// the catalog stays const. Read only by the dynamic rung's
    /// `rung_budget`.
    #[cfg_attr(not(feature = "unbounded-ladder"), allow(dead_code))]
    pub dynamic: fn(u32) -> u128,
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
        // The test-lane skips key on the rung position, not on
        // ESCALATES, so each lane keeps its meaning in every build:
        // force_escalate routes rung 1 to rung 2 (and no further),
        // force_rung3 routes both fixed rungs to the dynamic one
        // (meaningful only with `unbounded-ladder`; without it rung 2
        // has ESCALATES = false and delivers below).
        #[cfg(force_escalate)]
        if E::RUNG == 1 {
            return None;
        }
        #[cfg(force_rung3)]
        if E::RUNG <= 2 {
            return None;
        }
        if v.near_rounding_boundary::<F>(v.rung_budget(budget)) {
            return None;
        }
    } else {
        #[cfg(ladder_audit)]
        assert!(
            !v.near_rounding_boundary::<F>(v.rung_budget(budget)),
            "ladder_audit: top-rung residual ambiguity (value within \
             the rung 2 budget of a rounding boundary)"
        );
    }
    let (result, status) = v.to_format::<F>(0, rm);
    Some((result, status | Status::INEXACT))
}

/// [`round_guarded`] with the ADR-0060 exact integer adjudicator on
/// the rung 2 ambiguous path — the delivery the five algebraic
/// kernels (`rsqrt`, `hypot`, `powi`'s powering arm, `rootn`,
/// `compound`) run instead of the plain guard.
///
/// Rungs 1 and 3 delegate to [`round_guarded`] verbatim, so
/// `force_escalate` and `force_rung3` keep their meanings for these
/// operations exactly as for every other kernel (the `force_rung3`
/// arm below fires before the predicate, preserving the Ziv
/// termination lane). Rung 2 — in EVERY build — runs the identity
/// predicate on each delivery:
///
/// * `Clear`: deliver unconditionally, exactly as [`round_guarded`].
/// * `Near(b)` with `decide(b) = Some(side)`: the operands are inside
///   the operation's adjudicable range and the decider has computed,
///   in exact integer arithmetic, which side of the boundary the true
///   value sits on. Deliver through [`deliver_at_boundary`]. In an
///   `unbounded-ladder` build this replaces the Ziv entry: the fixed
///   rungs plus the adjudicator already decide everything in range,
///   so rung 3 is unnecessary for these operations (ADR-0060), while
///   the Ziv path stays wired for uniformity and for the out-of-range
///   remainder.
/// * `Near(b)` with `decide(b) = None` (operands outside the
///   adjudicable range), or `NearIndeterminate`: the pre-adjudicator
///   behavior per build — escalate to the Ziv rung under
///   `unbounded-ladder`; deliver unconditionally in the default
///   build. `ladder_audit`'s meaning for these operations sharpens to
///   "the adjudicator ran and DECLINED": an ambiguity the adjudicator
///   decides no longer panics, which is ADR-0060's "vacuous panics
///   removed by construction".
///
/// Escalation and adjudication stay deterministic functions of the
/// input alone: the predicate is mode-independent, the decider is
/// exact integer arithmetic on the operands and the boundary, and
/// the adjudicable-range gates read only the operands.
#[cfg(feature = "exp-log")]
pub(crate) fn round_adjudicated<F: DecimalFormat, E: ExtNum>(
    v: E,
    rm: RoundingMode,
    budget: &Budget,
    decide: impl Fn(Boundary) -> Option<crate::adjudicate::Side>,
) -> Option<(F, Status)> {
    if E::RUNG != 2 {
        return round_guarded::<F, E>(v, rm, budget);
    }
    #[cfg(force_rung3)]
    if E::ESCALATES {
        // The Ziv termination lane keeps its meaning: with the
        // unbounded feature both fixed rungs route past the
        // adjudicator into the dynamic rung. (Without the feature the
        // cfg is meaningless here, as it is in `round_guarded`.)
        return None;
    }
    // The `force_adjudicate` test lane replaces the budgeted verdict
    // with the unbudgeted nearest-boundary locate: EVERY rung 2
    // delivery of the five operations then routes through the
    // adjudicator (in range), and the pinned corpus is the byte
    // identity reference — the anti-rot differential ADR-0060's
    // inversion list demands, on the `force_escalate` lane's pattern.
    // Sound at any distance: the nearest boundary and the true value
    // share a bracket (no other boundary sits between a value and its
    // nearest boundary), so the adjudicated delivery still rounds
    // identically to the true value in every mode.
    #[cfg(force_adjudicate)]
    let verdict = v.nearest_boundary::<F>();
    #[cfg(not(force_adjudicate))]
    let verdict = v.candidate_boundary::<F>(v.rung_budget(budget));
    match verdict {
        BoundaryVerdict::Clear => {}
        BoundaryVerdict::Near(b) => match decide(b) {
            Some(side) => return Some(deliver_at_boundary::<F, E>(v, b, side, rm)),
            None => {
                if E::ESCALATES {
                    return None;
                }
                #[cfg(ladder_audit)]
                panic!(
                    "ladder_audit: rung 2 ambiguity the adjudicator \
                     declined (operands outside the adjudicable range)"
                );
            }
        },
        BoundaryVerdict::NearIndeterminate => {
            if E::ESCALATES {
                return None;
            }
            #[cfg(ladder_audit)]
            panic!("ladder_audit: rung 2 indeterminate boundary verdict");
        }
    }
    let (result, status) = v.to_format::<F>(0, rm);
    Some((result, status | Status::INEXACT))
}

/// The adjudicated delivery: the ADR-0051 residual channel anchored
/// AT the candidate boundary, on the exactly decided side.
///
/// Soundness, every mode at once. The predicate placed the working
/// value within `budget` units of `b`, and the rung's Tier 1 error
/// bound places the true value within another `budget` of the
/// working value, so `|y − b| ≤ 2·budget` working ulps — dozens of
/// decimal orders inside the gap to the adjacent boundary (half a
/// format quantum, ≥ 10^75 units at rung 2 width). The decider
/// proved `y` strictly on `side` of `b`. The channel's denoted open
/// interval (one working ulp beside `b`, forced sticky) therefore
/// lies strictly between the same two boundaries as `y`, on the same
/// side, and rounds identically to `y` at every rounding direction;
/// a midpoint anchor cannot re-tie because the sticky is forced. The
/// `| INEXACT` is unconditionally correct: `y` is off-boundary by
/// the classifier completeness the decider's `Equal` panic polices.
///
/// The five adjudicated kernels all deliver positive working
/// magnitudes (sign reflection happens in the kernels, after
/// delivery, under their `for_negation` rounding-mode rule), so the
/// anchor is constructed positive.
#[cfg(feature = "exp-log")]
fn deliver_at_boundary<F: DecimalFormat, E: ExtNum>(
    v: E,
    b: Boundary,
    side: crate::adjudicate::Side,
    rm: RoundingMode,
) -> (F, Status) {
    debug_assert!(!v.sign(), "adjudicated kernels deliver positive magnitudes");
    let anchor = v.from_parts_u128(b.coef, b.exp, false);
    let grows = side == crate::adjudicate::Side::Above;
    let (result, status) = anchor.to_format_with_residual::<F>(grows, rm);
    (result, status | Status::INEXACT)
}

/// Run the two-rung ladder for a kernel body: rung 1, and on
/// escalation the identical body at rung 2. The top rung's
/// [`round_guarded`] delivery is unconditional, so the second run
/// cannot itself report `None`.
///
/// A plain function (not a method) so the wrappers stay one
/// expression; the closures monomorphize per rung with zero dispatch.
#[cfg(not(feature = "unbounded-ladder"))]
pub(crate) fn run<F: DecimalFormat>(
    rung1: impl FnOnce() -> Option<(F, Status)>,
    rung2: impl FnOnce() -> Option<(F, Status)>,
) -> (F, Status) {
    match rung1() {
        Some(result) => result,
        None => rung2().expect("top rung delivers unconditionally"),
    }
}

/// The Ziv driver's starting precision: double rung 2's width, per the
/// plan of record ("doubling from 220 digits").
#[cfg(feature = "unbounded-ladder")]
const ZIV_START_PRECISION: u32 = 220;

/// Run the unbounded ladder for a kernel body: rung 1, rung 2 (which
/// escalates under this feature instead of delivering
/// unconditionally), then the Ziv loop — the identical body on the
/// dynamic rung at 220 digits, doubling until [`round_guarded`]'s
/// predicate clears at that width's `budget.dynamic(p)`.
///
/// Termination: the M7 input-side classifiers deliver every exact and
/// tie case before the ladder, so any true result reaching the loop is
/// irrational and sits at a finite distance from every rounding
/// boundary; `budget(p)` grows linearly in `p` while that distance,
/// measured in `10^-p` predicate units, grows as `10^p`, so the
/// predicate clears at some finite width. The constant generators cap
/// their depth at 100,000 digits, so a pathological non-clearing input
/// would panic loudly there rather than deliver a wrong rounding; the
/// widths reachable from 220 by doubling put that at `p = 57,344`
/// after eight escalations beyond rung 3's entry, each of which has
/// probability ~`budget/10^p` under the ADR-0059 model.
#[cfg(feature = "unbounded-ladder")]
pub(crate) fn run3<F: DecimalFormat>(
    rung1: impl FnOnce() -> Option<(F, Status)>,
    rung2: impl FnOnce() -> Option<(F, Status)>,
    attempt: impl Fn(u32) -> Option<(F, Status)>,
) -> (F, Status) {
    if let Some(result) = rung1() {
        return result;
    }
    if let Some(result) = rung2() {
        return result;
    }
    let mut p = ZIV_START_PRECISION;
    loop {
        if let Some(result) = attempt(p) {
            return result;
        }
        p = p
            .checked_mul(2)
            .expect("Ziv doubling overflowed the precision counter");
    }
}

/// The one wrapper macro every public kernel runs the ladder through.
///
/// The caller names the exemplar slot and writes the body call once;
/// the macro instantiates it per rung, filling the slot with that
/// rung's exemplar (`Extended::ZERO`, `Extended2::ZERO`, or a
/// fresh-per-attempt [`crate::extended_dyn::DynArena`] exemplar whose
/// arena is dropped when the attempt returns):
///
/// ```ignore
/// ladder::ladder_run!(|ex| sin_kernel_body::<F, _>(ex, x, rm))
/// ```
///
/// Without the `unbounded-ladder` feature the expansion is exactly the
/// pre-M8b two-closure [`run`] call — only a `let`-binding inside each
/// closure distinguishes it, so rungs 1 and 2 stay byte-identical.
#[cfg(not(feature = "unbounded-ladder"))]
macro_rules! ladder_run {
    (|$ex:ident| $body:expr) => {
        $crate::ladder::run(
            || {
                let $ex = $crate::extended::Extended::ZERO;
                $body
            },
            || {
                let $ex = $crate::extended2::Extended2::ZERO;
                $body
            },
        )
    };
}

/// The `unbounded-ladder` expansion of [`ladder_run!`]: the same two
/// fixed rungs, then the Ziv loop over per-attempt arenas.
#[cfg(feature = "unbounded-ladder")]
macro_rules! ladder_run {
    (|$ex:ident| $body:expr) => {
        $crate::ladder::run3(
            || {
                let $ex = $crate::extended::Extended::ZERO;
                $body
            },
            || {
                let $ex = $crate::extended2::Extended2::ZERO;
                $body
            },
            |p| {
                let arena = $crate::extended_dyn::DynArena::new(p);
                let $ex = arena.exemplar();
                $body
            },
        )
    };
}

pub(crate) use ladder_run;

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

// ----------------------------------------------------------------------------
// Dynamic-rung budget formulas (M8b): the catalog's itemizations
// re-evaluated at the runtime precision `p`, one `fn(u32) -> u128` per
// budget shape. Shared derivation facts:
//
// * Series items are `3 × cap(p)` with the precision-derived caps
//   (exp `p + 10`, sin/cos and sinh/cosh `2p + 20`, log1p `5p`, atan
//   `4p + 10`).
// * The constant items (reduction const-multiplies, amplification
//   factors) are precision-independent — the same numbers the rung 1
//   and rung 2 itemizations carry, because the amplification depends
//   on the argument ranges, not the working width.
// * Newton-seeded `div` / `recip` / `sqrt` charge a flat 60: the
//   derived step count is `⌈log2(2p / F::PRECISION)⌉ ≤ 15` for every
//   supported `p ≤ 10^5` even from the 7-digit Decimal32 seed, and
//   `3 × 15 + 3 < 60`.
// * The runtime trig reduction's items (documented at
//   `argred::reduce_dyn`) total under 2 units at any `p`: the π/2
//   constant `< 10^-(p+3)` relative, the window-plus-generator
//   truncation `≤ 2·10^-(p+36)` absolute in `x·2/π` (≤ 2·10^-2 units
//   after the worst 33-digit cancellation), the residual truncation
//   `< 10^-(p+4)` relative, and one final half-even rounding.
// * Every formula reproduces its rung 2 constant to within ±30% at
//   `p = 110` (`dynamic_budgets_track_the_rung2_catalog` pins the
//   ratio inside [1/5, 5]), which is the evidence the formulas are
//   the same model and not a second policy. The ×10 pad is inside
//   each formula.

/// [`EXP`] at `p`: reduction 22,201 + series `3(p + 10)`, ×10.
fn exp_budget_dyn(p: u32) -> u128 {
    10 * (22_300 + 3 * u128::from(p))
}
/// [`EXP2`] at `p`: [`exp_budget_dyn`]'s items + the argument
/// const-multiply 22,200, ×10.
fn exp2_budget_dyn(p: u32) -> u128 {
    10 * (44_500 + 3 * u128::from(p))
}
/// [`EXPM1`] at `p`: the reduction band's 32,700 constant items +
/// series `3(p + 10)` through the ≤ 1.47 closing amplification
/// (≈ 5p), ×10.
fn expm1_budget_dyn(p: u32) -> u128 {
    10 * (32_800 + 5 * u128::from(p))
}

/// [`EXP2M1`] / [`EXP10M1`] at `p`: the two const-item blocks
/// (argument multiply ≈ 33,000 + [`EXPM1`]'s reduction block
/// ≈ 32,800) + series `3(p + 10)` through the closing amplification
/// (≈ 5p), ×10.
fn expbasem1_budget_dyn(p: u32) -> u128 {
    10 * (65_800 + 5 * u128::from(p))
}
/// [`LN`] (and [`LOG10`] / [`LOG2`]) at `p`: decade path ~160 + log1p
/// series `3 · 5p` + closing ops, ×10. Reproduces the catalog's 950
/// at 50 and 1,850 at 110 before the pad.
fn ln_budget_dyn(p: u32) -> u128 {
    10 * (200 + 15 * u128::from(p))
}
/// [`SIN`] / [`COS`] at `p`: the runtime reduction's ≤ 2 units (see
/// the block comment above) + window survival ~1 + series
/// `3(2p + 20)` + recomposition ≤ 10, ×10. No 10^13-unit truncation
/// item here: the runtime reduction's π/2 depth follows `p`, which is
/// exactly what rung 1's fixed 38-digit constant could not do.
fn sin_budget_dyn(p: u32) -> u128 {
    10 * (6 * u128::from(p) + 80)
}
/// [`TAN`] at `p`: both components' items through the quotient +
/// Newton 60, ×10.
fn tan_budget_dyn(p: u32) -> u128 {
    10 * (12 * u128::from(p) + 175)
}
/// [`ATAN`] at `p`: Newton 60 + inner reduction 16 + series
/// `3(4p + 10)` + recomposition 10, ×10.
fn atan_budget_dyn(p: u32) -> u128 {
    10 * (12 * u128::from(p) + 120)
}
/// [`ASIN`] / [`ACOS`] at `p`: sqrt 60 + div 60 + the atan core +
/// doubling, ×10.
fn asin_budget_dyn(p: u32) -> u128 {
    10 * (12 * u128::from(p) + 240)
}
/// [`ATAN2`] at `p`: div 60 + the atan core + quadrant adjustment,
/// ×10.
fn atan2_budget_dyn(p: u32) -> u128 {
    10 * (12 * u128::from(p) + 180)
}
/// [`SINH`] at `p`: two exp cores through the `coth(0.5) ≈ 2.17`
/// cancellation + the small-band series, ×10.
fn sinh_budget_dyn(p: u32) -> u128 {
    10 * (96_600 + 14 * u128::from(p))
}
/// [`COSH`] at `p`: two exp cores, no cancellation, ×10.
fn cosh_budget_dyn(p: u32) -> u128 {
    10 * (44_500 + 6 * u128::from(p))
}
/// [`TANH`] at `p`: the sinh and cosh sums through the quotient +
/// Newton 60, ×10.
fn tanh_budget_dyn(p: u32) -> u128 {
    10 * (141_100 + 20 * u128::from(p))
}
/// [`ASINH`] at `p`: the log1p series `3 · 5p` + argument ops and
/// Newton charges, ×10.
fn asinh_budget_dyn(p: u32) -> u128 {
    10 * (15 * u128::from(p) + 450)
}
/// [`ACOSH`] at `p`: the direct band's ×7 amplification items + the
/// ln core, ×10.
fn acosh_budget_dyn(p: u32) -> u128 {
    10 * (15 * u128::from(p) + 2_700)
}
/// [`ATANH`] at `p`: the ratio band's ops + the ln core, ×10.
fn atanh_budget_dyn(p: u32) -> u128 {
    10 * (15 * u128::from(p) + 400)
}
/// [`HYPOT`] at `p`: precision independent. Two squares + one add
/// enter the square root, which halves their relative error, and the
/// Newton-seeded `sqrt` charges the flat 60 the module doc fixes for
/// every supported `p`; the scaling shifts are exact. `(60 + 25) × 10`.
fn hypot_budget_dyn(_p: u32) -> u128 {
    850
}
/// [`CBRT`] at `p`: the ln sum amplified through `|ln x|/3 ≤ 4,717`,
/// plus the exp series, ×10. Reproduces the catalog's 4.6e6 at 50
/// and 8.7e6 at 110 before the pad.
fn cbrt_budget_dyn(p: u32) -> u128 {
    10 * ((200 + 15 * u128::from(p)) * 4_717 + 3 * u128::from(p) + 31)
}
/// [`POW`] at `p`: the ln sum + product rounding amplified through
/// `|y·ln x| ≤ 14,151` + the exp series, ×10. Reproduces the
/// catalog's 1.35e7 at 50 and 2.62e7 at 110 before the pad.
fn pow_budget_dyn(p: u32) -> u128 {
    10 * ((201 + 15 * u128::from(p)) * 14_151 + 3 * (u128::from(p) + 10))
}
/// [`RSQRT`] at `p`: two flat-60 Newton charges + the polish step and
/// glue, ×10. Flat in `p` because the kernel runs no series, so no
/// term scales with the working width (the dynamic rung's `sqrt` and
/// `recip` derive their own step counts from `p`, which is what the
/// flat 60 already prices).
fn rsqrt_budget_dyn(_p: u32) -> u128 {
    1_250
}
/// [`POWI_INT`] at `p`: precision-independent by construction — the
/// powering arm's item count depends on `|n| ≤ 6`, not on the working
/// width. Newton-seeded `recip` charges the module's flat 60, the ≤ 5
/// working-precision multiplies charge 1 each through the ≤ 6×
/// exponent amplification, and glue rounds the sum up: `(60 + 5·2 + 5)`
/// ≈ 75, ×10 → 750.
fn powi_int_budget_dyn(_p: u32) -> u128 {
    750
}

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
    dynamic: exp_budget_dyn,
};

/// `exp2 = exp(x·ln2)`. Itemization: the argument const-multiply
/// `x·ln2` (magnitude ≤ 14151 by the overflow gate) adds another
/// ≤ 22,200 units of absolute-in-argument error on top of [`EXP`]'s
/// items. Sum ≈ 44,600; ×10 → 500,000. Same shape both rungs.
pub(crate) const EXP2: Budget = Budget {
    rung1: 500_000,
    rung2: 500_000,
    dynamic: exp2_budget_dyn,
};

/// `expm1 = e^x − 1` (IEEE 754-2019 §9.2 `expm1`; public `exp_m1`).
/// Itemization (rung 1), two disjoint branches, budget = the max:
///
/// * Direct band (`|x| ≤ 1.1513`, the reduction's own k = 0 window):
///   the all-positive series for positive `x`; for negative `x` the
///   alternating terms cancel by at most `e^{|x|} ≤ 3.17` at the band
///   edge, so the series charge is `3 × cap × 3.17 ≈ 600` units,
///   relative to the result by the series' construction.
/// * Reduction band (`|x| > 1.1513`): [`EXP`]'s items (reduction
///   ≤ 22,201, series ≤ 180) amplified through the closing
///   subtraction by `e^x/(e^x − 1) ≤ 1.47` at the band edge (and ≤ 1
///   on the negative side, where the subtraction adds magnitudes),
///   plus 1 unit for the subtraction itself: ≤ ~33,000.
///
/// Sum ≈ 33,000; ×10 → 350,000. Rung 2: identical structure, cap 120
/// series → ≈ 33,400; ×10 → 350,000. Dynamic: the same itemization at
/// `p` (series `3(p + 10)` with the ×3.17 band factor folded into the
/// constant term), ×10 inside the formula.
pub(crate) const EXPM1: Budget = Budget {
    rung1: 350_000,
    rung2: 350_000,
    dynamic: expm1_budget_dyn,
};

/// `exp2m1 = expm1(x · ln 2)` (IEEE 754-2019 §9.2 `exp2m1`; public
/// `exp2_m1`): the argument const-multiply (≤ 22,200 units of the
/// ≤ 14,151 argument magnitude, mapped 1:1 into result-relative
/// units through `d(e^u − 1) = e^u du` and the ≤ 1.47 closing
/// factor: ≤ ~33,000) on top of [`EXPM1`]'s items (≤ ~33,000).
/// Sum ≈ 66,000; ×10 → 700,000, both rungs.
pub(crate) const EXP2M1: Budget = Budget {
    rung1: 700_000,
    rung2: 700_000,
    dynamic: expbasem1_budget_dyn,
};

/// `exp10 = exp(x · ln 10)` (IEEE 754-2019 §9.2 `exp10`): the
/// argument const-multiply adds ≤ 22,200 units of absolute-in-
/// argument error on top of [`EXP`]'s items, the identical
/// composition shape as [`EXP2`] (the ≤ 14,151 argument bound is the
/// same overflow gate). Sum ≈ 44,600; ×10 → 500,000, both rungs;
/// dynamic shares [`exp2_budget_dyn`].
pub(crate) const EXP10: Budget = Budget {
    rung1: 500_000,
    rung2: 500_000,
    dynamic: exp2_budget_dyn,
};

/// `exp10m1 = expm1(x · ln 10)` (IEEE 754-2019 §9.2 `exp10m1`;
/// public `exp10_m1`): the argument const-multiply through the
/// closing amplification (≤ ~33,000) on top of [`EXPM1`]'s items
/// (≤ ~33,000), the same composition shape as [`EXP2M1`], whose
/// dynamic formula it shares. Sum ≈ 66,000; ×10 → 700,000, both
/// rungs.
pub(crate) const EXP10M1: Budget = Budget {
    rung1: 700_000,
    rung2: 700_000,
    dynamic: expbasem1_budget_dyn,
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
    dynamic: ln_budget_dyn,
};

/// `log10 = ln(x) · (1/ln10)`: [`LN`] plus one const-multiply on the
/// *result* (relative, ≤ 1.5 units). Same constants as [`LN`] after
/// the pad absorbs it.
pub(crate) const LOG10: Budget = Budget {
    rung1: 15_000,
    rung2: 25_000,
    dynamic: ln_budget_dyn,
};

/// `log2 = ln(x) · (1/ln2)`: as [`LOG10`].
pub(crate) const LOG2: Budget = Budget {
    rung1: 15_000,
    rung2: 25_000,
    dynamic: ln_budget_dyn,
};

/// `logp1 = ln(1 + x)` (IEEE 754-2019 §9.2 `logp1`; public `ln_1p`).
/// Itemization (rung 1):
///
/// * Direct band (`|x| < 0.5`): `u = x` is exact at every rung width
///   (`from_format` is exact), `log1p` series at cap 250 → ≤ 750
///   units, relative to the result by the series' construction (the
///   fd-aqs.6 relative accuracy argument, inherited verbatim from
///   [`LN`]'s near-1 path). Results that collapse onto the input's
///   own grid point are decided by the ADR-0051 anchor seam before
///   the predicate (`ln(1+x) < x` strictly; see the kernel), so the
///   band's guard only ever prices series noise.
/// * Wide band (`|x| ≥ 0.5`): the argument `t = 1 ⊕ x` is exact on
///   the whole negative side (≤ 35 aligned digits) and on the
///   positive side until `x` outgrows the rung width (`≳ 10^49` at
///   rung 1), where absorbing the 1 costs ≤ 1 unit of `t`, mapped by
///   `d(ln t)/dt = 1/t` into ≤ 3 result units against the band's
///   `|ln t| ≥ 0.405` floor; then [`LN`]'s decade items (≤ ~950)
///   apply to `t` verbatim.
///
/// Sum ≤ ~950; ×10 → 15,000: [`LN`]'s constants inherited, the one
/// extra op absorbed by the pad. Rung 2: cap 550 series → ≈ 1,850;
/// ×10 → 25,000. Dynamic: [`ln_budget_dyn`], the same itemization
/// re-evaluated at `p`.
pub(crate) const LOGP1: Budget = Budget {
    rung1: 15_000,
    rung2: 25_000,
    dynamic: ln_budget_dyn,
};

/// `log2p1 = logp1(x) · (1/ln 2)` (IEEE 754-2019 §9.2 `log2p1`;
/// public `log2_1p`): [`LOGP1`]'s items plus one const-multiply on
/// the result (relative, ≤ 1.5 units), the same composition shape as
/// [`LOG2`] over [`LN`]. Same constants; the pad absorbs the
/// multiply. Tiny inputs deliver `x · (1/ln 2)`, a generic working
/// value with no grid anchor (slope ≠ 1), so unlike [`LOGP1`] no
/// anchor seam precedes the guard.
pub(crate) const LOG2P1: Budget = Budget {
    rung1: 15_000,
    rung2: 25_000,
    dynamic: ln_budget_dyn,
};

/// `log10p1 = logp1(x) · (1/ln 10)` (IEEE 754-2019 §9.2 `log10p1`;
/// public `log10_1p`): [`LOGP1`]'s items plus one const-multiply on
/// the result (relative, ≤ 1.5 units), the same composition shape as
/// [`LOG10`] over [`LN`]. Same constants; the pad absorbs the
/// multiply. Tiny inputs deliver `x · (1/ln 10)`, a generic working
/// value with no grid anchor (slope ≠ 1), so unlike [`LOGP1`] no
/// anchor seam precedes the guard.
pub(crate) const LOG10P1: Budget = Budget {
    rung1: 15_000,
    rung2: 25_000,
    dynamic: ln_budget_dyn,
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
    dynamic: sin_budget_dyn,
};

/// `cos`: identical pipeline to [`SIN`]; the reduced-range map factor
/// is `|y·tan y| ≤ π²/16 < 1`.
pub(crate) const COS: Budget = Budget {
    rung1: 150_000_000_000_000,
    rung2: 10_000,
    dynamic: sin_budget_dyn,
};

/// `tan = sin/cos` on the shared reduction: both operands' relative
/// errors add through the quotient (≤ 2 × the [`SIN`] sum) plus a
/// Newton division (15). ×10 → 3e14 / 25,000.
pub(crate) const TAN: Budget = Budget {
    rung1: 300_000_000_000_000,
    rung2: 25_000,
    dynamic: tan_budget_dyn,
};

/// `atan`. Itemization (rung 1): outer `recip` inversion ≤ 15, inner
/// `tan(π/8)` reduction (sub + div) ≤ 16, Taylor at cap 200 → ≤ 600,
/// `π/2 − result` recomposition against a result ≥ π/8 in that branch
/// ≤ 10. Sum ≈ 640; ×10 → 10,000. Rung 2: cap 450 → ≈ 1,400; ×10 →
/// 20,000.
pub(crate) const ATAN: Budget = Budget {
    rung1: 10_000,
    rung2: 20_000,
    dynamic: atan_budget_dyn,
};

/// `asin = 2·atan(x / (1 + sqrt((1−|x|)(1+|x|))))` (fd-aqs.6, exact
/// factors for format-sourced `x`): sqrt 15 + div 15 + the [`ATAN`]
/// core ≈ 640 + doubling 1. Sum ≈ 680; ×10 → 10,000 / rung 2 20,000.
pub(crate) const ASIN: Budget = Budget {
    rung1: 10_000,
    rung2: 20_000,
    dynamic: asin_budget_dyn,
};

/// `acos = 2·atan(sqrt((1−x)/(1+x)))` (fd-aqs.6, exact factors):
/// div 15 + sqrt 15 + [`ATAN`] core ≈ 640 + doubling 1; the large-`t`
/// branch's `π/2 − atan(1/t)` recomposition is relative-safe against
/// its ≥ π/2 result. Sum ≈ 680; ×10 → 10,000 / 20,000.
pub(crate) const ACOS: Budget = Budget {
    rung1: 10_000,
    rung2: 20_000,
    dynamic: asin_budget_dyn,
};

/// `atan2`: quotient `y/x` (15) + [`ATAN`] core (≈ 640) + quadrant
/// `±π` adjustment (absolute ≤ 1.5 units of π against a result
/// magnitude ≥ π/2 in the adjusted quadrants → ≤ 4). Sum ≈ 660;
/// ×10 → 10,000 / 20,000. The exact-axis and constant deliveries
/// bypass the guard.
pub(crate) const ATAN2: Budget = Budget {
    rung1: 10_000,
    rung2: 20_000,
    dynamic: atan2_budget_dyn,
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
    dynamic: sinh_budget_dyn,
};

/// `cosh = (e^x + e^{-x})/2`: two [`EXP`]-core runs through an
/// *addition* (no cancellation, factor ≤ 1) → ≤ 45,000; small band
/// Taylor ≤ 360. ×10 → 500,000. Both rungs.
pub(crate) const COSH: Budget = Budget {
    rung1: 500_000,
    rung2: 500_000,
    dynamic: cosh_budget_dyn,
};

/// `tanh = sinh/cosh` below the nines-saturation band: the two cores'
/// relative errors add through the quotient (≤ [`SINH`]'s 98,000 +
/// [`COSH`]'s 45,000) + division 15. ×10 → 1,500,000. Both rungs.
pub(crate) const TANH: Budget = Budget {
    rung1: 1_500_000,
    rung2: 1_500_000,
    dynamic: tanh_budget_dyn,
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
    dynamic: asinh_budget_dyn,
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
    dynamic: acosh_budget_dyn,
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
    dynamic: atanh_budget_dyn,
};

/// `hypot = sqrt(x² + y²)` (IEEE 754-2019 §9.2 `hypot`), on the
/// scaled operands `w̃ = |w| · 10^(−adj w)` and `z̃ = |z| · 10^(−adj w)`
/// (the scaling is a pure exponent shift, so it contributes nothing).
/// Itemization, identical on both fixed rungs because every item is
/// precision independent:
///
/// * two squares, 1 unit each, and the add, 1 unit — the summands are
///   both positive, so the add cancels nothing and its relative error
///   is the max of the two inputs' plus its own rounding: ≤ 3 units of
///   relative error in `S̃`.
/// * the square root halves that: `d(√S)/√S = ½ · dS/S`, so the three
///   units above enter the result as ≤ 1.5.
/// * the Newton-seeded `sqrt` itself charges 15 (module doc), and the
///   closing scale-back and delivery glue ≤ 3.
///
/// Sum ≈ 20; ×10 → 250, both rungs. Dynamic: the same itemization with
/// `sqrt`'s flat 60, ×10 → 850 at every `p`
/// ([`hypot_budget_dyn`]).
///
/// The anchor band's residual delivery and the input-side exact and
/// tie classification both bypass this budget entirely: they run
/// before the guard and are correct by construction, not by margin.
pub(crate) const HYPOT: Budget = Budget {
    rung1: 250,
    rung2: 250,
    dynamic: hypot_budget_dyn,
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
    dynamic: cbrt_budget_dyn,
};

/// `rootn = exp(ln|x| / |n|)` (IEEE 754-2019 §9.2 `rootn`), the
/// general path for `|n| ≥ 3` (`|n| ≤ 2` delegates: identity,
/// division, the format's square root, and the `rsqrt` kernel).
/// Itemization (rung 1), [`CBRT`]'s with the fixed 3 replaced by
/// `|n|`: `ln`'s relative error (≤ 950 units, ≈ 9.5e-48) becomes
/// absolute through `|ln x| ≤ 14151` and is then divided by `|n|`,
/// so the amplification `|ln x| / |n| ≤ 14151/3 = 4717` at `|n| ≥ 3`
/// gives ≤ `4.5e-44` absolute in the exp argument after the
/// `div_u32` (+1 unit), i.e. ≤ 4.5e6 result-relative units, plus the
/// [`EXP`] Taylor ≤ 180. Sum ≈ 4.6e6; ×10 → 5e7. Rung 2: `ln`
/// ≤ 1,850 → ≤ 8.8e6; ×10 → 1e8. Both constants are [`CBRT`]'s,
/// which is the point: `rootn(x, 3)` and `cbrt(x)` are the same
/// computation. (`n = −2` never reaches this budget: it delegates to
/// the `rsqrt` kernel and [`RSQRT`], whose Newton composition is what
/// lets that operand carry ADR-0060's unconditional claim.)
pub(crate) const ROOTN: Budget = Budget {
    rung1: 50_000_000,
    rung2: 100_000_000,
    // Identical shape to `cbrt`'s: same items, same `|ln x| ≤ 14151`
    // amplification through a divisor of at least 2, same exp series.
    // Shared rather than copied so the two cannot drift apart.
    dynamic: cbrt_budget_dyn,
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
    dynamic: pow_budget_dyn,
};

/// `rsqrt = 1/sqrt(x)` by Newton composition (IEEE 754-2019 §9.2
/// `rSqrt`). ADR-0060 forces the architecture: the `exp(−½·ln x)`
/// route carries the `|ln x| ≤ 14151` amplification, so its budget
/// lands at the [`CBRT`]/[`POW`] `1e8` scale and its resulting
/// threshold `~10^-101` cannot clear the operation's proven
/// `4.9·10^-105` Liouville floor. The Newton kernel's can, and this
/// constant is what the ADR's unconditional two rung verdict is stated
/// against (`B₂ ≤ 10³`, target `≤ 500`).
///
/// Itemization (both rungs, identical because nothing here scales with
/// the working width):
///
/// * `sqrt` Newton-seeded 15 + `recip` Newton-seeded 15. Both charges
///   are the module doc's flat Newton price, and both are *erased*
///   rather than accumulated by the item below: the composition's
///   residual enters the polish step as `e` and leaves as `1.5·e²`.
/// * One division-free Newton polish `y ← y·(3 − x·y²)/2`: 5 working
///   roundings (square, multiply, subtract, multiply, halve), ≤ 5
///   units. This is the item that makes the two above honest at the
///   narrow formats — see [`crate::rsqrt`]'s "Why the composition is
///   polished" for the seed-width derivation and the per-format table.
/// * Closing glue ≤ 3.
///
/// Sum ≤ 38 charging the composition as if the polish did not square
/// it away (≤ 8 if it is credited); ×10 → 400, both rungs. Dynamic:
/// [`rsqrt_budget_dyn`], the same items with the Newton charges at the
/// module doc's flat 60.
pub(crate) const RSQRT: Budget = Budget {
    rung1: 400,
    rung2: 400,
    dynamic: rsqrt_budget_dyn,
};

/// `powi(x, n) = x^n` on the binary-powering arm (`|n| ≤ 6`, IEEE
/// 754-2019 §9.2 `pown`). Itemization (rung 1), all at *working*
/// precision — the format-precision `int_pow` of `pow`'s fast path is
/// a different routine with a different (H1-documented) error story:
///
/// * `from_format` of the base is exact (0 units); square-and-multiply
///   for `|n| ≤ 6` executes at most 2 squarings and at most 2 rounding
///   multiplies (the itemization budgets ≤ 3 and ≤ 2). A squaring
///   doubles the accumulated relative error and adds its own rounding:
///   the worst chain (`x → x² → x⁴`, then `x²·x⁴`) carries
///   `1 → 3 → 5` units.
/// * `n < 0` closes with a Newton-seeded `recip`: 15 units, plus the
///   ≤ 5 units it inherits, ≈ 20. The operand is scaled into
///   `[1, 10)` before the inversion and shifted back after (`pow`'s
///   `working_reciprocal`, which keeps the Newton seed's format round
///   trip in range); both steps are exact exponent arithmetic and
///   charge nothing.
///
/// Sum ≈ 20; ×10 pad → 200, both rungs (the item count depends on
/// `|n|`, never on the working width). Two consequences worth stating
/// where the constant lives. First, this arm is what makes ADR-0060's
/// unconditional tier claim reachable: the Liouville floor for
/// `pown` at small `|n|` (`10^−(34|n|+2)` positive, `10^−(34|n|+36)`
/// negative) is compared against `10^−109 · B₂`, and a budget of 200
/// leaves `109 − log₁₀ 200 ≈ 106.7` digits of resolution — which
/// clears the floor exactly on the operand ranges ADR-0060 tabulates
/// (`n ∈ {−2, 2, 3}` bare; the landed exact integer adjudicator
/// extends the unconditional claim to the full `−6 ≤ n ≤ 6` on this
/// arm's delivery). The `exp(n·ln|x|)` route's `~10^8` budget
/// could not: routing small `|n|` through it would cost six decimal
/// orders of resolution and put every one of those ranges out of
/// reach. Second, the arm carries no over/underflow gate of its own —
/// it does not need one, because `|n| ≤ 6` bounds the working result's
/// decimal exponent by `6 · 6176` plus the working width (≈ 37,100,
/// inside the envelope `exp10_integer` already exercises) and the
/// format rounder's §7.4 disposition is correct at any exponent in
/// it, while every value that sits exactly ON the grid out there is
/// disposed of input side by `exact::powi_exact_input`.
pub(crate) const POWI_INT: Budget = Budget {
    rung1: 200,
    rung2: 200,
    dynamic: powi_int_budget_dyn,
};

/// `powi(x, n) = exp(n·ln|x|)` on the large-exponent arm (`|n| ≥ 7`).
/// Itemization: [`POW`]'s verbatim — `ln`'s relative error plus the
/// product rounding become absolute through the *same* `|n·ln x| ≤
/// 14,151` overflow gate that bounds `|y·ln x|` there (the gate is a
/// property of `exp`'s convergence window, not of how the argument
/// was formed), and the [`EXP`] Taylor closes it. The only structural
/// difference from `pow` is that the multiplier is an exact integer
/// rather than a format value, which removes an error source rather
/// than adding one. Same constants, same dynamic formula.
pub(crate) const POWI: Budget = Budget {
    rung1: 150_000_000,
    rung2: 300_000_000,
    dynamic: pow_budget_dyn,
};

/// `compound(x, n) = (1 + x)^n = exp(n · log1p(x))` (IEEE 754-2019 §9.2
/// `compound`). Itemization (rung 1), the ADR-0060 derivation
/// transcribed:
///
/// * `logp1`'s relative error (≤ 950 units, ≈ 9.5e-48 — [`LOGP1`]'s
///   itemization inherited verbatim, both bands) plus 1 unit for the
///   `n · log1p(x)` product rounding, made *absolute* through
///   `|n · log1p(x)| ≤ 14,151` (the shared `exp` overflow gate, the same
///   bound [`POW`] amplifies through) → ≤ `1.35e-43` absolute in the
///   exp argument, i.e. ≤ 1.35e7 result-relative units.
/// * [`EXP`]'s Taylor series ≤ 180 units.
/// * The wide-band absorption item, `compound`'s one item [`POW`] does
///   not have: past the rung width `logp1`'s `t = 1 ⊕ x` absorbs the 1,
///   costing ≤ 1 unit of `t`, which `d(ln t)/dt = 1/t` maps into
///   ≤ 1 unit *absolute* in `log1p(x)` and the multiply then scales by
///   `|n|` → ≤ `|n|` result-relative units. The gate bounds that: the
///   absorption only happens for `x ≳ 10^49`, which forces
///   `log1p(x) ≥ 113`, and `|n · log1p(x)| ≤ 14,151` then caps
///   `|n| ≤ 14,151 / 113 ≈ 125` → ≤ ~130 units. Three decades below the
///   dominant item, and bounded rather than merely small.
///
/// Sum ≈ 1.4e7; ×10 → 1.5e8. Rung 2: `logp1` ≤ 1,850 → ≤ 2.6e7;
/// ×10 → 3e8. The composition shape is identical to [`POW`]'s (one
/// logarithm's relative error amplified through the same argument
/// bound into the same exponential), so the two catalogs agree
/// numerically and `compound` shares [`pow_budget_dyn`] rather than
/// carrying a second formula for the same model.
pub(crate) const COMPOUND: Budget = Budget {
    rung1: 150_000_000,
    rung2: 300_000_000,
    dynamic: pow_budget_dyn,
};

/// `powr = exp(y·ln x)` (IEEE 754-2019 §9.2 `powr`): the §9.2.1 `powr`
/// special-value table over [`POW`]'s rule-8 pipeline. Itemization by
/// reference to [`POW`], because the priced pipeline is the identical
/// one — the same `ln` relative error, the same product rounding, the
/// same `|y·ln x| ≤ 14151` overflow gate carrying both into absolute
/// error, the same [`EXP`] Taylor — and `powr`'s narrower domain
/// (`x > 0` strictly, no negative-base sign reflection) removes inputs
/// rather than adding error terms. Hence identical constants on both
/// rungs and the shared [`pow_budget_dyn`] formula.
///
/// The constant is duplicated rather than aliased to [`POW`] for
/// per-function containment: a budget is the auditable premise of its
/// own function's Tier 1 claim (ADR-0059), so a future revision that
/// turns out unsound reopens the correctness exposure on `powr` alone
/// instead of silently moving `pow` with it.
pub(crate) const POWR: Budget = Budget {
    rung1: 150_000_000,
    rung2: 300_000_000,
    dynamic: pow_budget_dyn,
};

/// `sinPi` / `cosPi` (IEEE 754-2019 §9.2; ADR-0061), one shared
/// kernel: exact decimal reduction of `x` to a fractional part
/// `δ ∈ [−1/4, 1/4]` revolutions with a branch choice (every step is
/// decimal add/subtract of exact quarters on the operand's own
/// digits: 0 units — the item trig's budget is DOMINATED by simply
/// does not exist here), then `sin(πδ)` or `cos(πδ)` by Taylor.
/// Itemization (rung 1):
///
/// * `πδ` const-multiply: ≤ 1.5 units of its own magnitude ≤ π/4,
///   mapped into result-relative units by `|y·cot y| ≤ 1` (sin arm)
///   or `|y·tan y| ≤ π²/16 < 1` (cos arm): ≤ 2.
/// * Taylor at cap 120 → ≤ 360; branch recomposition (sign only,
///   exact) ≤ 1.
///
/// Sum ≈ 365; ×10 → 4,000 (rounded up). Rung 2: cap 240 → ≈ 725;
/// ×10 → 10,000. The escalation rate lands near `8×10^-13` per call
/// where trig's is 3%: the payoff of the exact reduction, priced.
pub(crate) const SINPI: Budget = Budget {
    rung1: 4_000,
    rung2: 10_000,
    dynamic: sinpi_budget_dyn,
};

/// [`SINPI`] at `p`: the same items with the series at `2p + 20`.
fn sinpi_budget_dyn(p: u32) -> u128 {
    10 * (6 * u128::from(p) + 70)
}

/// `tanPi = sinPi/cosPi` on the shared reduction: both components'
/// relative errors add through the quotient (≤ 2 × [`SINPI`]'s sum)
/// plus a Newton division (15). ×10 → 8,000 / 25,000.
pub(crate) const TANPI: Budget = Budget {
    rung1: 8_000,
    rung2: 25_000,
    dynamic: tanpi_budget_dyn,
};

/// [`TANPI`] at `p`: both series plus the flat Newton 60.
fn tanpi_budget_dyn(p: u32) -> u128 {
    10 * (12 * u128::from(p) + 200)
}

/// `asinPi = asin(x)/π` (ADR-0061): [`ASIN`]'s items (sqrt 15 +
/// div 15 + atan core ≈ 640 + doubling 1) plus the closing `1/π`
/// const-multiply on the result (relative, ≤ 1.5). Sum ≈ 680;
/// ×10 → 10,000 / rung 2 20,000 — [`ASIN`]'s constants, the pad
/// absorbing the extra multiply, and the shared dynamic formula.
pub(crate) const ASINPI: Budget = Budget {
    rung1: 10_000,
    rung2: 20_000,
    dynamic: asin_budget_dyn,
};

/// `acosPi = acos(x)/π`: as [`ASINPI`] over [`ACOS`]'s identical
/// itemization.
pub(crate) const ACOSPI: Budget = Budget {
    rung1: 10_000,
    rung2: 20_000,
    dynamic: asin_budget_dyn,
};

/// `atanPi = atan(x)/π`: [`ATAN`]'s ≈ 640 plus the `1/π`
/// const-multiply, pad-absorbed. Same constants, same dynamic.
pub(crate) const ATANPI: Budget = Budget {
    rung1: 10_000,
    rung2: 20_000,
    dynamic: atan_budget_dyn,
};

/// `atan2Pi = atan2(y, x)/π`: [`ATAN2`]'s ≈ 660 plus the `1/π`
/// const-multiply, pad-absorbed. Same constants, same dynamic.
pub(crate) const ATAN2PI: Budget = Budget {
    rung1: 10_000,
    rung2: 20_000,
    dynamic: atan2_budget_dyn,
};

// Escalation-rate summary (Decimal128, the widest exposure; rate ≈
// 2 × rung1 × 10^-16 for a random input): trig ≈ 3%, tan ≈ 6%,
// pow and powi's large-|n| arm ≈ 3e-8, cbrt ≈ 1e-8, exp family
// ≈ 1e-10, powi's powering arm ≈ 4e-14 (the narrowest budget in the
// catalog), everything else ≤ 3e-12.
// Decimal64 rates are 10^18× smaller, Decimal32 smaller
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
        let all: [(&str, &Budget); 40] = [
            ("exp", &EXP),
            ("exp2", &EXP2),
            ("expm1", &EXPM1),
            ("exp2m1", &EXP2M1),
            ("exp10", &EXP10),
            ("exp10m1", &EXP10M1),
            ("ln", &LN),
            ("log10", &LOG10),
            ("log2", &LOG2),
            ("logp1", &LOGP1),
            ("log2p1", &LOG2P1),
            ("log10p1", &LOG10P1),
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
            ("rootn", &ROOTN),
            ("pow", &POW),
            ("rsqrt", &RSQRT),
            ("powi_int", &POWI_INT),
            ("powi", &POWI),
            ("compound", &COMPOUND),
            ("powr", &POWR),
            ("hypot", &HYPOT),
            ("sinpi", &SINPI),
            ("tanpi", &TANPI),
            ("asinpi", &ASINPI),
            ("acospi", &ACOSPI),
            ("atanpi", &ATANPI),
            ("atan2pi", &ATAN2PI),
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

    /// The rung hooks pick their own side of the pair, and the
    /// escalation flags encode the build's ladder shape: rung 1 always
    /// escalates, rung 2 only when the unbounded rung exists above it.
    #[test]
    fn rung_budget_selects_by_rung() {
        let b = Budget {
            rung1: 7,
            rung2: 11,
            dynamic: |p| u128::from(p) + 13,
        };
        assert_eq!(Extended::ZERO.rung_budget(&b), 7);
        assert_eq!(Extended2::ZERO.rung_budget(&b), 11);
        assert_eq!((b.dynamic)(220), 233);
        assert!(<Extended as ExtNum>::ESCALATES);
        assert_eq!(
            <Extended2 as ExtNum>::ESCALATES,
            cfg!(feature = "unbounded-ladder")
        );
        assert_eq!(<Extended as ExtNum>::RUNG, 1);
        assert_eq!(<Extended2 as ExtNum>::RUNG, 2);
    }

    /// The Ziv driver's control flow, exercised with synthetic
    /// closures: entry at 220 digits, doubling on `None`, delivery at
    /// the first clearing width. No corpus input can reach the
    /// doubling arm honestly (entering rung 3 at all is a ~1e-36
    /// event, doubling within it rarer still), so this is the loop's
    /// only executable witness.
    #[cfg(feature = "unbounded-ladder")]
    #[test]
    fn run3_doubles_until_the_attempt_clears() {
        use crate::mock_format::ValueFmt128;
        use std::vec::Vec;

        let widths = core::cell::RefCell::new(Vec::new());
        let (result, status) = run3::<ValueFmt128>(
            || None,
            || None,
            |p| {
                widths.borrow_mut().push(p);
                if p >= 880 {
                    Some((
                        ValueFmt128 {
                            coef: u128::from(p),
                            exp: 0,
                            sign: false,
                        },
                        Status::INEXACT,
                    ))
                } else {
                    None
                }
            },
        );
        assert_eq!(*widths.borrow(), alloc::vec![220, 440, 880]);
        assert_eq!(result.coef, 880);
        assert_eq!(status, Status::INEXACT);

        // Rungs 1 and 2 still short-circuit the loop entirely.
        let hit = core::cell::Cell::new(false);
        let (r1, _) = run3::<ValueFmt128>(
            || {
                Some((
                    ValueFmt128 {
                        coef: 1,
                        exp: 0,
                        sign: false,
                    },
                    Status::OK,
                ))
            },
            || unreachable!("rung 1 delivered"),
            |_| {
                hit.set(true);
                None
            },
        );
        assert_eq!(r1.coef, 1);
        assert!(!hit.get(), "the Ziv loop must not run when rung 1 delivers");
    }

    /// The dynamic formulas are the rung 2 itemizations re-evaluated,
    /// not a second policy: at `p = 110` every one lands within a
    /// factor of five of its rung 2 constant (observed: within ±30%).
    /// A formula drifting outside that band means the model and the
    /// catalog have diverged and one of them is wrong.
    #[test]
    fn dynamic_budgets_track_the_rung2_catalog() {
        let all: [(&str, &Budget); 40] = [
            ("exp", &EXP),
            ("exp2", &EXP2),
            ("expm1", &EXPM1),
            ("exp2m1", &EXP2M1),
            ("exp10", &EXP10),
            ("exp10m1", &EXP10M1),
            ("ln", &LN),
            ("log10", &LOG10),
            ("log2", &LOG2),
            ("logp1", &LOGP1),
            ("log2p1", &LOG2P1),
            ("log10p1", &LOG10P1),
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
            ("rootn", &ROOTN),
            ("pow", &POW),
            ("rsqrt", &RSQRT),
            ("powi_int", &POWI_INT),
            ("powi", &POWI),
            ("compound", &COMPOUND),
            ("powr", &POWR),
            ("hypot", &HYPOT),
            ("sinpi", &SINPI),
            ("tanpi", &TANPI),
            ("asinpi", &ASINPI),
            ("acospi", &ACOSPI),
            ("atanpi", &ATANPI),
            ("atan2pi", &ATAN2PI),
        ];
        for (name, b) in all {
            let at_110 = (b.dynamic)(110);
            assert!(
                at_110 >= b.rung2 / 5 && at_110 <= b.rung2 * 5,
                "{name}: dynamic(110) = {at_110} outside [rung2/5, 5*rung2] \
                 (rung2 = {})",
                b.rung2
            );
            // Monotone in p, and sane at the Ziv start width.
            assert!((b.dynamic)(220) >= at_110, "{name}: not monotone");
            assert!(
                (b.dynamic)(220) < 10u128.pow(12),
                "{name}: dynamic(220) implausibly wide"
            );
        }
    }
}
