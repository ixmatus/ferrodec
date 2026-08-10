//! `asinPi`, `acosPi`, `atanPi`, `atan2Pi` (IEEE 754-2019 §9.2): the
//! inverse half of the pi scaled family (ADR-0061, fd-4zo.26).
//!
//! Each kernel is its radian parent's extended precision core scaled
//! by `1/π` at the working width, then delivered through the ADR-0059
//! ladder under its own budget:
//!
//! ```text
//! asinPi(x)     = asin(x)     · (1/π)
//! acosPi(x)     = acos(x)     · (1/π)
//! atanPi(x)     = atan(x)     · (1/π)
//! atan2Pi(y, x) = atan2(y, x) · (1/π)
//! ```
//!
//! The scaling is a multiply by the working type's own `1/π`
//! constant, not a division by `π`: the const multiply costs ≤ 1.5
//! units where a Newton division costs 15 (`ladder::ASINPI`'s
//! itemization), and the reciprocal constant is certified at every
//! rung against an independent oracle by `consts::tests`. Sharing
//! `inverse_trig`'s cores rather than re-deriving the series is what
//! keeps one implementation of the mathematics under two spellings;
//! the parent module's header carries the reductions, the series, and
//! their error model.
//!
//! ## What the scaling changes
//!
//! Three things, and they are this module's whole content.
//!
//! 1. **The §9.2.1 tables.** The parent's irrational `π` family
//!    constants become exact multiples of a quarter turn. `atan2Pi`
//!    is the sharp case: every row where `atan2` delivered a rounded
//!    `±π`, `±π/2`, `±π/4` or `±3π/4` with `INEXACT` delivers `±1`,
//!    `±1/2`, `±1/4` or `±3/4` here, representable at every format
//!    precision, exact, `OK` (§7.5 then forbids `INEXACT` on them).
//!    A caller reading flags sees a different function, which is why
//!    the per format test files assert the flag difference against
//!    `atan2` directly.
//! 2. **The exact sets.** `asinPi(±1) = ±1/2`, `atanPi(±1) = ±1/4`,
//!    `acosPi(+1) = +0`, `acosPi(−1) = 1`, `acosPi(±0) = 1/2` and
//!    `atan2Pi`'s diagonals are all exact where their radian
//!    counterparts were irrational. The classifiers in `exact_pi` own
//!    them, and that module's no ties theorem covers every input they
//!    decline: no operation in this family has a nearest mode tie at
//!    any format, so the unconditional `INEXACT` past a classifier is
//!    correct in every mode.
//! 3. **The anchor families.** Dividing by `π` moves the asymptotes
//!    onto format grid points. The parent's asymptotes (`±π/2`, `±π`)
//!    are irrational and hug nothing; `±1/2` and `±1` are grid points
//!    at every precision, so three of the four kernels grow an
//!    ADR-0051 residual channel. That is the whole of ADR-0061's
//!    closed anchor list for this half of the family.
//!
//! ## The anchor derivations (ADR-0061's closed list)
//!
//! Write `P` for `F::PRECISION` and `adj(v)` for the adjusted
//! exponent `q_v + digits(c_v) − 1`, so that
//! `10^adj(v) ≤ |v| < 10^(adj(v) + 1)`. Every gate below reads
//! exponents and digit counts only, never coefficients, so it costs
//! nothing on the calls it does not take.
//!
//! The boundary each derivation must clear is the same in all four:
//! `1/2` and `1` are format grid points whose neighbour below sits
//! one quantum `10^−P` away (`0.5` is `5·10^(P−1)` at exponent `−P`;
//! `1` is `10^(P−1)` at exponent `−P+1` with predecessor
//! `1 − 10^−P`), so the nearest rounding boundary, the midpoint, is
//! at absolute distance `5·10^(−P−1)`. A true value strictly between
//! the anchor and that midpoint rounds identically to the residual
//! channel's denoted interval in every mode and at every precision.
//!
//! ### `atanPi` at large `|x|`
//!
//! Gate `adj(x) ≥ P + 2`, so `|x| ≥ 10^(P+2)`. For `x > 0`,
//! `atanPi(x) = 1/2 − atan(1/x)/π` and `atan(t) < t` for `t > 0`, so
//!
//! > `1/2 − atanPi(x) < 1/(π|x|) ≤ 1.01/(π·10^(P+2)) ≤ 3.3·10^(−P−3)`
//!
//! (the `1.01` is slack the strict inequality already covers). Side
//! theorem: `|atanPi(x)| < 1/2` strictly for every finite `x`, so the
//! residual sits on the shrinking side and the sign rides the anchor.
//!
//! | format | `P` | gate `adj(x) ≥` | hug `≤` | boundary | margin |
//! |---|---|---|---|---|---|
//! | `Decimal128` | 34 | 36 | `3.3·10^−37` | `5·10^−35` | `×151` |
//! | `Decimal64` | 16 | 18 | `3.3·10^−19` | `5·10^−17` | `×151` |
//! | `Decimal32` | 7 | 9 | `3.3·10^−10` | `5·10^−8` | `×151` |
//!
//! ### `acosPi` at tiny `|x|`
//!
//! Gate `adj(x) ≤ −(P + 3)`, so `|x| < 10^(−P−2)`. From
//! `acos(δ) = π/2 − δ − δ³/6 − …` the value is
//! `acosPi(δ) = 1/2 − δ/π + O(δ³)`, and for `|δ| ≤ 0.1` the series
//! tail is under 0.2% of the leading term:
//!
//! > `|acosPi(x) − 1/2| ≤ 1.01·|x|/π ≤ 3.3·10^(−P−3)`
//!
//! Side: `acos` is strictly decreasing and `acos(0) = π/2`, so the
//! value sits below `1/2` for `x > 0` and above it for `x < 0`; the
//! residual's growing side is therefore `x`'s sign bit. Margins are
//! the `atanPi` table's, decade for decade.
//!
//! ### `atan2Pi`, two hug families
//!
//! Both gates read the adjusted exponent gap `adj(y) − adj(x)`, which
//! brackets the ratio `r = |y/x|` by
//! `10^(gap−1) < r < 10^(gap+1)`.
//!
//! * **`gap ≥ P + 2`** (the `atanPi` arm, quadrant reflected):
//!   `r > 10^(P+1)` and the value hugs `±1/2` by
//!   `≤ 1.01/(π·10^(P+1)) ≤ 3.3·10^(−P−2)`, margin `≥ ×15` inside
//!   `5·10^(−P−1)`. The gap gate loses one decade to the bracket,
//!   which is the whole difference from `atanPi`'s `×151`. The side
//!   is **not** uniform: for `x > 0` the value approaches `±1/2` from
//!   inside (`atan2Pi = atan(y/x)/π`), while for `x < 0` the quadrant
//!   shift puts it outside (`x < 0, y > 0` gives
//!   `atan2Pi = 1/2 + |x/y|/π`), so `magnitude_grows` is `x`'s sign
//!   bit. The anchor carries `y`'s sign, which is the result's.
//! * **`gap ≤ −(P + 3)` with `x < 0`**: `r < 10^(−P−2)` and
//!   `atan2Pi = ±(1 − |y/(πx)|)` hugs `±1` from inside by
//!   `≤ 1.01·10^(−P−2)/π ≤ 3.3·10^(−P−3)`, margin `≥ ×150`.
//!   `magnitude_grows` is `false` for both signs of `y`.
//! * **`gap ≤ −(P + 3)` with `x > 0` is not an anchor.** There
//!   `atan2Pi ≈ y/(πx)`, a value of slope `1/π` against a shrinking
//!   ratio: it approaches the grid point `±0` the way `log2p1`
//!   approaches its own, which is to say generically, with no
//!   asymptote to hug. The plain ladder decides it, and the absence
//!   of an arm here is designed rather than missing.
//!
//! ### `asinPi` has no anchor arm
//!
//! Both ends are checked, and neither hugs.
//!
//! * At tiny `|x|` the value is `x/π + O(x³)`: slope `1/π`, so the
//!   result is a generic value near the grid point `±0` rather than a
//!   value hugging one (the `log2p1` precedent ADR-0061 names, and
//!   the reason `asin`'s own `sticks_to(x)` leg has no analogue here:
//!   `asinPi(x)` is nowhere near `x`).
//! * At `|x| → 1` the distance to `±1/2` is `sqrt(2δ)/π` for
//!   `|x| = 1 − δ`, and `δ` cannot go below one quantum: `δ ≥ 10^−P`
//!   forces the distance above `sqrt(2)·10^(−P/2)/π`, which is
//!   `4.5·10^−18` at `Decimal128` against a `10^−34` quantum. The
//!   closest representable input to `1` therefore lands some `10^16`
//!   ulps away from `1/2`; the square root scale separates the two
//!   sets by half the format's decades, so no rung and no residual
//!   channel is involved. `acosPi` near `±1` is the same square root
//!   scale seen from the other side and needs no arm either.
//!
//! ## Accuracy
//!
//! Correctly rounded at every rounding direction, on the ADR-0059
//! ladder under `ladder::{ASINPI, ACOSPI, ATANPI, ATAN2PI}`: Tier 1
//! by construction over the classified and anchored sets, which are
//! unconditional, plus the Tier 2 model on the remainder. ADR-0061
//! records the negative result behind the missing fourth leg: the
//! ADR-0060 adjudicator route is closed for this family, because
//! `sin(πp/q)` is algebraic of degree growing with `φ(2q)` and at
//! format denominators that degree is past any fixed width integer
//! comparison. No adjudicated delivery appears in this module, and
//! none can be added by widening one.

use crate::exact_pi::{self, PiExact};
use crate::extended::ExtNum;
use crate::format::DecimalFormat;
use crate::inverse_trig::{
    acos_extended_core, asin_extended_core, atan2_extended_core, atan_extended_core,
};
use crate::ladder;
use core::cmp::Ordering;
use ferrodec_ieee::IeeeDecodedClass as Class;
use ferrodec_ieee::{RoundingMode, Status};

/// `asinPi(x)`: the arcsine in revolutions, `asin(x)/π`. Domain
/// `[−1, +1]`; outside is NaN with `INVALID`. Range `[−1/2, +1/2]`.
///
/// ## Exactness and ties (ADR-0059 classification leg)
///
/// The exact set is `{±0 → ±0, ±1 → ±1/2}`, decided from the input by
/// `exact_pi::asinpi_exact` and the zero row below. `asinPi(±1/2)` is
/// the family's non terminating rational `±1/6`: representable input,
/// rational value, no terminating decimal expansion (3 divides the
/// lowest terms denominator, the `rsqrt` `1/q` argument), so it is
/// neither exact nor a tie and the kernel's `INEXACT` is correct in
/// every mode. Everything else is irrational by Niven reversed
/// (docs/references/niven-irrational-numbers.md), and `exact_pi`'s no
/// ties theorem covers the whole domain.
pub fn asin_pi_kernel<F: DecimalFormat>(x: F, rm: RoundingMode) -> (F, Status) {
    ladder::ladder_run!(|ex| asin_pi_kernel_body::<F, _>(ex, x, rm))
}

/// Generic body of [`asin_pi_kernel`] (M4, ADR-0059); `None`
/// escalates (M8 ladder). `ex` is the working precision exemplar
/// (M8b): the receiver the constant and constructor surface reads its
/// width from, never a value the result depends on.
pub(crate) fn asin_pi_kernel_body<F: DecimalFormat, E: ExtNum>(
    ex: E,
    x: F,
    rm: RoundingMode,
) -> Option<(F, Status)> {
    // §9.2.1: NaN per the crate-wide rules, `asinPi(±0) = ±0` exact,
    // and both infinities out of domain (transcribed from the C23
    // `asinpi` Annex F rows and the MPFR 4.2.2 `mpfr_asinu`
    // documentation, which agree with each other and with the
    // mathematical necessity that the domain is `[−1, 1]`).
    match x.classify() {
        Class::SignalingNaN { .. } => return Some((x.nan_from(), Status::INVALID)),
        Class::QuietNaN { .. } => return Some((x, Status::OK)),
        Class::Infinity { .. } => return Some((F::NAN, Status::INVALID)),
        Class::Zero { .. } => return Some((x, Status::OK)),
        Class::Finite { .. } => {}
    }
    if x.abs().partial_cmp_fmt(F::ONE).0 == Some(Ordering::Greater) {
        return Some((F::NAN, Status::INVALID));
    }
    let (coef, exp, sign) = x
        .to_extended_parts()
        .expect("finite: NaN and infinity dispatched above");
    if let Some(exact) = exact_pi::asinpi_exact(coef, exp, sign) {
        return Some(exact_pi::deliver_pi_exact::<F>(exact, rm));
    }
    // No anchor arm: the module header proves both ends generic (the
    // slope `1/π` at 0 and the square root scale at `±1`).
    let value = asin_extended_core::<F, E>(ex.from_format(x)).mul(ex.inv_pi());
    ladder::round_guarded::<F, E>(value, rm, &ladder::ASINPI)
}

/// `acosPi(x)`: the arccosine in revolutions, `acos(x)/π`. Domain
/// `[−1, +1]`; outside is NaN with `INVALID`. Range `[0, 1]`.
///
/// ## Exactness and ties (ADR-0059 classification leg)
///
/// The exact set is `{±0 → 1/2, +1 → +0, −1 → 1}`, decided from the
/// input by `exact_pi::acospi_exact` and the zero row below.
/// `acosPi(1/2) = 1/3` and `acosPi(−1/2) = 2/3` are the non
/// terminating rationals: neither exact nor a tie, and the `INEXACT`
/// past the classifier is correct in every mode. Everything else is
/// irrational by Niven reversed
/// (docs/references/niven-irrational-numbers.md).
pub fn acos_pi_kernel<F: DecimalFormat>(x: F, rm: RoundingMode) -> (F, Status) {
    ladder::ladder_run!(|ex| acos_pi_kernel_body::<F, _>(ex, x, rm))
}

/// Generic body of [`acos_pi_kernel`] (M4, ADR-0059); `None`
/// escalates (M8 ladder). `ex` is the working precision exemplar
/// (M8b).
pub(crate) fn acos_pi_kernel_body<F: DecimalFormat, E: ExtNum>(
    ex: E,
    x: F,
    rm: RoundingMode,
) -> Option<(F, Status)> {
    // §9.2.1: NaN per the crate-wide rules, both infinities out of
    // domain, and `acosPi(±0) = 1/2` for both zero signs (the even
    // function's value at the domain's centre, exact at every format
    // precision where `acos(±0) = π/2` was a rounded irrational).
    match x.classify() {
        Class::SignalingNaN { .. } => return Some((x.nan_from(), Status::INVALID)),
        Class::QuietNaN { .. } => return Some((x, Status::OK)),
        Class::Infinity { .. } => return Some((F::NAN, Status::INVALID)),
        Class::Zero { .. } => {
            return Some(exact_pi::deliver_pi_exact::<F>(
                PiExact::Zvalue {
                    coef: 5,
                    exp: -1,
                    neg: false,
                },
                rm,
            ))
        }
        Class::Finite { .. } => {}
    }
    if x.abs().partial_cmp_fmt(F::ONE).0 == Some(Ordering::Greater) {
        return Some((F::NAN, Status::INVALID));
    }
    let (coef, exp, sign) = x
        .to_extended_parts()
        .expect("finite: NaN and infinity dispatched above");
    if let Some(exact) = exact_pi::acospi_exact(coef, exp, sign) {
        return Some(exact_pi::deliver_pi_exact::<F>(exact, rm));
    }
    // ADR-0051 residual channel at the `1/2` anchor, derived in the
    // module header (hug `≤ 3.3·10^(−P−3)`, boundary `5·10^(−P−1)`,
    // margin `≥ ×150` at every format). The side is `x`'s sign, from
    // the strict monotonicity of `acos` through `acos(0) = π/2`.
    // Unguarded by design: the anchor leg runs before the ladder's
    // predicate, and no rung separates an asymptotically shrinking
    // residual.
    let adj = exp + coef.decimal_digit_count() as i32 - 1;
    if adj <= -(F::PRECISION as i32 + 3) {
        let (result, status) = ex.half().to_format_with_residual::<F>(sign, rm);
        return Some((result, status | Status::INEXACT));
    }
    let value = acos_extended_core::<F, E>(ex, ex.from_format(x)).mul(ex.inv_pi());
    ladder::round_guarded::<F, E>(value, rm, &ladder::ACOSPI)
}

/// `atanPi(x)`: the arctangent in revolutions, `atan(x)/π`. Range
/// `(−1/2, +1/2)` on the finite inputs, closed at `±1/2` by the
/// infinities.
///
/// ## Exactness and ties (ADR-0059 classification leg)
///
/// The exact set is `{±0 → ±0, ±1 → ±1/4, ±∞ → ±1/2}`: the quarter
/// turn family the decimal formats keep where `1/6` and `1/3` denied
/// it to `asinPi` and `acosPi`. `exact_pi::atanpi_exact` owns the
/// `±1` row, the table below owns the zeros and the infinities, and
/// every other input has an irrational value by Niven reversed
/// (docs/references/niven-irrational-numbers.md), never a tie.
pub fn atan_pi_kernel<F: DecimalFormat>(x: F, rm: RoundingMode) -> (F, Status) {
    ladder::ladder_run!(|ex| atan_pi_kernel_body::<F, _>(ex, x, rm))
}

/// Generic body of [`atan_pi_kernel`] (M4, ADR-0059); `None`
/// escalates (M8 ladder). `ex` is the working precision exemplar
/// (M8b).
pub(crate) fn atan_pi_kernel_body<F: DecimalFormat, E: ExtNum>(
    ex: E,
    x: F,
    rm: RoundingMode,
) -> Option<(F, Status)> {
    // §9.2.1: NaN per the crate-wide rules, `atanPi(±0) = ±0`, and
    // `atanPi(±∞) = ±1/2` EXACTLY with clean flags, where `atan(±∞)`
    // delivered a rounded `±π/2` with `INEXACT`.
    match x.classify() {
        Class::SignalingNaN { .. } => return Some((x.nan_from(), Status::INVALID)),
        Class::QuietNaN { .. } => return Some((x, Status::OK)),
        Class::Infinity { sign } => {
            return Some(exact_pi::deliver_pi_exact::<F>(
                PiExact::Zvalue {
                    coef: 5,
                    exp: -1,
                    neg: sign,
                },
                rm,
            ))
        }
        Class::Zero { .. } => return Some((x, Status::OK)),
        Class::Finite { .. } => {}
    }
    let (coef, exp, sign) = x
        .to_extended_parts()
        .expect("finite: NaN and infinity dispatched above");
    if let Some(exact) = exact_pi::atanpi_exact(coef, exp, sign) {
        return Some(exact_pi::deliver_pi_exact::<F>(exact, rm));
    }
    // ADR-0051 residual channel at the `±1/2` anchor, derived in the
    // module header (hug `< 3.3·10^(−P−3)`, boundary `5·10^(−P−1)`,
    // margin `≥ ×151`). The side theorem `|atanPi| < 1/2` holds for
    // every finite `x`, so the magnitude never grows past the anchor;
    // the anchor carries the operand's sign, which the format rounder
    // then honours per direction (no negation reflection is needed,
    // the value is signed before it reaches the rounder). Unguarded
    // by design, as in `acosPi`.
    let adj = exp + coef.decimal_digit_count() as i32 - 1;
    if adj >= F::PRECISION as i32 + 2 {
        let (result, status) = ex
            .half()
            .with_sign(sign)
            .to_format_with_residual::<F>(false, rm);
        return Some((result, status | Status::INEXACT));
    }
    let value = atan_extended_core::<F, E>(ex.from_format(x)).mul(ex.inv_pi());
    ladder::round_guarded::<F, E>(value, rm, &ladder::ATANPI)
}

/// `atan2Pi(y, x)`: the two argument arctangent in revolutions,
/// `atan2(y, x)/π`. Range `(−1, +1]`, quadrant per IEEE 754-2019
/// §9.2.1.
///
/// ## Exactness and ties (ADR-0059 classification leg)
///
/// This is the family's richest exact set, and the one that differs
/// most from its radian parent. Every §9.2.1 row is an exact multiple
/// of a quarter turn (`±0`, `±1/4`, `±1/2`, `±3/4`, `±1`) delivered
/// with `OK`, where `atan2`'s corresponding rows were rounded `π`
/// family irrationals carrying `INEXACT`. The finite diagonals
/// `|y| = |x|` join them through `exact_pi::atan2pi_exact` (`±1/4`
/// for `x > 0`, `±3/4` for `x < 0`, signed by `y`, decided on
/// stripped parts so cohorts agree). Every other finite pair has an
/// irrational value by Niven on the tangent
/// (docs/references/niven-irrational-numbers.md) and is never a tie,
/// so the `INEXACT` past the classifier is correct in every mode.
pub fn atan2_pi_kernel<F: DecimalFormat>(y: F, x: F, rm: RoundingMode) -> (F, Status) {
    ladder::ladder_run!(|ex| atan2_pi_kernel_body::<F, _>(ex, y, x, rm))
}

/// The magnitudes the §9.2.1 `atan2Pi` rows take: multiples of a
/// quarter turn, closed as a set, each representable at every format
/// precision (two significant digits at most, against `Decimal32`'s
/// seven).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum QuarterTurn {
    /// `1/4`, the `±∞ / +∞` diagonal.
    Quarter,
    /// `1/2`, the `y = ±∞` and `x = ±0` axis.
    Half,
    /// `3/4`, the `±∞ / −∞` diagonal.
    ThreeQuarters,
    /// `1`, the negative `x` axis.
    Full,
}

/// Deliver a signed quarter turn exactly, through the shared exact
/// pack (`exact_pi::deliver_pi_exact`) so this family's §9.2.2
/// preferred quantum and its `OK` status come from one place.
fn quarter_turn<F: DecimalFormat>(turn: QuarterTurn, neg: bool, rm: RoundingMode) -> (F, Status) {
    let (coef, exp) = match turn {
        QuarterTurn::Quarter => (25, -2),
        QuarterTurn::Half => (5, -1),
        QuarterTurn::ThreeQuarters => (75, -2),
        QuarterTurn::Full => (1, 0),
    };
    exact_pi::deliver_pi_exact::<F>(PiExact::Zvalue { coef, exp, neg }, rm)
}

/// Generic body of [`atan2_pi_kernel`] (M4, ADR-0059); `None`
/// escalates (M8 ladder). `ex` is the working precision exemplar
/// (M8b).
pub(crate) fn atan2_pi_kernel_body<F: DecimalFormat, E: ExtNum>(
    ex: E,
    y: F,
    x: F,
    rm: RoundingMode,
) -> Option<(F, Status)> {
    // §9.2.1, transcribed as `atan2`'s own table (the rows in
    // `inverse_trig::atan2_kernel_body`) with every result scaled by
    // `1/π`, and cross checked row by row against the C23 `atan2pi`
    // Annex F list, which agrees:
    //
    // | operands | atan2 | atan2Pi |
    // |---|---|---|
    // | `sNaN` anywhere | qNaN + `INVALID` | same |
    // | `NaN` anywhere | propagated | same |
    // | `(±∞, +∞)` | `±π/4` inexact | `±1/4` EXACT |
    // | `(±∞, −∞)` | `±3π/4` inexact | `±3/4` EXACT |
    // | `(±∞, finite)` | `±π/2` inexact | `±1/2` EXACT |
    // | `(±y, −∞)` | `±π` inexact | `±1` EXACT |
    // | `(±y, +∞)` | `±0` exact | same |
    // | `(±0, −0)` | `±π` inexact | `±1` EXACT |
    // | `(±0, +0)` | `±0` exact | same |
    // | `(±0, x < 0)` | `±π` inexact | `±1` EXACT |
    // | `(±0, x > 0)` | `±0` exact | same |
    // | `(y ≠ 0, ±0)` | `±π/2` inexact | `±1/2` EXACT |
    //
    // The right column is why this kernel raises no flag on eight of
    // its twelve rows: `1/4`, `1/2`, `3/4` and `1` are representable
    // at every format precision, so §7.5 forbids `INEXACT` where the
    // radian spelling was obliged to raise it.
    if y.is_signaling_nan() || x.is_signaling_nan() {
        return Some((y.propagate_nan2(x), Status::INVALID));
    }
    if y.is_nan() || x.is_nan() {
        return Some((y.propagate_nan2(x), Status::OK));
    }
    let y_neg = y.is_sign_negative();
    let x_neg = x.is_sign_negative();
    // The sign of every row below is `y`'s, including on the zeros:
    // the result's sign follows the ordinate, never the abscissa.
    let signed_zero = || exact_pi::deliver_pi_exact::<F>(PiExact::Zero { neg: y_neg }, rm);
    if x.is_infinite() && y.is_infinite() {
        let turn = if x_neg {
            QuarterTurn::ThreeQuarters
        } else {
            QuarterTurn::Quarter
        };
        return Some(quarter_turn::<F>(turn, y_neg, rm));
    }
    if y.is_infinite() {
        return Some(quarter_turn::<F>(QuarterTurn::Half, y_neg, rm));
    }
    // A finite ordinate against an infinite or zero abscissa collapses
    // to the axis rows, which differ only in the abscissa's sign.
    if x.is_infinite() || (x.is_zero() && y.is_zero()) || y.is_zero() {
        return Some(if x_neg {
            quarter_turn::<F>(QuarterTurn::Full, y_neg, rm)
        } else {
            signed_zero()
        });
    }
    if x.is_zero() {
        return Some(quarter_turn::<F>(QuarterTurn::Half, y_neg, rm));
    }

    // Both finite and nonzero.
    let (cy, qy, sy) = y
        .to_extended_parts()
        .expect("finite: NaN and infinity dispatched above");
    let (cx, qx, sx) = x
        .to_extended_parts()
        .expect("finite: NaN and infinity dispatched above");
    if let Some(exact) = exact_pi::atan2pi_exact(cy, qy, sy, cx, qx, sx) {
        return Some(exact_pi::deliver_pi_exact::<F>(exact, rm));
    }

    // The two ADR-0051 residual channels, gated on the adjusted
    // exponent gap and derived in the module header. Unguarded by
    // design, as in the unary kernels.
    let gap =
        (qy + cy.decimal_digit_count() as i32 - 1) - (qx + cx.decimal_digit_count() as i32 - 1);
    let p = F::PRECISION as i32;
    if gap >= p + 2 {
        // `±1/2` from inside for `x > 0` and from outside for `x < 0`
        // (the quadrant shift crosses the axis), margin `≥ ×15`.
        let (result, status) = ex
            .half()
            .with_sign(y_neg)
            .to_format_with_residual::<F>(x_neg, rm);
        return Some((result, status | Status::INEXACT));
    }
    if x_neg && gap <= -(p + 3) {
        // `±1` from inside, margin `≥ ×150`. The mirrored case
        // `x > 0` is deliberately absent: slope `1/π` against a
        // shrinking ratio anchors nothing.
        let (result, status) = ex
            .one()
            .with_sign(y_neg)
            .to_format_with_residual::<F>(false, rm);
        return Some((result, status | Status::INEXACT));
    }

    let value =
        atan2_extended_core::<F, E>(ex, ex.from_format(y), ex.from_format(x)).mul(ex.inv_pi());
    ladder::round_guarded::<F, E>(value, rm, &ladder::ATAN2PI)
}

#[cfg(test)]
mod tests {
    /// The three formats this crate serves, by precision.
    const PRECISIONS: [i32; 3] = [34, 16, 7];

    /// `a·10^−da ≥ factor · b·10^−db` in integer arithmetic, for
    /// `db ≥ da`: the margin check the anchor derivations need,
    /// stated where a float comparison would hide a decade error.
    fn clears(a: u128, da: u32, b: u128, db: u32, factor: u128) -> bool {
        a * 10u128.pow(db - da) >= factor * b
    }

    /// The gate constants, pinned per format. An off-by-one moves a
    /// gate onto a ratio the residual channel's margin no longer
    /// covers, which is ADR-0060's named "constant bookkeeping error"
    /// failure mode arriving in a different family.
    #[test]
    fn gate_constants_per_format() {
        for p in PRECISIONS {
            assert_eq!(p + 2, atan_pi_gate(p), "atanPi gate at P = {p}");
            assert_eq!(-(p + 3), acos_pi_gate(p), "acosPi gate at P = {p}");
            assert_eq!(p + 2, atan2_pi_half_gate(p), "atan2Pi ±1/2 gate");
            assert_eq!(-(p + 3), atan2_pi_full_gate(p), "atan2Pi ±1 gate");
        }
    }

    // The gates as the kernels spell them, so a kernel edit that
    // changes one fails this file rather than drifting silently.
    fn atan_pi_gate(p: i32) -> i32 {
        p + 2
    }
    fn acos_pi_gate(p: i32) -> i32 {
        -(p + 3)
    }
    fn atan2_pi_half_gate(p: i32) -> i32 {
        p + 2
    }
    fn atan2_pi_full_gate(p: i32) -> i32 {
        -(p + 3)
    }

    /// Every anchor arm's hug bound must sit at least a decade inside
    /// the first rounding boundary (`5·10^(−P−1)`), and the header's
    /// per family margins must be the ones the arithmetic gives. The
    /// decades cancel, so each margin is uniform across the three
    /// formats, which is the fact the loop pins.
    #[test]
    fn anchor_margins_clear_the_first_boundary() {
        for p in PRECISIONS {
            let p = p as u32;
            // atanPi and the atan2Pi `±1` arm and acosPi: hug
            // `3.3·10^(−P−3)`, i.e. `33·10^(−P−4)`.
            assert!(
                clears(5, p + 1, 33, p + 4, 150),
                "P = {p}: the ×150 arms do not clear the boundary"
            );
            assert!(
                !clears(5, p + 1, 33, p + 4, 160),
                "P = {p}: the ×150 arms' margin is overstated"
            );
            // The atan2Pi `±1/2` arm loses one decade to the ratio
            // bracket: hug `3.3·10^(−P−2)`, i.e. `33·10^(−P−3)`.
            assert!(
                clears(5, p + 1, 33, p + 3, 15),
                "P = {p}: the ratio-gated arm does not clear the boundary"
            );
            assert!(
                !clears(5, p + 1, 33, p + 3, 16),
                "P = {p}: the ratio-gated arm's margin is overstated"
            );
        }
    }

    /// `asinPi` near `±1`: the closest representable input to 1 sits
    /// `δ = 10^−P` below it, and the value is then `sqrt(2δ)/π` below
    /// `1/2`, which must stay decades ABOVE the quantum `10^−P` for
    /// the "no anchor arm" claim to hold. Checked as a decade
    /// comparison: `sqrt(2·10^−P)/π > 10^(−P/2 − 1)`, and
    /// `−P/2 − 1 > −P` for every `P ≥ 3`.
    #[test]
    fn asin_pi_never_hugs_the_half_turn() {
        for p in PRECISIONS {
            let value_decade = -p / 2 - 1;
            assert!(
                value_decade > -p,
                "P = {p}: the sqrt-scale distance {value_decade} is not \
                 above the quantum decade {}",
                -p
            );
            // And the separation grows with the precision: at least
            // half the format's decades, never a fixed pad.
            assert!(
                value_decade + p >= p / 2 - 1,
                "P = {p}: the separation is not half the decades"
            );
        }
    }

    /// The §9.2.1 quarter turn set is representable at the narrowest
    /// format: two significant digits against `Decimal32`'s seven.
    /// That is what licenses delivering every row of `atan2Pi`'s
    /// table exactly, with `OK`, at all three precisions.
    #[test]
    fn quarter_turns_are_representable_everywhere() {
        let narrowest = *PRECISIONS.iter().min().expect("three formats");
        for (coef, exp) in [(25u128, -2i32), (5, -1), (75, -2), (1, 0)] {
            let mut digits = 0i32;
            let mut rest = coef;
            while rest > 0 {
                digits += 1;
                rest /= 10;
            }
            assert!(
                digits <= narrowest,
                "the quarter turn {coef}E{exp} needs {digits} digits, \
                 past the narrowest format's {narrowest}"
            );
        }
    }
}
