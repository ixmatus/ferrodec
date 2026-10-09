//! `hypot(x, y) = sqrt(x² + y²)` (IEEE 754-2019 §9.2 `hypot`), on the
//! ADR-0059 escalation ladder with the ADR-0060 two band design.
//!
//! ## The two bands
//!
//! Write `w` for the larger magnitude operand and `z` for the smaller
//! (both finite and nonzero; every other class is dispatched by the
//! §9.2.1 table below), and `ρ = |z| / |w| ≤ 1` for their ratio. The
//! operation splits on `ρ` because the two regimes need *different
//! kinds* of decision, not merely different precisions.
//!
//! ### The anchor band (`ρ ≤ 10^−δ₀`, `δ₀ = ⌈(P + 2)/2⌉`)
//!
//! `hypot(w, z) = |w| · sqrt(1 + ρ²)` sits in `(|w|, |w| · (1 + ρ²/2)]`
//! — above `|w|` strictly for `z ≠ 0` (the side theorem), and below
//! `|w| · (1 + ρ²/2)` because `sqrt(1 + u) ≤ 1 + u/2`. So its relative
//! distance above the grid point `|w|` is at most
//!
//! > `ρ²/2 ≤ 10^(−2δ₀)/2 = 5 · 10^(−2δ₀ − 1)`.
//!
//! The first rounding boundary above `|w|` is the midpoint between
//! `|w|` and its successor, at relative distance at least
//! `5 · 10^(−P − 1)` (the worst case is a coefficient of `10^P − 1`,
//! where one ulp is `10^−P` of the value; a leading-1 coefficient
//! makes it ten times wider, and a *subnormal* `|w|` — whose quantum
//! is pinned at `etiny`, so its coefficient is narrower still — makes
//! it wider by as many decades as it has lost digits). The bound is
//! therefore the worst case across the whole range, normal and
//! subnormal alike. Since `δ₀ = ⌈(P + 2)/2⌉` gives `2δ₀ + 1 ≥ P + 3`,
//! the true value's distance is at least a factor `10^2` inside that
//! boundary:
//!
//! | format | `P` | `δ₀` | `ρ²/2 ≤` | first boundary `≥` | margin |
//! |---|---|---|---|---|---|
//! | `Decimal128` | 34 | 18 | `5·10^−37` | `5·10^−35` | `×10²` |
//! | `Decimal64` | 16 | 9 | `5·10^−19` | `5·10^−17` | `×10²` |
//! | `Decimal32` | 7 | 5 | `5·10^−11` | `5·10^−8` | `×10³` |
//!
//! The true value therefore lies strictly between the grid point
//! `|w|` and the next boundary above it, in every format. That is
//! exactly what the ADR-0051 residual channel encodes: the anchor
//! `|w|` widened to the rung's full width with `pre_sticky = true` on
//! the growing side denotes an interval that rounds identically to the
//! true value at every direction and precision. No rung of the ladder
//! is consulted, and none could help — the distance shrinks with `ρ`
//! without bound while the boundary stays put.
//!
//! Two consequences worth stating explicitly. The band's inputs are
//! never exact and never ties (the value is strictly between a grid
//! point and the next boundary), which is the other half of the
//! kernel band classifier's completeness argument. And the gate is
//! decided from exponents and digit counts alone — no arithmetic on
//! coefficients — so it costs nothing on the calls it does not take.
//!
//! ### The kernel band (`ρ` above the gate)
//!
//! Here the exponent gap is bounded, so the exact integer
//! `S = A² + B²` of the aligned operands is small enough to work with
//! and `crate::exact::hypot_exact_or_tie` decides exactness and ties
//! from the inputs (Pythagorean pairs, at every scale and every
//! cohort). What survives is irrational by Niven, so the
//! approximation kernel's unconditional `INEXACT` is correct, and
//! ADR-0060's Engine B floor bounds its distance to every boundary.
//!
//! The kernel itself scales both operands by the exact power of ten
//! `10^(−adj(w))` before squaring, which is a pure exponent shift on
//! the working type. That makes `w̃ ∈ [1, 10)` and `z̃ ≤ w̃`, so
//! `S̃ ∈ [1, 200)` and `sqrt(S̃) ∈ [1, 14.2)`: intermediate overflow
//! is not bounded away, it is *structurally absent*, and the Newton
//! seed for `sqrt` (which round-trips through the format) is always
//! well posed. Scaling the root back by `10^(adj w)` is exact too, so
//! the error model is untouched by the transform. Result overflow and
//! underflow near the format's edges then flow through
//! `round_guarded`'s `to_format` delivery and the rounder's §7.4
//! disposition, like every other kernel here.
//!
//! ## Special values (IEEE 754-2019 §9.2.1), and one reading recorded
//!
//! * `hypot(±0, ±0)` is `+0`.
//! * any `±∞` operand gives `+∞`, *including* `hypot(±∞, qNaN)` and
//!   `hypot(qNaN, ±∞)` — the standard's explicit exception to NaN
//!   propagation, because the result is determined whatever the other
//!   operand is.
//! * a signaling NaN anywhere gives a quiet NaN and `INVALID`, and it
//!   is checked **before** the infinity rule. §9.2.1 states the
//!   infinity exception for a *quiet* NaN operand (`qNaN` in the
//!   standard's own wording); §6.2 makes a signaling NaN operand
//!   signal `INVALID` for every general-computational operation, and
//!   §7.2 gives it precedence over result-determining rules. This
//!   crate's other two-operand kernels (`atan2`, `pow`) resolve the
//!   same collision in the same order, and the payload follows the
//!   shared first-NaN-wins rule
//!   (`DecimalFormat::propagate_nan2`).
//! * a quiet NaN with a finite other operand propagates.
//! * `hypot(x, ±0) = |x|` exactly, no exception.
//! * the result is always positive: neither operand's sign reaches it.
//! * `hypot(x, y)` and `hypot(y, x)` deliver identical bits and
//!   identical flags for every non-NaN operand pair — the kernel
//!   canonicalises the operand order before it computes anything. (For
//!   two NaN operands the delivered payload follows the crate-wide
//!   first-NaN-wins rule and is order sensitive, exactly as it is for
//!   `atan2` and `pow`.)
//!
//! ## Preferred exponent (IEEE 754-2019 §9.2.2)
//!
//! `Q(hypot(x, y))` is `min(Q(x), Q(y))`. The rule binds on the exact
//! deliveries — the zero cases, the `hypot(x, ±0) = |x|` case, and the
//! classifier's Pythagorean values — which pass that quantum to the
//! format rounder as its preferred exponent. An inexact result uses
//! the full format precision, as everywhere else in this crate.
//!
//! ## Accuracy
//!
//! Correctly rounded at every rounding direction, on the ADR-0059
//! ladder with the `ladder::HYPOT` budget. ADR-0060 makes the claim
//! unconditional across the band for this operation: the anchor band
//! is decided by a side theorem, the kernel band's exact and tie set
//! is classified input side, and what remains carries the Engine B
//! Liouville floor `≥ 1/(8.1 · S)` against a budget many orders
//! below it.

use crate::extended::ExtNum;
use crate::format::DecimalFormat;
use crate::ladder;
use core::cmp::Ordering;
use ferrodec_ieee::{RoundingMode, Status};
use ferrodec_multiword::U256;

/// `hypot(x, y) = sqrt(x² + y²)` (IEEE 754-2019 §9.2 `hypot`), rounded
/// by `rm`. The public wrappers are `Decimal128::hypot` and the
/// `Decimal64` / `Decimal32` siblings.
///
/// ## Exactness and ties (ADR-0059 classification leg)
///
/// Unlike the transcendental families, `hypot` has a rich exact set:
/// every scaled Pythagorean pair is in it (`hypot(3, 4) = 5`,
/// `hypot(0.3, 0.4) = 0.5`, `hypot(5, 12) = 13`, …). It is decided
/// completely, from the operands alone, by
/// `crate::exact::hypot_exact_or_tie`: the aligned integer
/// `S = A² + B²` is a perfect square exactly when the value is
/// rational, and the value is then that square root scaled. Everything
/// the classifier declines is irrational (Niven, *Irrational Numbers*
/// — docs/references/niven-irrational-numbers.md), so the kernel's
/// unconditional `INEXACT` past it is correct in every mode, and the
/// ladder's standing assumption that every remaining input sits a
/// finite distance from its rounding boundary holds with the ADR-0060
/// floor as its quantitative form.
///
/// The special-value deliveries (`hypot(x, ±0) = |x|`, the zeros, and
/// the infinities) are exact and carry `Status::OK`: §7.5 forbids
/// `INEXACT` on an exact result.
pub fn hypot_kernel<F: DecimalFormat>(x: F, y: F, rm: RoundingMode) -> (F, Status) {
    ladder::ladder_run!(|ex| hypot_kernel_body::<F, _>(ex, x, y, rm))
}

/// Generic body of [`hypot_kernel`] (M4, ADR-0059); `None` escalates
/// (M8 ladder). `ex` is the working-precision exemplar (M8b): the
/// receiver the constant and constructor surface reads its width from,
/// never a value the result depends on.
pub(crate) fn hypot_kernel_body<F: DecimalFormat, E: ExtNum>(
    ex: E,
    x: F,
    y: F,
    rm: RoundingMode,
) -> Option<(F, Status)> {
    // §9.2.1, in the order the standard's exceptions force. A
    // signaling NaN outranks the infinity rule (§6.2 / §7.2; the
    // module header records the reading and its agreement with
    // `atan2`), and the infinity rule then outranks quiet NaN
    // propagation — that inversion is specific to `hypot` and is the
    // one place this kernel's dispatch differs from `atan2`'s.
    if x.is_signaling_nan() || y.is_signaling_nan() {
        return Some((x.propagate_nan2(y), Status::INVALID));
    }
    if x.is_infinite() || y.is_infinite() {
        return Some((F::INFINITY, Status::OK));
    }
    if x.is_nan() || y.is_nan() {
        return Some((x.propagate_nan2(y), Status::OK));
    }
    // Both operands are finite or zero, so the seam decodes them (the
    // fd-aqs.13 contract: in-kernel callers unwrap after dispatching
    // the non-finite classes, which the three checks above just did).
    let (cx, qx, _) = x
        .to_extended_parts()
        .expect("finite or zero: NaN and infinity dispatched above");
    let (cy, qy, _) = y
        .to_extended_parts()
        .expect("finite or zero: NaN and infinity dispatched above");

    // §9.2.2: the preferred quantum of every exact delivery below.
    let q_pref = qx.min(qy);

    if cx.is_zero() && cy.is_zero() {
        // §9.2.1 "hypot(±0, ±0) is +0" — positive whatever the operand
        // signs, exact, no exception.
        return Some(pack_exact::<F>(U256::ZERO, q_pref, q_pref, rm));
    }
    if cy.is_zero() {
        // `hypot(x, ±0) = |x|`, exact: the magnitude is representable
        // by construction, and the preferred quantum re-expresses it
        // at `min(Q(x), Q(y))` when the precision allows.
        return Some(pack_exact::<F>(cx, qx, q_pref, rm));
    }
    if cx.is_zero() {
        return Some(pack_exact::<F>(cy, qy, q_pref, rm));
    }

    // Order canonicalisation. `w` is the larger magnitude operand.
    // When the magnitudes are equal the tie breaks on the quantum, so
    // the choice is a function of the operand *set* rather than of the
    // argument order: equal magnitude with equal quantum means
    // identical data, and either branch then delivers the same bits.
    // Everything downstream reads only `w`, `z` and the symmetric
    // `q_pref`, which is what makes `hypot(x, y)` and `hypot(y, x)`
    // byte-identical.
    let x_is_w = match x.abs().partial_cmp_fmt(y.abs()).0 {
        Some(Ordering::Greater) => true,
        Some(Ordering::Less) => false,
        _ => qx <= qy,
    };
    let (w, z, cw, qw, cz, qz) = if x_is_w {
        (x, y, cx, qx, cy, qy)
    } else {
        (y, x, cy, qy, cx, qx)
    };

    // The band gate, from exponents and digit counts only. With
    // `adj(v) = q_v + digits(c_v) − 1` the value satisfies
    // `10^adj(v) ≤ |v| < 10^(adj(v) + 1)`, so
    // `ρ = |z|/|w| < 10^(adj(z) − adj(w) + 1)`; an adjusted-exponent
    // gap strictly wider than `δ₀` therefore proves `ρ < 10^−δ₀`, the
    // anchor band's premise. Both adjusted exponents are inside
    // `±(E_MAX + P)`, so the difference cannot overflow `i32`. The gate
    // is conservative — it never claims a band it has not proven —
    // which is the direction soundness needs; the kernel band then
    // inherits `adj(w) − adj(z) ≤ δ₀` as the premise bounding its
    // alignment shifts.
    let delta0 = (F::PRECISION + 2).div_ceil(2) as i32;
    let adj_w = qw + cw.decimal_digit_count() as i32 - 1;
    let adj_z = qz + cz.decimal_digit_count() as i32 - 1;

    if adj_w - adj_z > delta0 {
        // ADR-0051 anchor residual delivery, on the side theorem
        // `hypot(w, z) > |w|` for `z ≠ 0`, with the boundary margin
        // derived in the module header (≥ ×100 at every format).
        // Unguarded by design: the anchor leg runs before the ladder's
        // predicate, and no rung resolves an asymptotically shrinking
        // residual.
        let anchor = ex.from_format(w.abs());
        let (result, status) = anchor.to_format_with_residual::<F>(true, rm);
        return Some((result, status | Status::INEXACT));
    }

    // Kernel band. Exact and tie values first, from the operands
    // alone (the proof and every bail live on the classifier).
    if let Some(exact) = crate::exact::hypot_exact_or_tie::<F>(cw, qw, cz, qz, rm) {
        return Some(exact);
    }

    // Scale by the exact `10^(−adj w)` so both squares and their sum
    // are O(1) at working precision; `mul_pow10_exp` is a pure
    // exponent shift, so this changes no digit and no error term.
    let w_s = ex.from_format(w.abs()).mul_pow10_exp(-adj_w);
    let z_s = ex.from_format(z.abs()).mul_pow10_exp(-adj_w);
    let sum = w_s.square().add(z_s.square());
    let result_ext = sum.sqrt::<F>().mul_pow10_exp(adj_w);
    ladder::round_guarded::<F, E>(result_ext, rm, &ladder::HYPOT)
}

/// Deliver an exact positive value `coef · 10^exp` at the §9.2.2
/// preferred quantum. The value is representable by construction at
/// every call site here, so the rounder returns it unchanged with a
/// clean status (§7.5 forbids `INEXACT` on an exact result), honouring
/// `q_preferred` as far as the precision allows.
fn pack_exact<F: DecimalFormat>(
    coef: U256,
    exp: i32,
    q_preferred: i32,
    rm: RoundingMode,
) -> (F, Status) {
    F::round_and_pack_finite(coef, exp, q_preferred, false, false, rm, Status::OK)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock_format::ValueFmt128;

    /// The band constant per format, pinned: an off-by-one in `δ₀`
    /// moves the gate onto a ratio the residual channel's boundary
    /// margin no longer covers, which is exactly the ADR-0060
    /// "constant bookkeeping error" failure mode.
    #[test]
    fn anchor_delta_per_format() {
        for (precision, want) in [(34u32, 18u32), (16, 9), (7, 5)] {
            assert_eq!((precision + 2).div_ceil(2), want, "δ₀ for P = {precision}");
        }
    }

    /// The anchor band's margin inequality, checked as decimal
    /// exponents rather than trusted from the prose: the true value's
    /// relative offset above `|w|` (`≤ 5·10^(−2δ₀−1)`) must sit at
    /// least two decades inside the first boundary above it
    /// (`≥ 5·10^(−P−1)`).
    #[test]
    fn anchor_band_clears_the_first_boundary() {
        for precision in [34u32, 16, 7] {
            let delta0 = (precision + 2).div_ceil(2);
            let offset_decade = 2 * delta0 + 1;
            let boundary_decade = precision + 1;
            assert!(
                offset_decade >= boundary_decade + 2,
                "P = {precision}: offset 5e-{offset_decade} is not two \
                 decades inside boundary 5e-{boundary_decade}"
            );
        }
    }

    /// The kernel band's alignment-shift bounds, the premise the
    /// classifier's integer widths rest on. Enumerated over every
    /// in-band `(adj w − adj z, digits)` combination rather than
    /// argued: with `adj(z) ≥ adj(w) − δ₀` and both digit counts in
    /// `1..=P`, the wide-side shift `q_w − q_z` stays within
    /// `δ₀ + P − 1` and the narrow-side shift within `P − 1`.
    #[test]
    fn in_band_alignment_shifts_stay_within_the_derived_bounds() {
        for precision in [34i32, 16, 7] {
            let delta0 = ((precision + 2) as u32).div_ceil(2) as i32;
            let (mut max_wide, mut max_narrow) = (i32::MIN, i32::MIN);
            for gap in 0..=delta0 {
                for dw in 1..=precision {
                    for dz in 1..=precision {
                        // adj_w − adj_z = gap ⇒ q_w − q_z = gap + dz − dw.
                        let delta = gap + dz - dw;
                        max_wide = max_wide.max(delta);
                        max_narrow = max_narrow.max(-delta);
                    }
                }
            }
            assert_eq!(max_wide, delta0 + precision - 1, "P = {precision} wide");
            assert_eq!(max_narrow, precision - 1, "P = {precision} narrow");
        }
    }

    /// The scaling transform is exact: `mul_pow10_exp` moves the
    /// exponent and nothing else, so squaring the scaled operands
    /// cannot overflow the working exponent however extreme the input
    /// decade is. Checked at both format edges.
    #[test]
    fn scaling_keeps_the_working_exponent_small() {
        for exp in [-6176i32, -3000, 0, 3000, 6111] {
            let v = ValueFmt128 {
                coef: 9_999_999_999_999_999_999_999_999_999_999_999,
                exp,
                sign: false,
            };
            let ext = crate::extended::Extended::ZERO.from_format(v);
            let adj = ext.exponent() + ext.digit_count() as i32 - 1;
            let scaled = ext.mul_pow10_exp(-adj);
            let scaled_adj = scaled.exponent() + scaled.digit_count() as i32 - 1;
            assert_eq!(scaled_adj, 0, "scaled value is not in [1, 10)");
            let sq = scaled.square();
            assert!(
                sq.exponent().abs() < 200,
                "squared exponent {} escaped the O(1) window",
                sq.exponent()
            );
        }
    }
}
