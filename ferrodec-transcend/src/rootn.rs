//! `rootn(x, n)` — the `n`-th root, IEEE 754-2019 §9.2 (ADR-0059
//! Track D group D3, fd-4zo.25).
//!
//! `rootn(x, n) = x^(1/n)` for an integer `n`, defined on the whole
//! real line for odd `n` and on `[0, ∞)` for even `n`. The general
//! path evaluates `sign(x) · exp(ln|x| / n)` on the ADR-0059
//! escalation ladder, exactly the shape [`crate::cbrt`] uses — and
//! `rootn(x, 3)` IS `cbrt(x)`, which the differential tests pin.
//!
//! ## The four arms, and why the small `|n|` ones are delegations
//!
//! ADR-0060 gives the algebraic §9.2 group its Liouville floors and
//! its unconditional operand ranges. Two of `rootn`'s arms clear that
//! bar without any of this module's machinery, because IEEE 754-2019
//! §5 already answers them correctly rounded:
//!
//! * `n = 1` returns `x` itself — the identity, with the quantum
//!   untouched, which is also §9.2.2's preferred exponent
//!   `floor(Q(x)/1) = Q(x)`.
//! * `n = −1` returns `1 ÷ x` through the format's own division, a §5
//!   basic operation and therefore correctly rounded by the format's
//!   existing verified rounder in every direction. Its preferred
//!   exponent `Q(1) − Q(x) = −Q(x)` is §9.2.2's `floor(Q(x)/−1)`.
//! * `n = 2` delegates to the format's own square root (§5.4.1,
//!   correctly rounded, preferred exponent `floor(Q(x)/2)` — §9.2.2's
//!   value here). The delegation carries its own exactness: a
//!   correctly rounded square root delivers a representable root
//!   exactly with clean flags, so this arm never consults the
//!   classifier.
//! * `n = −2` delegates to the `rsqrt` kernel body, whose direct
//!   Newton composition ADR-0060 mandates (the `|ln x| ≤ 14151`
//!   amplification of this module's `exp`/`ln` route cannot clear
//!   rSqrt's `4.9·10^−105` floor), and which carries the operation's
//!   unconditional two rung claim, its own classifier — the
//!   `rootn(2^2k, −2)` tie family included — and its `RSQRT` budget.
//!
//! Every other `n` (`|n| ≥ 3`) takes the general path: input-side
//! exact and tie classification, the hug-at-1 anchor arm, then the
//! guarded `exp(ln|x|/n)` pipeline under `ladder::ROOTN`.
//!
//! ## Accuracy
//!
//! Correctly rounded on the ADR-0059 escalation ladder from this
//! operation's first release: rung 1 evaluates at 50 digits and
//! delivers only when the `ladder::ROOTN` budget clears
//! every rounding boundary of the format, otherwise the identical
//! body re-runs at rung 2's 110 digits, and under the
//! `unbounded-ladder` feature at a dynamic rung that widens until the
//! rounding is decided. The budget's itemization — [`crate::cbrt`]'s,
//! since the composition is the same with `|n|` in place of 3 — lives
//! on `ladder::ROOTN`; `n = −2` carries `ladder::RSQRT` through its
//! delegation instead.
//!
//! With ADR-0060's exact integer adjudicator landed
//! (`adjudicate::rootn_side` on the rung 2 delivery; the `|n| = 6`
//! comparisons are what `U1024` exists for), the claim is
//! *unconditional* over `2 ≤ |n| ≤ 6` in every build: `|n| ≤ 2` by
//! its delegations (identity, division, the format's own square
//! root, and the `rsqrt` kernel), `3 ≤ |n| ≤ 6` by classification
//! plus the Liouville floors plus the adjudicator on the residual
//! path. Outside that range the operation carries the standing
//! Tier 1 / Tier 2 statement.
//!
//! ## Special values (IEEE 754-2019 §9.2.1)
//!
//! Reproduced verbatim from the standard's table, in
//! [`rootn_special_cases`]:
//!
//! * `rootn(±0, n)` is `±∞` and signals `divideByZero` for odd `n < 0`.
//! * `rootn(±0, n)` is `+∞` and signals `divideByZero` for even `n < 0`.
//! * `rootn(±0, n)` is `+0` for even `n > 0`.
//! * `rootn(±0, n)` is `±0` for odd `n > 0`.
//! * `rootn(+∞, n)` is `+∞` for `n > 0`.
//! * `rootn(−∞, n)` is `−∞` for odd `n > 0`.
//! * `rootn(−x, n)` is qNaN and signals `invalid` for even `n > 0`.
//! * `rootn(+∞, n)` is `+0` for `n < 0`.
//! * `rootn(−∞, n)` is `−0` for odd `n < 0`.
//! * `rootn(−x, n)` is qNaN and signals `invalid` for even `n < 0`.
//!
//! The standard's NOTE beside that table is a real behavioural
//! difference the tests pin: `rootn(−0, 2)` is `+0` (the even `n > 0`
//! row, which the zero rows reach before the negative-base rows),
//! while `squareRoot(−0)` is `−0` (§5.4.1 preserves the sign of a
//! zero). Two spellings of "the square root of minus zero", two
//! different answers, both mandated.

use crate::extended::ExtNum;
use crate::format::DecimalFormat;
use crate::ladder;
use crate::ln::ln_extended_body;
use core::cmp::Ordering;
use ferrodec_ieee::IeeeDecodedClass as Class;
use ferrodec_ieee::{RoundingMode, Status};

/// Apply IEEE 754-2019 §9.2.1's `rootn` table plus the two operand
/// cases the table leaves out, without touching the working-precision
/// path. `None` means "finite nonzero `x`, `n ≠ 0`, and the sign
/// combination is in the domain": the kernel's own territory.
///
/// Order is load bearing in two places. NaN comes first, so a NaN
/// operand propagates rather than colliding with the `n = 0` rule
/// (`rootn` has no `pow(x, ±0) = 1` style NaN-consuming row). Zero
/// comes before the negative-base rows, which is what makes
/// `rootn(−0, 2)` the `+0` of the even `n > 0` row instead of the
/// `invalid` of the negative-base row — the standard's NOTE.
///
/// `n = 0` is absent from the standard's table. IEEE 754-2019 leaves
/// operand cases it does not list to the implementation, and the
/// established practice is a quiet NaN with `invalid`: MPFR's
/// `mpfr_rootn` returns NaN for `n = 0`. This kernel does the same,
/// so the absence is documented rather than silently defaulted.
pub fn rootn_special_cases<F: DecimalFormat>(x: F, n: i32) -> Option<(F, Status)> {
    // NaN first: no row of the table consumes a NaN operand.
    match x.classify() {
        Class::SignalingNaN { .. } => return Some((x.nan_from(), Status::INVALID)),
        Class::QuietNaN { .. } => return Some((x, Status::OK)),
        _ => {}
    }
    if n == 0 {
        // Absent from the table; language-defined, and MPFR agrees.
        return Some((F::NAN, Status::INVALID));
    }
    // `n.unsigned_abs()` rather than `-n`: `i32::MIN` has no positive
    // counterpart, and it is an even, negative `n` the table covers.
    let odd = n % 2 != 0;
    let n_neg = n < 0;
    let x_neg = x.is_sign_negative();
    match x.classify() {
        Class::Zero { .. } => Some(match (n_neg, odd) {
            // "rootn(±0, n) is ±∞ and signals the divideByZero
            // exception for odd n < 0"
            (true, true) => (
                if x_neg { F::NEG_INFINITY } else { F::INFINITY },
                Status::DIV_BY_ZERO,
            ),
            // "rootn(±0, n) is +∞ and signals the divideByZero
            // exception for even n < 0"
            (true, false) => (F::INFINITY, Status::DIV_BY_ZERO),
            // "rootn(±0, n) is ±0 for odd n > 0"
            (false, true) => (if x_neg { F::NEG_ZERO } else { F::ZERO }, Status::OK),
            // "rootn(±0, n) is +0 for even n > 0" — and this row, not
            // the negative-base one, is what `rootn(−0, 2)` takes.
            (false, false) => (F::ZERO, Status::OK),
        }),
        Class::Infinity { .. } => {
            if x_neg && !odd {
                // "rootn(−x, n) is qNaN and signals the invalid
                // operation exception for even n": −∞ is a negative
                // operand like any other.
                return Some((F::NAN, Status::INVALID));
            }
            Some(match (n_neg, x_neg) {
                // "rootn(+∞, n) is +∞ for n > 0"
                (false, false) => (F::INFINITY, Status::OK),
                // "rootn(−∞, n) is −∞ for odd n > 0"
                (false, true) => (F::NEG_INFINITY, Status::OK),
                // "rootn(+∞, n) is +0 for n < 0"
                (true, false) => (F::ZERO, Status::OK),
                // "rootn(−∞, n) is −0 for odd n < 0"
                (true, true) => (F::NEG_ZERO, Status::OK),
            })
        }
        Class::Finite { .. } => {
            if x_neg && !odd {
                // "rootn(−x, n) is qNaN and signals the invalid
                // operation exception for even n > 0" (and the
                // even n < 0 row, identically).
                return Some((F::NAN, Status::INVALID));
            }
            None
        }
        // NaN handled above.
        Class::SignalingNaN { .. } | Class::QuietNaN { .. } => None,
    }
}

/// The `n`-th root of `x` (IEEE 754-2019 §9.2 `rootn`), rounded by
/// `rm`.
///
/// ## Exactness and ties (ADR-0059 classification leg)
///
/// Exactness and ties are decided from the inputs alone by the
/// decimal Lauter–Lefèvre criterion at `y = 1/n` (`|n| divides both
/// the 2-exponent and the 5-exponent of |x|`, and the 2,5-free part
/// is a perfect `|n|`-th power), including the `PRECISION + 1`
/// midpoints; the criterion and its per-bail completeness proofs live
/// on `exact::rootn_exact_input`. `n = ±1` and `n = 2` never consult
/// it: those arms delegate to §5 operations that are correctly
/// rounded, exact cases included. Past the classifier `x^(1/n)` is
/// irrational and the unconditional `INEXACT` is correct in every
/// mode.
///
/// ## Preferred exponent (IEEE 754-2019 §9.2.2)
///
/// The standard asks for `Q(rootn(x, n)) = floor(Q(x)/n)`. The
/// delegated arms deliver it exactly (identity, division, square
/// root). The classified and computed arms deliver the §6.3 quantum
/// the shared kernel rounder produces for every §9.2 operation in
/// this crate (preferred quantum 0), so an exact result with trailing
/// zeros lands in the cohort nearest quantum 0 rather than at
/// `floor(Q(x)/n)`: `rootn(1E+30, 5)` delivers `1000000` where
/// §9.2.2 asks for `1E+6`. Same value, different cohort; the
/// divergence is the shared rounder's, not this kernel's, and is
/// recorded here rather than patched locally.
pub fn rootn_kernel<F: DecimalFormat>(x: F, n: i32, rm: RoundingMode) -> (F, Status) {
    ladder::ladder_run!(|ex| rootn_kernel_body::<F, _>(ex, x, n, rm))
}

/// Generic body of [`rootn_kernel`] (M4, ADR-0059); `None` escalates
/// (M8 ladder). `ex` is the working-precision exemplar (M8b): the
/// receiver the constant and constructor surface reads its width
/// from, never a value the result depends on.
pub(crate) fn rootn_kernel_body<F: DecimalFormat, E: ExtNum>(
    ex: E,
    x: F,
    n: i32,
    rm: RoundingMode,
) -> Option<(F, Status)> {
    if let Some(early) = rootn_special_cases(x, n) {
        return Some(early);
    }
    // Past the special cases: `x` is finite and nonzero, `n ≠ 0`, and
    // either `x > 0` or `n` is odd.

    // The §5 delegations (ADR-0060's unconditional tier for |n| ≤ 2).
    match n {
        // Identity, quantum untouched — which is also §9.2.2's
        // `floor(Q(x)/1)`. No rounding happens, so no flag is raised.
        1 => return Some((x, Status::OK)),
        // Division is a §5.4.1 basic operation: correctly rounded in
        // every direction by the format's own verified rounder, with
        // the §6.3 preferred exponent `Q(1) − Q(x)`. It also carries
        // the sign and the over/underflow dispositions itself, so no
        // reflection and no gate are needed here.
        -1 => return Some(F::ONE.div_fmt(x, rm)),
        // Square root is a §5.4.1 basic operation on the same terms,
        // and `x > 0` here (a negative base with even `n` became NaN
        // above, zero was answered above). Its preferred exponent
        // `floor(Q(x)/2)` is exactly §9.2.2's value for `n = 2`, and
        // its exact cases (perfect squares) come out exact with clean
        // flags, so this arm needs no classifier.
        //
        // `sqrt_seed` is the seam's name for the format's own
        // `squareRoot`; its contract (not its name) is what this arm
        // leans on, and the trait method's rustdoc now says so. The
        // three format test files pin the delegation as a bit-for-bit
        // differential against `sqrt`, so a divergence breaks loudly.
        2 => return Some(x.sqrt_seed(rm)),
        // rSqrt: the direct Newton kernel ADR-0060 mandates, with
        // its own classifier (which owns the `rootn(2^2k, −2)` tie
        // family) and the RSQRT budget carrying the unconditional
        // claim. Escalation propagates through this body's own
        // ladder_run.
        -2 => return crate::rsqrt::rsqrt_kernel_body::<F, E>(ex, x, rm),
        _ => {}
    }

    // `rootn` is odd in `x` for odd `n`, so the pipeline works on
    // `|x|` and re-applies the sign. Rounding the magnitude and then
    // negating means the directed modes must be reflected first
    // (`cbrt`'s `for_negation` rule; fd-aqs.5): rounding `|rootn(x,n)|`
    // toward `+∞` and negating yields the result rounded toward `−∞`.
    // Only odd `n` reaches here with a negative `x`.
    let sign_neg = x.is_sign_negative();
    let eff_rm = if sign_neg { rm.for_negation() } else { rm };
    let abs_x = x.abs();

    // Input-side exact and tie classification (ADR-0059 M7). An exact
    // root (`rootn(8, 3) = 2`, `rootn(1E+30, 5) = 1E+6`) or a
    // `PRECISION + 1` midpoint is delivered from its exact
    // coefficient through the format rounder before any approximation
    // runs — every rounding direction correct, `INEXACT` clean on the
    // exact ones (§7.5). `|x| = 1` is inside this classifier for every
    // `n`, which the anchor arm below depends on.
    if let Some((mag, status)) = crate::exact::rootn_exact_input::<F>(abs_x, n, eff_rm) {
        return Some((if sign_neg { mag.neg() } else { mag }, status));
    }

    // The hug-at-1 anchor arm (ADR-0051 residual channel). Unguarded
    // by design: the anchor leg runs before the ladder's predicate,
    // because no finite rung separates a value this close to a grid
    // point — the theorem-backed side does.
    if let Some(above) = rootn_hug_one::<F, E>(ex, abs_x, n) {
        let (mag, status) = ex.one().to_format_with_residual::<F>(above, eff_rm);
        let mag = if sign_neg { mag.neg() } else { mag };
        return Some((mag, status | Status::INEXACT));
    }

    // General path: `exp(ln|x| / n)` at working precision, rounded
    // once at the format boundary under the ROOTN budget.
    let ln_x = ln_extended_body::<F, E>(ex, abs_x);
    let mut arg = ln_x.div_u32(n.unsigned_abs());
    if n < 0 {
        // `x^(1/n) = 1 / x^(1/|n|)`, i.e. `exp(−ln|x|/|n|)`. Negating
        // the argument is exact, so the reciprocal costs nothing.
        arg = arg.neg();
    }
    // On a rung 2 ambiguity the decider settles the side by the exact
    // q-th power comparison (ADR-0060); `3 ≤ |n| ≤ 6` is this
    // kernel's adjudicable range (`|n| ≤ 2` delegated above).
    let (mut result, status) =
        crate::exp::exp_from_extended_body_adjudicated::<F, E>(arg, eff_rm, &ladder::ROOTN, |b| {
            crate::adjudicate::rootn_side(abs_x, n, b)
        })?;
    if sign_neg {
        result = result.neg();
    }
    // `exp_from_extended_body` raised INEXACT, and here that is
    // correct unconditionally: the exact values and the ties returned
    // above, and `exact::rootn_exact_input` proves everything past it
    // irrational.
    Some((result, status))
}

/// The hug-at-1 arm's gate and side: `Some(above)` when the true
/// value provably lies strictly between 1 and the first rounding
/// boundary beside it, with `above` saying which side of 1 it sits
/// on; `None` falls through to the general path. The caller turns a
/// `Some` into an ADR-0051 residual delivery anchored at 1.
///
/// Caller guarantees `abs_x` is finite, positive, `≠ 1` (the
/// classifier answers `|x| = 1` for every `n`), and `|n| ≥ 2`.
///
/// ## Why the arm exists
///
/// `rootn(x, n) − 1 ≈ ln(x)/n`, so a large `|n|` pins the true value
/// arbitrarily close to 1 — at `Decimal128` the closest reachable is
/// `|ln(1 − 10^−34)| / 2^31 ≈ 4.7·10^−44` relative, nine decades
/// inside the nearest rounding boundary. That is the D1/D2
/// integer-anchor lesson in this operation's costume: the working
/// value's distance from the grid point 1 shrinks with `|n|` while no
/// rung's own resolution follows it, so the directed modes end up
/// decided by how much room the rung happens to have left rather than
/// by a theorem. Decided here instead, input side, the answer is the
/// same at every rung and every build.
///
/// ## The side theorem (strict, hence usable by the directed modes)
///
/// > For `x > 0`, `x ≠ 1`, and `n ≠ 0`:
/// > `rootn(x, n) > 1` iff `(x > 1) XOR (n < 0)`, strictly.
///
/// Proof: `rootn(x, n) = e^(ln(x)/n)` and `t ↦ e^t` is strictly
/// increasing with `e^0 = 1`, so `rootn(x, n) > 1` iff `ln(x)/n > 0`.
/// `ln` is strictly increasing with `ln 1 = 0`, so `ln(x) > 0` iff
/// `x > 1`, and `ln(x) ≠ 0` because `x ≠ 1`. The quotient is positive
/// exactly when its two factors share a sign, which is the stated
/// exclusive-or. Equality is impossible: `ln(x)/n ≠ 0`. ∎
///
/// ## The threshold, and its soundness
///
/// The grid is coarser above 1 than below it, because 1 is a decade
/// point: the spacing above is `10^−(P−1)` and below is `10^−P`, so
/// the two boundaries beside 1 are the midpoints `1 + 5·10^−P` and
/// `1 − 5·10^−(P+1)`. The nearer is the lower one, at relative
/// distance `5·10^−(P+1)`, so a true value within `5·10^−(P+2)` of 1
/// — a tenth of that, the mandated ×10 margin — lies strictly between
/// 1 and both boundaries, and every rounding direction's answer is
/// the anchor's: the nearest modes and the direction away from the
/// residual deliver 1, the direction toward it delivers 1's
/// neighbour on that side, all `INEXACT`.
///
/// The arm fires on a *sound upper bound* `B ≥ |ln|x||` computed from
/// the input alone, so `B/|n| ≤ 5·10^−(P+2)` implies
/// `|ε| = |ln|x||/|n| ≤ 5·10^−(P+2)` and then
/// `|rootn(x,n) − 1| = |e^ε − 1| ≤ |ε|·e^|ε| < 1.000001·|ε|`, inside
/// the margin with the whole ×10 to spare. `B` comes in three
/// regimes, selected by `|x|`'s decade exponent `adj`
/// (`10^adj ≤ |x| < 10^(adj+1)`):
///
/// * `adj = 0` (`1 ≤ |x| < 10`): `B = |x| − 1`, from `ln v ≤ v − 1`
///   for `v ≥ 1` (the tangent at 1 lies above a concave function).
/// * `adj = −1` (`0.1 ≤ |x| < 1`): `B = 10·(1 − |x|)`, from
///   `−ln v = ∫_v^1 dt/t ≤ (1 − v)/v` and `v ≥ 0.1`.
/// * otherwise: `B = (|adj| + 1)·2.303`, from
///   `|ln|x|| ≤ (|adj| + 1)·ln 10` and `ln 10 < 2.303`.
///
/// Both subtractions are exact at every rung width: in those two
/// decades `|x|` carries at most `PRECISION` significant digits at a
/// quantum no finer than `10^−(PRECISION+1)`, so the aligned
/// difference needs at most `PRECISION + 2 ≤ 36` digits against the
/// rung's 50. The scaling by 10 and by `10^−3` are exponent shifts,
/// also exact. The one rounding is the closing `div_u32` by `|n|`
/// (≤ 10^−49 relative), thirty orders below the margin it is compared
/// against — and where the two rungs could in principle disagree on
/// the comparison, both deliveries round identically anyway, which is
/// what the seam-continuity test pins.
///
/// `B = 0` would break the strict side theorem, and cannot happen:
/// `B = 0` requires `|x| = 1`, which the classifier answered before
/// this arm runs.
fn rootn_hug_one<F: DecimalFormat, E: ExtNum>(ex: E, abs_x: F, n: i32) -> Option<bool> {
    let v = ex.from_format(abs_x); // exact
    let adj = v.exponent() + v.digit_count() as i32 - 1;
    let bound = if adj == 0 {
        v.sub(ex.one())
    } else if adj == -1 {
        ex.one().sub(v).mul_pow10_exp(1)
    } else {
        // (|adj| + 1) · 2.303, assembled as an integer times 10^−3 so
        // no constant rounding enters the bound. `|adj| ≤ 6176` at the
        // widest format, so the product stays under 1.5·10^7 and the
        // saturating steps never engage; were they to, they would
        // only enlarge `B`, which keeps the gate conservative.
        let decades = adj.saturating_abs().saturating_add(1);
        ex.from_i32(decades.saturating_mul(2303)).mul_pow10_exp(-3)
    };
    // The ×10 margin: half the last-place spacing beside 1 is
    // `5·10^−(P+1)` relative, and the gate admits a tenth of it.
    let threshold = ex.from_i32(5).mul_pow10_exp(-(F::PRECISION as i32) - 2);
    if bound.div_u32(n.unsigned_abs()).cmp(threshold) == Ordering::Greater {
        return None;
    }
    // The side theorem. `v > 1` and `n < 0` are both strict, and the
    // caller has ruled out `v = 1`.
    Some((v.cmp(ex.one()) == Ordering::Greater) != (n < 0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock_format::ValueFmt128;

    // The in-crate mock has no NaN or Infinity encoding (`classify`
    // yields only `Zero` and `Finite`, and `is_nan` / `is_infinite`
    // are `unreachable!`), so these unit tests cover the rows whose
    // verdict is visible in the status and the sign alone. The three
    // integration files (`tests/transcend_exact_rootn.rs` and the
    // sibling mirrors) carry every table row on the real formats,
    // where NaN and ±∞ are representable.

    fn v(coef: u128, exp: i32, sign: bool) -> ValueFmt128 {
        ValueFmt128 { coef, exp, sign }
    }

    /// The `n = 0` case the standard's table leaves out: `INVALID`
    /// for every operand class the mock can name.
    #[test]
    fn zero_n_is_invalid() {
        for x in [v(1, 0, false), v(1, 0, true), v(0, 0, false), v(0, 0, true)] {
            let (_, st) =
                rootn_special_cases::<ValueFmt128>(x, 0).expect("n = 0 is a special case");
            assert_eq!(st, Status::INVALID, "rootn(x, 0) must signal INVALID");
        }
    }

    /// `i32::MIN` is an even negative `n`: the parity and sign tests
    /// must not reach for `-n`, which would overflow.
    #[test]
    fn i32_min_is_an_even_negative_n() {
        let n = i32::MIN;
        assert_eq!(n % 2, 0);
        assert_eq!(n.unsigned_abs(), 2_147_483_648);
        // rootn(±0, even n < 0) is +∞ with DIV_BY_ZERO, both zero
        // signs (the sign of the zero does not reach the result).
        for sign in [false, true] {
            let (r, st) = rootn_special_cases::<ValueFmt128>(v(0, 0, sign), n)
                .expect("zero is a special case");
            assert!(!r.is_sign_negative(), "even n < 0 delivers +∞");
            assert_eq!(st, Status::DIV_BY_ZERO);
        }
        // rootn(negative finite, even n) is qNaN + INVALID.
        let (_, st) =
            rootn_special_cases::<ValueFmt128>(v(2, 0, true), n).expect("negative base, even n");
        assert_eq!(st, Status::INVALID);
    }

    /// The standard's NOTE, at the special-case layer: `rootn(−0, 2)`
    /// takes the even `n > 0` zero row (`+0`), not the negative-base
    /// row, so it differs from `squareRoot(−0) = −0`.
    #[test]
    fn negative_zero_second_root_is_positive_zero() {
        let (r, st) =
            rootn_special_cases::<ValueFmt128>(v(0, 0, true), 2).expect("zero is a special case");
        assert!(
            r.is_zero() && !r.is_sign_negative(),
            "rootn(−0, 2) must be +0"
        );
        assert_eq!(st, Status::OK);
        // The odd companion keeps the sign: rootn(−0, 3) is −0.
        let (r, st) =
            rootn_special_cases::<ValueFmt128>(v(0, 0, true), 3).expect("zero is a special case");
        assert!(
            r.is_zero() && r.is_sign_negative(),
            "rootn(−0, 3) must be −0"
        );
        assert_eq!(st, Status::OK);
    }

    /// The hug-at-1 gate at `Decimal128`'s shape, on the arithmetic
    /// alone (the delivery needs a real format rounder, which the
    /// integration files exercise). Threshold `5·10^−36`; bound
    /// `|x| − 1` in the `adj = 0` decade. `n = 2^31 − 1` admits
    /// `|x| − 1 ≤ 1.07·10^−26`, so `10^−26` fires and `2·10^−26` does
    /// not — the seam-continuity pair the integration tests then show
    /// rounds identically on both sides.
    #[test]
    fn hug_one_gate_straddles_its_threshold() {
        let ex = crate::extended::Extended::ZERO;
        // 1 + 10^−26 and 1 + 2·10^−26, both at 27 significant digits.
        let inside = v(100_000_000_000_000_000_000_000_001, -26, false);
        let outside = v(100_000_000_000_000_000_000_000_002, -26, false);
        assert_eq!(
            rootn_hug_one::<ValueFmt128, _>(ex, inside, i32::MAX),
            Some(true),
            "1 + 1e-26 at n = i32::MAX is inside the gate, above 1"
        );
        assert_eq!(
            rootn_hug_one::<ValueFmt128, _>(ex, outside, i32::MAX),
            None,
            "1 + 2e-26 at n = i32::MAX is outside the gate"
        );
        // A modest `n` never reaches the gate at this width.
        assert_eq!(rootn_hug_one::<ValueFmt128, _>(ex, inside, 3), None);
    }

    /// The side theorem's truth table: `rootn(x, n) > 1` iff
    /// `(x > 1) XOR (n < 0)`. Both decades adjacent to 1 are covered,
    /// since they take different regimes of the bound.
    #[test]
    fn hug_one_side_follows_the_theorem() {
        let ex = crate::extended::Extended::ZERO;
        // 1 + 10^−26 (adj = 0, bound `v − 1`) and 1 − 10^−27
        // (adj = −1, bound `10(1 − v)` — the extra decade is why the
        // two sides need different distances to clear the same gate).
        let above_one = v(100_000_000_000_000_000_000_000_001, -26, false);
        let below_one = v(999_999_999_999_999_999_999_999_999, -27, false);
        for (x, x_gt_1) in [(above_one, true), (below_one, false)] {
            for n in [i32::MAX, i32::MIN] {
                let want = x_gt_1 != (n < 0);
                assert_eq!(
                    rootn_hug_one::<ValueFmt128, _>(ex, x, n),
                    Some(want),
                    "side theorem at n = {n}, x > 1 = {x_gt_1}"
                );
            }
        }
    }

    /// The far decades take the `(|adj| + 1)·2.303` regime, whose
    /// bound is never small enough at `Decimal128`'s 34 digits: an
    /// `|ln x| ≥ ln 10` divided by at most `2^31` still exceeds
    /// `5·10^−36` by twenty-six orders.
    #[test]
    fn hug_one_gate_is_closed_away_from_one() {
        let ex = crate::extended::Extended::ZERO;
        for (coef, exp) in [(2u128, 0i32), (5, -1), (1, 6000), (1, -6000)] {
            assert_eq!(
                rootn_hug_one::<ValueFmt128, _>(ex, v(coef, exp, false), i32::MAX),
                None,
                "{coef}e{exp} must not reach the Decimal128 gate"
            );
        }
    }

    /// A finite nonzero operand in the domain falls through to the
    /// kernel, for both parities and both signs of `n`; a negative
    /// base with even `n` does not.
    #[test]
    fn in_domain_finite_falls_through() {
        for n in [-7, -3, -2, -1, 1, 2, 3, 7] {
            assert!(rootn_special_cases::<ValueFmt128>(v(8, 0, false), n).is_none());
        }
        for n in [-7, -3, -1, 1, 3, 7] {
            assert!(rootn_special_cases::<ValueFmt128>(v(8, 0, true), n).is_none());
        }
        for n in [-8, -2, 2, 8] {
            let (_, st) = rootn_special_cases::<ValueFmt128>(v(8, 0, true), n)
                .expect("even n, negative base");
            assert_eq!(st, Status::INVALID);
        }
    }
}
