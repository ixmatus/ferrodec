//! `powr(x, y)` — `x` raised to the power `y`, defined by
//! `exp(y · ln(x))` (IEEE 754-2019 §9.2 `powr`, ADR-0059 Track D D3).
//!
//! `powr` is `pow`'s rule 8 pipeline under a different special value
//! table. The two operations share the kernel exactly: the same input
//! side exact and tie classifier (`exact::pow_exact_input`), the same
//! `ln` then multiply then `exp` composition at working precision, the
//! same escalation ladder, and budgets with identical itemizations.
//! Everything that distinguishes them happens before the first
//! approximation runs, in [`powr_special_cases`].
//!
//! ## The definitional contrast with `pow`
//!
//! `pow` extends `x^y` to negative bases through the integer exponent
//! sign rule, so it answers `pow(−2, 3) = −8` and carries the odd
//! integer machinery that implies. `powr` takes its definition from
//! `exp(y · ln x)` literally: `ln x` is undefined for `x < 0`, so
//! **every** negative base is an invalid operation, integer exponents
//! included. `powr` also declines the three limits where `pow` picks a
//! value by continuity of the integer power rather than of the
//! composition, because each is a genuine indeterminate form of
//! `y · ln x`:
//!
//! | input | `pow` | `powr` | the indeterminate form |
//! |---|---|---|---|
//! | `(x < 0, y)` | sign rule for integer `y` | qNaN + `INVALID` | `ln x` undefined |
//! | `(±0, ±0)` | `1` | qNaN + `INVALID` | `0 · (−∞)` |
//! | `(+∞, ±0)` | `1` | qNaN + `INVALID` | `0 · (+∞)` |
//! | `(+1, ±∞)` | `1` | qNaN + `INVALID` | `±∞ · 0` |
//!
//! Those four rows are the whole behavioural delta on the special
//! table, and `tests/transcend_exact_powr.rs` pins each of them beside
//! the `pow` call that disagrees.
//!
//! ## Special values (IEEE 754-2019 §9.2.1), in resolution order
//!
//! 1. A signaling NaN in either operand: qNaN + `INVALID`. Unlike
//!    `pow`, no earlier row preempts it — `powr` has no
//!    `pow(x, ±0) = 1` and no `pow(1, y) = 1` that fire on NaN.
//! 2. "`powr(x, y)` signals the invalid operation exception for
//!    `x < 0`", for every `y`. Ordered ahead of the quiet NaN rows
//!    because §9.2.1 states the quiet NaN row only "for `x ≥ 0`", so
//!    `powr(−2, qNaN)` is invalid rather than a quiet propagation.
//!    `−0` is not `< 0` and falls through to the zero rows; `−∞` is.
//! 3. "`powr(x, qNaN)` is qNaN for `x ≥ 0`" and "`powr(qNaN, y)` is
//!    qNaN", no exception.
//! 4. `powr(±0, y)`: "`powr(±0, ±0)` signals the invalid operation
//!    exception"; "`powr(±0, y)` is `+∞` and signals the divideByZero
//!    exception for finite `y < 0`"; "`powr(±0, −∞)` is `+∞`" with no
//!    exception; "`powr(±0, y)` is `+0` for `y > 0`". Both limits read
//!    off `exp(y · (−∞))`; only the finite one is a division by zero.
//!    The result is `+0` or `+∞` for either sign of the zero base:
//!    `powr` has no odd integer sign rule to apply.
//! 5. `powr(+∞, y)`: "`powr(+∞, ±0)` signals the invalid operation
//!    exception"; otherwise `exp(y · (+∞))` gives `+∞` for `y > 0` and
//!    `+0` for `y < 0`, `y = ±∞` included.
//! 6. "`powr(+1, y)` is 1 for finite `y`"; "`powr(+1, ±∞)` signals the
//!    invalid operation exception". Both rows name `+1` only, and the
//!    base `−1` was already refused at step 2, where `pow` instead
//!    delivers `pow(−1, ±∞) = 1`.
//! 7. "`powr(x, ±0)` is 1 for finite `x > 0`".
//! 8. `powr(x, ±∞)` for finite `x > 0`, `x ≠ 1`: `y · ln x` runs to
//!    `±∞` with the sign of `ln x`, so `x > 1` sends `+∞` to `+∞` and
//!    `−∞` to `+0`, and `0 < x < 1` mirrors it.
//! 9. Otherwise the general path, on a domain narrowed to exactly
//!    `x` finite `> 0` and `y` finite non-zero.
//!
//! ## Preferred exponent (IEEE 754-2019 §9.2.2)
//!
//! "`Q(powr(x, y))` is `floor(y × Q(x))`", the same rule `pow` cites.
//! It binds only where the delivery is exact; the ladder's guarded
//! deliveries are inexact and take the rounder's §6.3 disposition.
//! Quantum behaviour is pinned by test rather than steered here: this
//! module never touches the pack machinery.
//!
//! ## Exactness and ties (ADR-0059 classification leg)
//!
//! Shared with `pow` and unmodified. `exact::pow_exact_input` decides
//! exactness and nearest mode ties from the inputs alone by the
//! decimal Lauter–Lefèvre criterion (`b | α`, `b | β`, `t = s^b`;
//! docs/references/lauter-lefevre-pow-boundary.md), including the real
//! `PRECISION + 1` ties such as `powr(5, 49)`. `powr` reaches that
//! classifier on a strictly smaller domain than `pow` does, since the
//! negative bases are gone, so `pow`'s completeness proofs cover it
//! verbatim. Past the classifier `x^y` is irrational and the
//! unconditional `INEXACT` is correct in every mode.
//!
//! ## Accuracy and claim tier (ADR-0059 ladder, ADR-0060 negative result)
//!
//! Correctly rounded on the ADR-0059 escalation ladder: rung 1
//! evaluates at 50 digits and delivers only when the `ladder::POWR`
//! budget clears every rounding boundary of the format, otherwise the
//! identical body re-runs at rung 2's 110 digits, and under the
//! `unbounded-ladder` feature at a dynamic rung that widens until the
//! rounding is decided.
//!
//! `powr` carries `pow`'s tier statement verbatim, Tier 1 by
//! construction over the audited budget and the complete input side
//! classification plus the Tier 2 model in default two rung builds
//! (ADR-0059 §The claim ladder), and that tier **cannot** be upgraded
//! by the ADR-0060 mechanism that makes the rest of the algebraic §9.2
//! group unconditional. The obstruction is structural rather than an
//! implementation gap: the algebraic degree of `x^(a/b)` is `y`'s
//! reduced denominator `b`, which a format operand drives to `~10^33`,
//! and the Liouville floor's exponent scales linearly in that degree,
//! so no useful uniform floor exists (ADR-0060 §The powr negative
//! result). Matveev style linear forms in logarithms, the ADR-0059 S5
//! spike (docs/references/matveev-2000.md), are the only literature
//! route that could improve the claim.

use crate::exp::exp_from_extended_body;
use crate::extended::ExtNum;
use crate::format::DecimalFormat;
use crate::ladder;
use crate::ln::ln_extended_body;
use ferrodec_ieee::{RoundingMode, Status};

/// Apply the IEEE 754-2019 §9.2.1 `powr` special value table without
/// touching the working precision general path.
///
/// Returns `Some((result, status))` whenever a §9.2.1 row fires, and
/// `None` exactly on the general path domain: `x` finite `> 0` and `y`
/// finite non-zero. The module doc lists the rows in this function's
/// resolution order and derives each from `exp(y · ln x)`.
///
/// Loop-free and self-contained, matching
/// [`pow_special_cases`](crate::pow::pow_special_cases): its only
/// `F`-touch points (`is_nan` / `is_zero` / `is_infinite` /
/// `is_sign_negative` / `is_signaling_nan`, `partial_cmp_fmt`,
/// `propagate_nan2`, the named constants) are all on the ADR-0016
/// loop-free list, so a future Kani special case harness can exhaust
/// the table here rather than through [`powr_kernel`].
pub fn powr_special_cases<F: DecimalFormat>(x: F, y: F) -> Option<(F, Status)> {
    // 1. Signaling NaN. `powr` has no row that preempts it — THE
    //    structural contrast with `pow`, whose rules 1 and 2 answer
    //    `1` for a NaN operand.
    if x.is_signaling_nan() || y.is_signaling_nan() {
        return Some((x.propagate_nan2(y), Status::INVALID));
    }

    // 2. "powr(x, y) signals the invalid operation exception for
    //    x < 0", every y included, because `ln x` is undefined there.
    //    Ahead of the quiet NaN rows: §9.2.1 states the qNaN row only
    //    "for x ≥ 0", leaving powr(negative, qNaN) invalid. `-0` is
    //    not < 0 (the zero rows below own it); `-∞` is.
    if !x.is_nan() && x.is_sign_negative() && !x.is_zero() {
        return Some((F::NAN, Status::INVALID));
    }

    // 3. "powr(x, qNaN) is qNaN for x ≥ 0"; "powr(qNaN, y) is qNaN".
    if x.is_nan() || y.is_nan() {
        return Some((x.propagate_nan2(y), Status::OK));
    }

    let y_zero = y.is_zero();
    let y_neg = y.is_sign_negative();
    let y_inf = y.is_infinite();

    // 4. powr(±0, y). Reached with x = ±0 and y non-NaN.
    if x.is_zero() {
        if y_zero {
            // "powr(±0, ±0) signals the invalid operation exception":
            // the indeterminate `0 · (−∞)`. `pow(±0, ±0)` is 1.
            return Some((F::NAN, Status::INVALID));
        }
        if y_neg {
            // "powr(±0, y) is +∞ and signals the divideByZero
            // exception for finite y < 0"; "powr(±0, −∞) is +∞" with
            // no exception. Both are `exp(y · (−∞)) = +∞`; only the
            // finite exponent divides by zero. `+∞` for either sign of
            // the zero base: no odd integer sign rule exists in `powr`.
            return Some((
                F::INFINITY,
                if y_inf {
                    Status::OK
                } else {
                    Status::DIV_BY_ZERO
                },
            ));
        }
        // "powr(±0, y) is +0 for y > 0", `+∞` included
        // (`exp((+∞) · (−∞)) = +0`), and `+0` for either sign of base.
        return Some((F::ZERO, Status::OK));
    }

    // 5. powr(+∞, y). Step 2 refused every negative x, so an infinite
    //    x here is `+∞`.
    if x.is_infinite() {
        if y_zero {
            // "powr(+∞, ±0) signals the invalid operation exception":
            // the indeterminate `0 · (+∞)`. `pow(+∞, ±0)` is 1.
            return Some((F::NAN, Status::INVALID));
        }
        // `exp(y · (+∞))`: `+∞` for y > 0, `+0` for y < 0, y = ±∞
        // included.
        return Some((if y_neg { F::ZERO } else { F::INFINITY }, Status::OK));
    }

    // x is finite and strictly positive from here.
    let (cmp_one, _) = x.partial_cmp_fmt(F::ONE);

    // 6. "powr(+1, y) is 1 for finite y"; "powr(+1, ±∞) signals the
    //    invalid operation exception" (the indeterminate `±∞ · 0`).
    //    Both rows name `+1`; the base `−1` left at step 2, where
    //    `pow(−1, ±∞)` instead delivers 1.
    if matches!(cmp_one, Some(core::cmp::Ordering::Equal)) {
        if y_inf {
            return Some((F::NAN, Status::INVALID));
        }
        return Some((F::ONE, Status::OK));
    }

    // 7. "powr(x, ±0) is 1 for finite x > 0".
    if y_zero {
        return Some((F::ONE, Status::OK));
    }

    // 8. powr(x, ±∞) for finite x > 0, x ≠ 1: `y · ln x` runs to an
    //    infinity whose sign is the product of the two signs, so the
    //    result is `+∞` exactly when `x > 1` agrees with `y > 0`.
    if y_inf {
        let x_gt_one = matches!(cmp_one, Some(core::cmp::Ordering::Greater));
        return Some((
            if x_gt_one == y_neg {
                F::ZERO
            } else {
                F::INFINITY
            },
            Status::OK,
        ));
    }

    // 9. General path: x finite > 0, x ≠ 1, y finite non-zero.
    None
}

/// `x` raised to the power `y` under the IEEE 754-2019 §9.2 `powr`
/// definition, `exp(y · ln x)`.
///
/// See the module doc for the §9.2.1 table, the four rows that
/// deliberately disagree with `pow`, and the ADR-0060 tier statement.
pub fn powr_kernel<F: DecimalFormat>(x: F, y: F, rm: RoundingMode) -> (F, Status) {
    ladder::ladder_run!(|ex| powr_kernel_body::<F, _>(ex, x, y, rm))
}

/// Generic body of [`powr_kernel`] (M4, ADR-0059); `None` escalates
/// (M8 ladder). `ex` is the working precision exemplar (M8b): the
/// receiver the constant and constructor surface reads its width from,
/// never a value the result depends on.
pub(crate) fn powr_kernel_body<F: DecimalFormat, E: ExtNum>(
    ex: E,
    x: F,
    y: F,
    rm: RoundingMode,
) -> Option<(F, Status)> {
    if let Some(early) = powr_special_cases(x, y) {
        return Some(early);
    }

    // Past the table the domain is exactly `x` finite > 0 and `y`
    // finite non-zero, and `pow`'s sign machinery is conspicuously
    // absent. Its absence is the point: `powr` refuses every negative
    // base at step 2, so there is no `IntegerKind` test, no odd
    // integer `sign_neg`, and no `rm.for_negation()` reflection to
    // keep a directed mode on the correct neighbour across a sign
    // flip. `rm` therefore reaches both the classifier and the rounder
    // unreflected, and `x` reaches the classifier without `abs()`.
    //
    // The classifier itself is `pow`'s, called unchanged: exact
    // rational powers (`powr(4, 0.5) = 2`, `powr(10, 300) = 1E+300`)
    // and the `PRECISION + 1` boundary ties (`powr(5, 49)`) are
    // delivered from their exact coefficient through the format
    // rounder before any approximation runs. Past it `x^y` is
    // provably irrational, so the guarded delivery's unconditional
    // `INEXACT` is correct in every mode.
    if let Some(done) = crate::exact::pow_exact_input::<F>(x, y, rm) {
        return Some(done);
    }

    // General path, `pow`'s rule 8 verbatim: evaluate `exp(y · ln x)`
    // entirely at working precision and round once at the format
    // boundary. The overflow and underflow gates and the tiny argument
    // 1-anchor are inherited unchanged from the shared
    // `exp_from_extended_body` core.
    let ln_x_ext = ln_extended_body::<F, E>(ex, x);
    let y_ext = ex.from_format(y);
    let y_ln_x_ext = y_ext.mul(ln_x_ext);
    exp_from_extended_body::<F, E>(y_ln_x_ext, rm, &ladder::POWR)
}
