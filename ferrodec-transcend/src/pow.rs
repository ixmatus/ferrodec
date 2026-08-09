//! Moved from `ferrodec/src/math/pow.rs` @ commit 82a7fe1 (P0a.2 c11).
//! Behaviour-neutral: genericized over [`DecimalFormat`]; the
//! `Decimal128` instantiation is byte-identical to the pre-move
//! kernel. ADR-0016: [`pow_special_cases`] stays loop-free; the kani
//! shim routes only through it, never through [`pow_kernel`].
//!
//! `pow(x, y)` — `x` raised to the power `y`.
//!
//! The module also carries [`powi_kernel`] (IEEE 754-2019 §9.2
//! `pown`), `x` raised to an `i32` power, which shares this module's
//! sign rule and its `exp(·ln|x|)` composition but neither its
//! special-value table nor its exactness classifier: an integer
//! exponent legalises every negative base, makes every result
//! rational, and admits a working-precision powering arm for small
//! `|n|` that ADR-0060's floors require. Its own derivation lives on
//! [`powi_kernel`] and [`powi_special_cases`].
//!
//! ## Special cases (IEEE 754-2019 §9.2.1)
//!
//! Order matters for the NaN-and-zero tie-breakers:
//!
//! 1. `pow(x, ±0) = 1` for every `x`, **including NaN**. This is the
//!    one place where NaN doesn't propagate.
//! 2. `pow(1, y) = 1` for every `y`, including NaN and ±∞.
//! 3. NaN in `x` or `y` (other than the cases above) → NaN; sNaN
//!    raises `INVALID`.
//! 4. `pow(±0, y)`:
//!    * `y < 0`: `±∞ + DIV_BY_ZERO` (sign by `is_odd_integer(y)`).
//!    * `y > 0`: `±0` (sign by `is_odd_integer(y)`).
//! 5. `pow(±∞, y)`:
//!    * `y < 0`: `±0`.
//!    * `y > 0`: `±∞`.
//!    * Sign by `is_odd_integer(y)`.
//! 6. `pow(x, ±∞)`:
//!    * `|x| > 1, y = +∞` ⇒ `+∞`; `y = −∞` ⇒ `+0`.
//!    * `|x| < 1, y = +∞` ⇒ `+0`; `y = −∞` ⇒ `+∞`.
//!    * `|x| = 1, y = ±∞` ⇒ `1` (handled by rule 2 above).
//! 7. `pow(negative_finite, non_integer)` → `NaN + INVALID`.
//! 8. Otherwise: positive-base path via `exp(y · ln(x))` evaluated at
//!    `Extended` precision and rounded once at the end. Negative-
//!    integer-y over negative base applies the sign of `(-1)^y`.
//!
//! ## Accuracy
//!
//! Correctly rounded for every finite input via the `Extended`
//! pipeline (ADR-0032; supersedes ADR-0024's faithful contract).
//! Integer exponents up to `±256` first try a square and multiply
//! path at `Decimal128` precision; the caller falls through to
//! `Extended` whenever any intermediate multiply rounds (i.e. the
//! path is *only* taken when it produces a bit exact result).
//! Pre-1.15 the fast path was taken unconditionally for `|y| ≤ 256`
//! and accumulated ~5 ULP for cases like `pow(3, 50)` (H1 of the
//! 2026-05-10 six agent correctness review).
//!
//! `pow(x, y) = exp(y · ln |x|)` couples the `exp` and `ln` bounds.
//! The Arb empirical worst case half ULP margin from
//! `tests/vectors/transcend/pow.prov` (ADR-0026, fd-97a; binary
//! search over `(x, y)`) is `1.406e-2` at `Decimal32` precision,
//! `6.849e-3` at `Decimal64` precision, and `1.020e-2` at
//! `Decimal128` precision. The kernel's working precision exceeds
//! the cumulative `exp` + `ln` + composition error budget by more
//! than thirty orders of magnitude on every format, so the
//! composed bound holds.
//!
//! `pow` is binary and was excluded from the ADR-0033 Plan C4
//! unary exhaustive sweep per the ADR-0033 §Rejected alternatives:
//! the canonical Decimal32 input pair cardinality is roughly 10^16,
//! beyond exhaustive reach at per-candidate Arb cost. `pow` at
//! `Decimal32` therefore continues to cite the sampled corpus
//! minimum as the binding empirical margin under the ADR-0033
//! Slice A corpus integrity discipline (cap hits asserted zero per
//! regeneration).
//!
//! The shared error model lives in ADR-0032 §Decision; the corpus
//! test (`tests/transcend_vectors.rs`) is the standing empirical
//! witness. For bases near 1 with large exponents the composed
//! bound additionally relies on `ln`'s near-1 direct path
//! (ADR-0050): `y · ln x` amplifies an *absolute* `ln` error by
//! `y`, and before the repair that broke the relative model for
//! `|y|` past ~10^15 at `Decimal128` (2026-06-09 review; the band
//! corpus `tests/vectors/transcend/anchor_bands/` pins the class).

use crate::exp::exp_from_extended_body;
use crate::extended::ExtNum;
use crate::format::DecimalFormat;
use crate::ladder;
use crate::ln::ln_extended_body;
use ferrodec_ieee::{decimal_digit_count_u128 as decimal_digit_count, IeeeDecodedClass as Class};
use ferrodec_ieee::{RoundingMode, Status};

/// Apply IEEE 754-2019 §9.2.1 rules 1–7 for `pow(x, y)` without
/// touching the `Extended`-precision general path.
///
/// Returns `Some((result, status))` whenever an IEEE-distinguished
/// rule fires; returns `None` for the rule-8 general path (finite
/// non-zero positive-base or integer-y over negative base).
///
/// Loop-free and self-contained; the Kani special-case harness in
/// `ferrodec/src/verify/pow.rs` proves the rule table by exhausting a
/// small operand pool against this function rather than against the
/// full [`pow_kernel`], keeping CBMC inside its time budget. ADR-0016
/// requires this routine stay loop-free: its only `F`-touch points
/// (`F::classify`, `partial_cmp_fmt`, `propagate_nan2`, the named
/// constants) are all on the explicit loop-free list.
pub fn pow_special_cases<F: DecimalFormat>(x: F, y: F) -> Option<(F, Status)> {
    // Rule 1: pow(x, ±0) = 1, even for NaN.
    if y.is_zero() {
        // sNaN x still "consumes" — IEEE 754-2019 §9.2.1 says
        // pow(x, ±0) = 1 even when x is sNaN, but we conservatively
        // raise INVALID for sNaN inputs since real implementations
        // disagree.
        let status = if x.is_signaling_nan() {
            Status::INVALID
        } else {
            Status::OK
        };
        return Some((F::ONE, status));
    }

    // Rule 2: pow(1, y) = 1, regardless of y.
    if !x.is_nan() {
        let (cmp, _) = x.partial_cmp_fmt(F::ONE);
        if matches!(cmp, Some(core::cmp::Ordering::Equal)) {
            let status = if y.is_signaling_nan() {
                Status::INVALID
            } else {
                Status::OK
            };
            return Some((F::ONE, status));
        }
    }

    // Rules 3: NaN propagation.
    if x.is_signaling_nan() || y.is_signaling_nan() {
        return Some((x.propagate_nan2(y), Status::INVALID));
    }
    if x.is_nan() || y.is_nan() {
        return Some((x.propagate_nan2(y), Status::OK));
    }

    let y_sign_neg = y.is_sign_negative();
    let y_int = integer_test(y);

    // Rule 4: pow(±0, y).
    if x.is_zero() {
        let result_sign = x.is_sign_negative() && matches!(y_int, IntegerKind::OddInteger);
        if y_sign_neg {
            // ±∞ + DIV_BY_ZERO
            return Some((
                if result_sign {
                    F::NEG_INFINITY
                } else {
                    F::INFINITY
                },
                Status::DIV_BY_ZERO,
            ));
        }
        // ±0
        return Some((if result_sign { F::NEG_ZERO } else { F::ZERO }, Status::OK));
    }

    // Rule 5: pow(±∞, y).
    if x.is_infinite() {
        let result_sign = x.is_sign_negative() && matches!(y_int, IntegerKind::OddInteger);
        if y_sign_neg {
            return Some((if result_sign { F::NEG_ZERO } else { F::ZERO }, Status::OK));
        }
        return Some((
            if result_sign {
                F::NEG_INFINITY
            } else {
                F::INFINITY
            },
            Status::OK,
        ));
    }

    // Rule 6: pow(x, ±∞).
    if y.is_infinite() {
        let abs_x = x.abs();
        let (cmp, _) = abs_x.partial_cmp_fmt(F::ONE);
        return Some(match (cmp, y_sign_neg) {
            (Some(core::cmp::Ordering::Greater), false) => (F::INFINITY, Status::OK),
            (Some(core::cmp::Ordering::Greater), true) => (F::ZERO, Status::OK),
            (Some(core::cmp::Ordering::Less), false) => (F::ZERO, Status::OK),
            (Some(core::cmp::Ordering::Less), true) => (F::INFINITY, Status::OK),
            // pow(±1, ±∞) = 1 per IEEE 754-2019 §9.2.1. Rule 2 above
            // only short-circuits for x = +1 (so that pow(-1, qNaN)
            // can still propagate NaN), so the negative-base case
            // arrives here.
            (Some(core::cmp::Ordering::Equal), _) => (F::ONE, Status::OK),
            (None, _) => unreachable!("NaN handled above"),
        });
    }

    // Rule 7: negative finite base with non-integer exponent.
    if x.is_sign_negative() && matches!(y_int, IntegerKind::NonInteger) {
        return Some((F::NAN, Status::INVALID));
    }

    // Rule 8: general path. Caller (`pow_kernel`) handles the integer
    // fast path and the `exp(y · ln(|x|))` Extended pipeline.
    None
}

/// `x` raised to the power `y`.
///
/// ## Exactness and ties (ADR-0059 classification leg)
///
/// Exactness and ties are decided from the inputs alone by the
/// decimal Lauter–Lefèvre criterion (`b | α`, `b | β`, `t = s^b`;
/// docs/references/lauter-lefevre-pow-boundary.md), including the
/// real `PRECISION + 1` ties such as `pow(5, 49)`; the criterion, its
/// tie handling, and the per-bail completeness proofs live on
/// `exact::pow_exact_input`. Past the classifier `x^y` is irrational
/// and the unconditional `INEXACT` is correct in every mode.
pub fn pow_kernel<F: DecimalFormat>(x: F, y: F, rm: RoundingMode) -> (F, Status) {
    ladder::ladder_run!(|ex| pow_kernel_body::<F, _>(ex, x, y, rm))
}

/// Generic body of [`pow_kernel`] (M4, ADR-0059); `None` escalates
/// (M8 ladder). `ex` is the working-precision exemplar (M8b): the
/// receiver the constant and constructor surface reads its width from,
/// never a value the result depends on.
pub(crate) fn pow_kernel_body<F: DecimalFormat, E: ExtNum>(
    ex: E,
    x: F,
    y: F,
    rm: RoundingMode,
) -> Option<(F, Status)> {
    if let Some(early) = pow_special_cases(x, y) {
        return Some(early);
    }

    // Rule 8: general path. `pow_special_cases` returned None, so x is
    // a finite non-zero (positive-base after rule 7 cleared the
    // negative-non-integer case) and y is a finite non-zero. Try the
    // integer fast path; *only* take its result when it was exact
    // (square-and-multiply at Decimal128 precision accumulates ULP
    // errors otherwise — the H1 finding in the 2026-05-10 review
    // documented ~5 ULP for `pow(3, 50)`). The fast path remains
    // valuable for small integer exponents where the result fits in
    // 34 digits and no multiply rounds.
    let y_int = integer_test(y);
    if let Some((v, status)) = pow_integer_fast_path(x, y, &y_int, rm) {
        if !status.inexact() {
            return Some((v, status));
        }
        // Fall through: int_pow accumulated rounding error; the
        // Extended pipeline below is more accurate.
    }

    // The pipeline evaluates |x|^y and re-applies the sign for an odd
    // integer y over a negative base. Round the magnitude under the
    // negation-reflected mode so the directed modes land on the
    // correct neighbour after the sign flip (the cbrt `for_negation`
    // rule; fd-aqs.5). Both the classifier and the kernel share this
    // sign treatment.
    let sign_neg = x.is_sign_negative() && matches!(y_int, IntegerKind::OddInteger);
    let eff_rm = if sign_neg { rm.for_negation() } else { rm };

    // Input-side exact and tie classification (ADR-0059 M7): an exact
    // rational power (pow(4, 0.5) = 2, pow(10, 300) = 1E+300) or a
    // PRECISION + 1 boundary case (the tie pow(5, 49)) is delivered
    // from its exact coefficient through the format rounder before any
    // approximation runs. This replaces the ADR-0047 post-hoc proof,
    // which was circular: it could only recognise an exact power the
    // kernel had already delivered exactly, so at TowardZero /
    // TowardNegative the kernel's 50-digit error landed pow(4, 0.5)
    // on 1.999…9 and the wrong value shipped with a spurious INEXACT.
    // Past this point x^y is provably irrational (the classifier's
    // completeness proofs), so the kernel's unconditional INEXACT is
    // correct in every mode.
    if let Some((mag, status)) = crate::exact::pow_exact_input::<F>(x.abs(), y, eff_rm) {
        return Some((if sign_neg { mag.neg() } else { mag }, status));
    }

    // General path: pow(x, y) = exp(y · ln(|x|)) evaluated entirely at
    // Extended precision. Single round when converting back to the
    // format, so the final result is correctly rounded per ADR-0032
    // (the Extended pipeline's cumulative error is bounded well inside
    // the half-ULP grid at every format precision; see the module
    // Accuracy section and ADR-0032 §Decision).
    let abs_x = x.abs();
    let ln_x_ext = ln_extended_body::<F, E>(ex, abs_x);
    let y_ext = ex.from_format(y);
    let y_ln_x_ext = y_ext.mul(ln_x_ext);
    let (result, status) = exp_from_extended_body::<F, E>(y_ln_x_ext, eff_rm, &ladder::POW)?;
    let signed = if sign_neg { result.neg() } else { result };
    Some((signed, status))
}

/// Try the square-and-multiply fast path for integer `y` up to `±256`.
/// Beyond that the cumulative rounding error in repeated multiplication
/// can exceed the ulp envelope, and we fall through to the general
/// `exp(y·ln(x))` path.
fn pow_integer_fast_path<F: DecimalFormat>(
    x: F,
    y: F,
    y_int: &IntegerKind,
    rm: RoundingMode,
) -> Option<(F, Status)> {
    if matches!(y_int, IntegerKind::NonInteger) {
        return None;
    }
    let (n_i32, st) = y.to_i32_fmt(RoundingMode::NearestEven);
    if !st.is_ok() {
        return None;
    }
    if !(-256..=256).contains(&n_i32) {
        return None;
    }
    Some(int_pow(x, n_i32, rm))
}

fn int_pow<F: DecimalFormat>(x: F, n: i32, rm: RoundingMode) -> (F, Status) {
    if n == 0 {
        return (F::ONE, Status::OK);
    }
    let mut status = Status::OK;
    let invert = n < 0;
    let mut exp = n.unsigned_abs();
    let mut base = x;
    let mut result = F::ONE;

    while exp > 0 {
        if exp & 1 == 1 {
            let (r, s) = result.mul_fmt(base, rm);
            result = r;
            status |= s;
        }
        exp >>= 1;
        if exp > 0 {
            let (b, s) = base.mul_fmt(base, rm);
            base = b;
            status |= s;
        }
    }

    if invert {
        let (r, s) = F::ONE.div_fmt(result, rm);
        result = r;
        status |= s;
    }
    (result, status)
}

// ----------------------------------------------------------------------------
// `powi` — IEEE 754-2019 §9.2 `pown`, `x` raised to an `i32` power
// (ADR-0059 Track D group D3, under ADR-0060's kernel architecture
// constraints).

/// The exponent magnitude at which [`powi_kernel`] switches from
/// binary powering at working precision to `exp(n·ln|x|)`.
///
/// ADR-0060 fixes the boundary, not convenience: the Liouville floor
/// for `pown` degrades as `10^−(34|n|+2)`, so the operand range over
/// which the two-rung claim can be unconditional is exactly the range
/// where the powering arm's ~200-unit budget applies. Six is the
/// widest `|n|` ADR-0060's table claims at all (`n` positive `≤ 6`,
/// negative `≥ −5`, once the exact integer adjudicator lands); past
/// it the floor is out of reach at any fixed rung and the cheaper
/// `exp`/`ln` composition is the honest choice. `n = −6` runs the arm
/// too — it is the faster route and the boundary is one constant, not
/// two — it simply does not carry the unconditional claim.
const POWI_POWERING_MAX: u32 = 6;

/// Apply IEEE 754-2019 §9.2.1's `pown` special-value table without
/// touching the working-precision path. `None` routes to the general
/// path (finite nonzero `x`, `n ≠ 0`).
///
/// The table is transcribed from the standard row by row; the order
/// below is the order the rows must be tested in, since `n = 0`
/// outranks NaN propagation and both outrank the zero and infinity
/// rows:
///
/// 1. `pown(x, 0)` is 1 if `x` is not a signaling NaN — quiet NaN and
///    the infinities included. A signaling NaN takes the general
///    §7.2 rule instead: quieted payload plus `INVALID`.
/// 2. `pown(±0, n)` is `±∞` and signals `divideByZero` for odd
///    `n < 0`, `+∞` and signals `divideByZero` for even `n < 0`,
///    `+0` for even `n > 0`, and `±0` for odd `n > 0`.
/// 3. `pown(+∞, n)` is `+∞` for `n > 0` and `+0` for `n < 0`;
///    `pown(−∞, n)` is `−∞` for odd `n > 0`, `+∞` for even `n > 0`,
///    `−0` for odd `n < 0`, and `+0` for even `n < 0`.
/// 4. A quiet NaN operand propagates; a signaling NaN raises
///    `INVALID` and returns the quieted payload.
///
/// A negative finite base is legal for **every** `n` — `n` is an
/// integer by type, so `pow`'s rule 7 (`NaN + INVALID` for a
/// non-integer exponent over a negative base) has no analog here, and
/// there is no `pown(x, ±∞)` row because there is no infinite `n`.
///
/// ## Divergence from [`pow_special_cases`], deliberate
///
/// `pow_special_cases` returns `(1, INVALID)` for `pow(sNaN, ±0)`: it
/// delivers the standard's value *and* raises, a documented
/// conservative choice made because implementations disagree there.
/// `pown` reads "is 1 if `x` is not a signaling NaN", which names the
/// signaling case and excludes it, so this routine returns the
/// quieted NaN. The two operations therefore disagree on
/// `x = sNaN, n = 0`, by construction and per the standard's own
/// wording; neither is changed to match the other.
pub fn powi_special_cases<F: DecimalFormat>(x: F, n: i32) -> Option<(F, Status)> {
    // Row 1: pown(x, 0) is 1 unless x signals.
    if n == 0 {
        if x.is_signaling_nan() {
            return Some((x.nan_from(), Status::INVALID));
        }
        return Some((F::ONE, Status::OK));
    }
    // `i32::MIN % 2` is 0 (only `i32::MIN / -1` overflows), so the
    // parity test is total over `i32`.
    let odd = n % 2 != 0;
    match x.classify() {
        Class::SignalingNaN { .. } => Some((x.nan_from(), Status::INVALID)),
        Class::QuietNaN { .. } => Some((x, Status::OK)),
        Class::Zero { sign, .. } => {
            let result_neg = sign && odd;
            Some(if n < 0 {
                (
                    if result_neg {
                        F::NEG_INFINITY
                    } else {
                        F::INFINITY
                    },
                    Status::DIV_BY_ZERO,
                )
            } else {
                (if result_neg { F::NEG_ZERO } else { F::ZERO }, Status::OK)
            })
        }
        Class::Infinity { sign } => {
            let result_neg = sign && odd;
            Some(if n < 0 {
                (if result_neg { F::NEG_ZERO } else { F::ZERO }, Status::OK)
            } else {
                (
                    if result_neg {
                        F::NEG_INFINITY
                    } else {
                        F::INFINITY
                    },
                    Status::OK,
                )
            })
        }
        Class::Finite { .. } => None,
    }
}

/// `x` raised to the integer power `n`: IEEE 754-2019 §9.2 `pown`.
///
/// ## Two arms, and why the boundary is a correctness constant
///
/// * `|n| ≤ 6`: square-and-multiply at *working* precision. Not the
///   format-precision `int_pow` of `pow`'s fast path — that routine
///   accumulates format ULPs and is only trusted when it happens to
///   round exactly (the H1 finding of the 2026-05-10 review) — but
///   the same recurrence carried at 50 or 110 digits, where the whole
///   chain costs ~20 units of the last working place. ADR-0060 makes
///   this arm load bearing rather than merely faster: the Liouville
///   floor for `pown` is `10^−(34|n|+2)` for positive `n` and
///   `10^−(34|n|+36)` for negative, and only a budget of this size
///   (`ladder::POWI_INT`, 200) clears it at rung 2's 110 digits over
///   the operand ranges ADR-0060 claims unconditional. Routing small
///   `|n|` through `exp(n·ln|x|)` instead would spend six decimal
///   orders on the `|ln x| ≤ 14151` amplification and put the claim
///   out of reach.
/// * `|n| ≥ 7`: `exp(n · ln|x|)`, `pow`'s rule-8 path with an exact
///   integer multiplier in place of the format-valued `y`. Same
///   `exp` gates, same composed budget shape (`ladder::POWI`), and
///   the same ADR-0050 near-1-base reliance on `ln`'s direct path.
///
/// ## Sign
///
/// The result is negative exactly when `x` is negative and `n` is
/// odd. Both arms evaluate `|x|^|n|`-shaped magnitudes and re-apply
/// the sign at the end, so the magnitude is rounded under the
/// negation-reflected mode (`rm.for_negation()`) and the directed
/// modes land on the correct neighbour after the flip — the `cbrt`
/// `for_negation` rule, fd-aqs.5, shared verbatim with [`pow_kernel`]
/// and with the classifier.
///
/// ## Exactness, ties, and the over/underflow gates
///
/// An integer exponent makes `x^n` rational for *every* representable
/// `x`, so — unlike every other §9.2 operation in this crate — the
/// classification question is width, not rationality.
/// `exact::powi_exact_input` decides it from the inputs alone: every
/// value expressible in `PRECISION + 1` stripped digits at an `i32`
/// exponent is delivered through the format rounder, exact ones with
/// no `INEXACT` (§7.5), midpoints with the mode's own tie rule (the
/// `powi(5, 49)` family), and out-of-range exponents with the §7.4
/// over/underflow disposition of the rounding direction. Past the
/// classifier the true value needs at least `PRECISION + 2` digits,
/// so it is neither a grid point nor a midpoint and the kernels'
/// unconditional `INEXACT` is correct in every mode.
///
/// That coherence is what keeps the `ladder_audit` lane quiet. On the
/// `|n| ≥ 7` arm a magnitude past the `exp` gates saturates unguarded,
/// which is sound because the gate thresholds put the true value past
/// the last boundary; the `|n| ≤ 6` arm has no gate and needs none,
/// because the format rounder's disposition is correct at any
/// exponent. Either way the only inputs whose true value sits exactly
/// ON a rounding boundary out there are the classifier's, and it owns
/// them at every magnitude — including the whole-range power-of-ten
/// family `x = 10^j`, whose `10^(j·n)` is on the grid at any `j·n`
/// (the ADR-0059 Track D `exp10_integer` lesson, arriving through the
/// input instead of the output). The single family the classifier
/// declines while on the grid is an exact value whose exponent
/// overflows `i32`, which its own proof shows is astronomically past
/// both gates.
///
/// ## Accuracy
///
/// Correctly rounded on the ADR-0059 escalation ladder from this
/// operation's first release, with the ADR-0060 caveat that the claim
/// is *unconditional* only over the operand ranges that ADR tabulates
/// (`n ∈ {−2, 2, 3}` in the bare two-rung build; `−5 ≤ n ≤ 6` once
/// the exact integer adjudicator lands) and carries the Tier 1 / Tier
/// 2 statement outside them. Rung 1 evaluates at 50 digits and
/// delivers only when the arm's budget clears every rounding boundary
/// of the format, otherwise the identical body re-runs at rung 2's
/// 110 digits, and under the `unbounded-ladder` feature at a dynamic
/// rung that widens until the rounding is decided.
///
/// The dynamic rung's termination width follows ADR-0060's
/// `p ≈ D + log₁₀ B + 2` from the same floors: for `−5 ≤ n ≤ 6` that
/// is `p ≤ 211`, inside the Ziv loop's 220-digit first attempt, which
/// is the ADR's claim. `n = −6` is the one operand the ADR's
/// dynamic-rung sentence ("first attempt for `pown |n| ≤ 6`") reads
/// one wider than its own floor table supports: `D ≤ 34·6 + 36 = 240`
/// puts its proven width at `p ≈ 245`, so it terminates at the first
/// doubling (440) instead. Nothing in the delivery changes; only the
/// proven bound moves one rung out, and it is recorded here rather
/// than rounded off.
#[doc(alias = "pown")]
pub fn powi_kernel<F: DecimalFormat>(x: F, n: i32, rm: RoundingMode) -> (F, Status) {
    ladder::ladder_run!(|ex| powi_kernel_body::<F, _>(ex, x, n, rm))
}

/// Generic body of [`powi_kernel`] (M4, ADR-0059); `None` escalates
/// (M8 ladder). `ex` is the working-precision exemplar (M8b): the
/// receiver the constant and constructor surface reads its width from,
/// never a value the result depends on.
pub(crate) fn powi_kernel_body<F: DecimalFormat, E: ExtNum>(
    ex: E,
    x: F,
    n: i32,
    rm: RoundingMode,
) -> Option<(F, Status)> {
    if let Some(early) = powi_special_cases(x, n) {
        return Some(early);
    }
    // Past the table: `x` is finite and nonzero, `n` is nonzero.
    let sign_neg = x.is_sign_negative() && n % 2 != 0;
    let eff_rm = if sign_neg { rm.for_negation() } else { rm };
    let abs_x = x.abs();

    // Input-side exact and tie classification (ADR-0059 M7 machinery,
    // ADR-0060 tripod leg 1): every result narrow enough to be a
    // format value or a nearest-mode midpoint, at any exponent.
    if let Some((mag, status)) = crate::exact::powi_exact_input::<F>(abs_x, n, eff_rm) {
        return Some((if sign_neg { mag.neg() } else { mag }, status));
    }

    let (mag, status) = if n.unsigned_abs() <= POWI_POWERING_MAX {
        powi_powering_arm::<F, E>(ex, abs_x, n, eff_rm)?
    } else {
        let ln_x_ext = ln_extended_body::<F, E>(ex, abs_x);
        let n_ext = ex.from_i32(n);
        exp_from_extended_body::<F, E>(n_ext.mul(ln_x_ext), eff_rm, &ladder::POWI)?
    };
    Some((if sign_neg { mag.neg() } else { mag }, status))
}

/// `|x|^n` for `|n| ≤ 6` by square-and-multiply at working precision,
/// closed by a Newton `recip` for `n < 0`. `None` escalates.
///
/// The accumulator starts at the working `1`, so the first multiply
/// of an odd `|n|` is exact (a format-sourced base has at most 34
/// digits and the working width is at least 50); the itemization on
/// [`ladder::POWI_INT`] counts it anyway.
fn powi_powering_arm<F: DecimalFormat, E: ExtNum>(
    ex: E,
    abs_x: F,
    n: i32,
    eff_rm: RoundingMode,
) -> Option<(F, Status)> {
    let mut base = ex.from_format(abs_x);
    let mut acc = ex.one();
    let mut e = n.unsigned_abs();
    while e > 0 {
        if e & 1 == 1 {
            acc = acc.mul(base);
        }
        e >>= 1;
        if e > 0 {
            base = base.square();
        }
    }
    let value = if n < 0 {
        working_reciprocal::<F, E>(acc)
    } else {
        acc
    };
    ladder::round_guarded::<F, E>(value, eff_rm, &ladder::POWI_INT)
}

/// `1 / v` at working precision for a positive nonzero `v`, with the
/// operand first scaled into `[1, 10)` and the scale re-applied to the
/// result as a pure exponent shift.
///
/// The scaling is not an optimization. `ExtNum::recip` seeds Newton by
/// round-tripping its operand through the *format*, and this is the
/// crate's first caller that can hand it a magnitude outside the
/// format's exponent range: `powi(1.7e2000, -6)` accumulates `≈
/// 2.4e12000` before the reciprocal, whose `to_format` is `+∞` and
/// whose `from_format` then panics on the non-finite datum. Every
/// earlier caller (`div`, the trig and inverse-trig
/// kernels) works on operands the surrounding algebra already keeps in
/// range, so the seam only opens here. Scaling to `[1, 10)` closes it
/// for every format at once, and it costs nothing measurable in the
/// budget: `with_exponent` and `mul_pow10_exp` are exact exponent
/// arithmetic, so the itemization on [`ladder::POWI_INT`] is unchanged.
fn working_reciprocal<F: DecimalFormat, E: ExtNum>(v: E) -> E {
    let digits = v.digit_count() as i32;
    // `v = unit · 10^shift` with `unit ∈ [1, 10)`, so
    // `1/v = (1/unit) · 10^-shift` and `1/unit ∈ (0.1, 1]`.
    let shift = v.exponent() + digits - 1;
    let unit = v.with_exponent(1 - digits);
    unit.recip::<F>().mul_pow10_exp(-shift)
}

/// Classify the exponent `y` as an integer (and which kind) or not.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum IntegerKind {
    NonInteger,
    EvenInteger,
    OddInteger,
}

fn integer_test<F: DecimalFormat>(y: F) -> IntegerKind {
    if y.is_nan() || y.is_infinite() {
        return IntegerKind::NonInteger;
    }
    match y.classify() {
        Class::Zero { .. } => IntegerKind::EvenInteger,
        Class::Finite {
            sign: _,
            biased_exp,
            coefficient,
        } => {
            let unbiased = biased_exp as i32 - F::BIAS;
            // Integer iff value's quantum exponent + (digit count of c) ≥
            // (digit count of c) + min_q_for_integer. Simpler: value is
            // integer iff coefficient × 10^unbiased is whole.
            if unbiased >= 0 {
                // Definitely an integer; can we tell odd/even?
                // value = c * 10^unbiased. For unbiased > 0, value is
                // c * 10^unbiased — last digit is 0 (even).
                if unbiased > 0 {
                    return IntegerKind::EvenInteger;
                }
                // unbiased == 0: parity from c's last digit.
                let last_digit = (coefficient % 10) as i32;
                if last_digit & 1 == 0 {
                    IntegerKind::EvenInteger
                } else {
                    IntegerKind::OddInteger
                }
            } else {
                // Fractional? Only integer if coefficient is divisible
                // by 10^|unbiased|.
                let drop = (-unbiased) as u32;
                let digits = decimal_digit_count(coefficient);
                if drop >= digits {
                    // |value| < 1 — only integer if value == 0, but
                    // we already excluded zeros above.
                    return IntegerKind::NonInteger;
                }
                let divisor = 10u128.pow(drop);
                if coefficient % divisor != 0 {
                    return IntegerKind::NonInteger;
                }
                let int_part = coefficient / divisor;
                let last_digit = (int_part % 10) as i32;
                if last_digit & 1 == 0 {
                    IntegerKind::EvenInteger
                } else {
                    IntegerKind::OddInteger
                }
            }
        }
        _ => IntegerKind::NonInteger,
    }
}
