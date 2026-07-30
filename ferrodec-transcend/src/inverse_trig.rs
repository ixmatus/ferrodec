//! Moved from `ferrodec/src/math/inverse_trig.rs` @ commit 82a7fe1
//! (P0a.2 c9). Behaviour-neutral: genericized over [`DecimalFormat`];
//! the `Decimal128` instantiation is byte-identical to the pre-move
//! kernel.
//!
//! `atan(x)` and friends — `asin`, `acos`, `atan2`.
//!
//! ## atan algorithm
//!
//! Two stages of argument reduction bring `|x|` into
//! `[0, tan(π/8)] ≈ [0, 0.4142]`:
//!
//! 1. **Inversion**: `atan(x) = ±π/2 − atan(1/x)` for `|x| > 1`.
//! 2. **π/4 shift**: `atan(x) = π/4 + atan((x−1)/(x+1))` for
//!    `|x| > tan(π/8)`. The shifted argument is in
//!    `[−tan(π/8), 0]` for the original input range
//!    `(tan(π/8), 1]`.
//!
//! After reduction, the Taylor series
//! `atan(y) = y − y³/3 + y⁵/5 − y⁷/7 + …` converges in ≤ 200
//! iterations for `|y| ≤ tan(π/8)` (`0.414^200 ≈ 10^{-77}` —
//! comfortably past `EXT_PRECISION` = 50). Sign of `x` is folded back
//! at the end (`atan` is odd).
//!
//! ## asin / acos
//!
//! `asin(x) = 2 · atan(x / (1 + sqrt(1 − x²)))` with the radicand
//! factored exactly as `(1 − |x|)(1 + |x|)` (ADR-0050: squaring
//! first leaves an absolute rounding residue that dominates the
//! small radicand near `|x| = 1`).
//! `acos(x) = 2 · atan(sqrt((1 − x) / (1 + x)))` across the open
//! domain (ADR-0050: the previous `π/2 − asin(x)` form cancelled
//! catastrophically near `x = 1`, where the result is far smaller
//! than either operand).
//!
//! ## atan2
//!
//! Quadrant dispatch as IEEE 754-2019 §9.2.1 specifies, plus the
//! `atan2(±0, ±0)` corner cases.
//!
//! ## Accuracy
//!
//! Correctly rounded across each function's domain (ADR-0032;
//! supersedes ADR-0024's faithful contract). The worst case half
//! ULP margins per format precision are:
//!
//! - `atan`: `6.577106e-09` at `Decimal32` (ADR-0033 Plan C4
//!   exhaustive sweep at input `-29.33065`;
//!   `tests/vectors/transcend/exhaustive/atan.txt`), `4.038e-4` at
//!   `Decimal64`, `1.242e-3` at `Decimal128`.
//! - `asin`: `1.138763e-08` at `Decimal32` (Plan C4 exhaustive at
//!   `6.694329e-4`), `3.553e-4` at `Decimal64`, `1.052e-3` at
//!   `Decimal128`.
//! - `acos`: `2.328715e-09` at `Decimal32` (Plan C4 exhaustive at
//!   `8.288267e-4`), `7.313e-4` at `Decimal64`, `6.409e-3` at
//!   `Decimal128`.
//! - `atan2`: `1.602e-3` at `Decimal32`, `3.017e-3` at `Decimal64`,
//!   `3.701e-3` at `Decimal128` (all sampled corpus minima from
//!   `tests/vectors/transcend/atan2.prov`, ADR-0026 fd-97a; `atan2`
//!   is binary and was excluded from the ADR-0033 Plan C4 unary
//!   exhaustive sweep per the §Rejected alternatives — its 10^16
//!   canonical Decimal32 input pair cardinality is beyond
//!   exhaustive reach and it stays on the sampled corpus path).
//!
//! The `Decimal32` figures for the unary functions are proven
//! correctly rounded across the full canonical input set by Arb;
//! the `Decimal64` and `Decimal128` figures are sampled corpus
//! minima from `tests/vectors/transcend/{atan,asin,acos}.prov`
//! (ADR-0026 fd-97a) under the ADR-0033 Slice A corpus integrity
//! discipline. For `asin` and `acos` the margin-to-every-input
//! inference additionally relies on the relative error model the
//! factored radicand and the direct `acos` form restore (ADR-0050;
//! the 2026-06-09 review measured up to ~1.5e6 ULP for `acos` near
//! 1 at `Decimal128` under the previous forms, and the band corpus
//! `tests/vectors/transcend/anchor_bands/` is the standing witness).
//!
//! At 50 digit kernel working precision, the cumulative two stage
//! argument reduction and Taylor series error (`atan`) and the
//! composition error (`asin`, `acos`, `atan2`) clears the smallest
//! margin by more than thirty orders of magnitude on every format.
//! `asin` uses the `2 · atan(x / (1 + sqrt(1 − x²)))` form with the
//! radicand factored exactly, and `acos` the direct
//! `2 · atan(sqrt((1 − x)/(1 + x)))` form, so the cancellations
//! that would otherwise break the bound near `|x| = 1` are
//! structurally absent (ADR-0050).
//!
//! None of `atan`, `asin`, `acos` has a TMD hard candidate in the
//! Plan C4 enumeration. `asin(0) = 0`, `atan(0) = 0`, `acos(1) = 0`
//! are all exact representable outputs that would hit the
//! Arb-ball-spans-zero pattern, but `asin` and `atan` skip
//! coef = 0 in the canonical enumeration and `acos`'s UNIT domain
//! is strictly `|x| < 1`, so `x = 1` is not tested. (Compare
//! `acosh` in [`crate::hyperbolic`], whose domain `x ≥ 1`
//! includes `x = 1` and which does hit the TMD hard pattern.)
//!
//! The shared error model lives in ADR-0032 §Decision; the sampled
//! corpus test, the ADR-0033 exhaustive worst case kernel
//! verification gate
//! (`ferrodec-decimal32/tests/transcend_vectors_exhaustive.rs`,
//! 18/18 exact), and the MPFR cross-validation gate
//! (`ferrodec-test-support/tests/mpfr_gate.rs`, 0 disagreements)
//! are the empirical witnesses.

use crate::extended::{ExtNum, Extended};
use crate::format::DecimalFormat;
use ferrodec_ieee::IeeeDecodedClass as Class;
use ferrodec_ieee::{RoundingMode, Status};

/// Inverse tangent. Range `(-π/2, +π/2)`.
pub fn atan_kernel<F: DecimalFormat>(x: F, rm: RoundingMode) -> (F, Status) {
    atan_kernel_body::<F, Extended>(x, rm)
}

/// Generic body of [`atan_kernel`] (M4, ADR-0059).
pub(crate) fn atan_kernel_body<F: DecimalFormat, E: ExtNum>(x: F, rm: RoundingMode) -> (F, Status) {
    match x.classify() {
        Class::SignalingNaN { .. } => return (x.nan_from(), Status::INVALID),
        Class::QuietNaN { .. } => return (x, Status::OK),
        Class::Infinity { sign } => {
            // atan(−∞) = −π/2: round the magnitude under the
            // negation-reflected mode before flipping the sign, so the
            // two directed modes land on the correct neighbour (the
            // cbrt `for_negation` rule; fd-aqs.5).
            let rm_mag = if sign { rm.for_negation() } else { rm };
            let half_pi = E::pi_over_two().to_format::<F>(0, rm_mag).0;
            return (if sign { half_pi.neg() } else { half_pi }, Status::INEXACT);
        }
        Class::Zero { .. } => return (x, Status::OK),
        Class::Finite { .. } => {}
    }
    let x_ext = E::from_format(x);
    let result_ext = atan_ext::<F, E>(x_ext);
    // Grid-stuck at the input (ADR-0051): a small argument absorbs
    // every correction and the result is exactly `x`; the directed
    // modes need the side, and `|atan x| < |x|` is a theorem.
    if result_ext.sticks_to(x_ext) {
        let (result, status) = x_ext.to_format_with_residual::<F>(false, rm);
        return (result, status | Status::INEXACT);
    }
    let (result, status) = result_ext.to_format::<F>(0, rm);
    (result, status | Status::INEXACT)
}

/// Inverse sine. Domain `[-1, +1]`; outside is NaN + INVALID.
/// Range `[-π/2, +π/2]`.
pub fn asin_kernel<F: DecimalFormat>(x: F, rm: RoundingMode) -> (F, Status) {
    asin_kernel_body::<F, Extended>(x, rm)
}

/// Generic body of [`asin_kernel`] (M4, ADR-0059).
pub(crate) fn asin_kernel_body<F: DecimalFormat, E: ExtNum>(x: F, rm: RoundingMode) -> (F, Status) {
    match x.classify() {
        Class::SignalingNaN { .. } => return (x.nan_from(), Status::INVALID),
        Class::QuietNaN { .. } => return (x, Status::OK),
        Class::Infinity { .. } => return (F::NAN, Status::INVALID),
        Class::Zero { .. } => return (x, Status::OK),
        Class::Finite { .. } => {}
    }
    let abs_x = x.abs();
    let (cmp_one, _) = abs_x.partial_cmp_fmt(F::ONE);
    match cmp_one {
        Some(core::cmp::Ordering::Greater) => return (F::NAN, Status::INVALID),
        Some(core::cmp::Ordering::Equal) => {
            // asin(±1) = ±π/2. Round the magnitude under the
            // negation-reflected mode before flipping the sign
            // (fd-aqs.5).
            let neg = x.is_sign_negative();
            let rm_mag = if neg { rm.for_negation() } else { rm };
            let half_pi = E::pi_over_two().to_format::<F>(0, rm_mag).0;
            let signed = if neg { half_pi.neg() } else { half_pi };
            return (signed, Status::INEXACT);
        }
        _ => {}
    }
    let x_ext = E::from_format(x);
    let result_ext = asin_ext::<F, E>(x_ext);
    // Grid-stuck at the input (ADR-0051): `|asin x| > |x|` is a
    // theorem, so the residual side is the growing one.
    if result_ext.sticks_to(x_ext) {
        let (result, status) = x_ext.to_format_with_residual::<F>(true, rm);
        return (result, status | Status::INEXACT);
    }
    let (result, status) = result_ext.to_format::<F>(0, rm);
    (result, status | Status::INEXACT)
}

/// Inverse cosine. Domain `[-1, +1]`; outside is NaN + INVALID.
/// Range `[0, π]`.
pub fn acos_kernel<F: DecimalFormat>(x: F, rm: RoundingMode) -> (F, Status) {
    acos_kernel_body::<F, Extended>(x, rm)
}

/// Generic body of [`acos_kernel`] (M4, ADR-0059).
pub(crate) fn acos_kernel_body<F: DecimalFormat, E: ExtNum>(x: F, rm: RoundingMode) -> (F, Status) {
    match x.classify() {
        Class::SignalingNaN { .. } => return (x.nan_from(), Status::INVALID),
        Class::QuietNaN { .. } => return (x, Status::OK),
        Class::Infinity { .. } => return (F::NAN, Status::INVALID),
        Class::Zero { .. } => {
            let half_pi = E::pi_over_two().to_format::<F>(0, rm).0;
            return (half_pi, Status::INEXACT);
        }
        Class::Finite { .. } => {}
    }
    let abs_x = x.abs();
    let (cmp_one, _) = abs_x.partial_cmp_fmt(F::ONE);
    match cmp_one {
        Some(core::cmp::Ordering::Greater) => return (F::NAN, Status::INVALID),
        Some(core::cmp::Ordering::Equal) => {
            // acos(1) = 0; acos(-1) = π.
            if x.is_sign_negative() {
                let pi_d = E::pi().to_format::<F>(0, rm).0;
                return (pi_d, Status::INEXACT);
            }
            return (F::ZERO, Status::OK);
        }
        _ => {}
    }
    let x_ext = E::from_format(x);
    // acos(x) = 2 · atan(sqrt((1 − x) / (1 + x))) (fd-aqs.6). The
    // previous `π/2 − asin(x)` form cancelled catastrophically for
    // x near 1, where the result `≈ sqrt(2(1−x))` is tiny against
    // two ~π/2-magnitude operands each carrying ~1e-49 absolute
    // rounding error (up to ~1.5e6 ULP measured at Decimal128 by
    // the 2026-06-09 review). Here both factors are exact for
    // format-sourced coefficients — `1 − x` near x = 1 and `1 + x`
    // near x = −1 cancel exactly — and `atan` preserves relative
    // accuracy at both ends (small-argument Taylor on one side, the
    // `π/2 − atan(1/t)` inversion against a result of comparable
    // magnitude on the other), so the result is relative-accurate
    // across the whole open domain. (cos(2·atan t) with
    // t² = (1−x)/(1+x) reduces to x exactly, so the identity is the
    // same function.)
    let num = E::ONE.sub(x_ext);
    let den = E::ONE.add(x_ext);
    let t = num.div::<F>(den).sqrt::<F>();
    let half = atan_ext::<F, E>(t);
    let result_ext = half.add(half);
    let (result, status) = result_ext.to_format::<F>(0, rm);
    (result, status | Status::INEXACT)
}

/// Two-argument arctangent `atan2(y, x)`. Range `(-π, +π]`.
/// Quadrant per IEEE 754-2019 §9.2.1.
pub fn atan2_kernel<F: DecimalFormat>(y: F, x: F, rm: RoundingMode) -> (F, Status) {
    atan2_kernel_body::<F, Extended>(y, x, rm)
}

/// Generic body of [`atan2_kernel`] (M4, ADR-0059).
pub(crate) fn atan2_kernel_body<F: DecimalFormat, E: ExtNum>(
    y: F,
    x: F,
    rm: RoundingMode,
) -> (F, Status) {
    // NaN propagation (sNaN raises INVALID).
    if y.is_signaling_nan() || x.is_signaling_nan() {
        return (y.propagate_nan2(x), Status::INVALID);
    }
    if y.is_nan() || x.is_nan() {
        return (y.propagate_nan2(x), Status::OK);
    }
    let y_neg = y.is_sign_negative();
    // Round a positive π-family constant at the point of use, carrying
    // y's sign. The magnitude is rounded under the negation-reflected
    // mode when the result will be negated, so the two directed modes
    // land on the correct neighbour (the cbrt `for_negation` rule;
    // fd-aqs.5). Rounding eagerly under the caller's `rm` and negating
    // afterwards — the previous shape — was wrong by one ULP for
    // negative `y` at `TowardPositive` / `TowardNegative`.
    let signed_const = |c: E| {
        if y_neg {
            c.to_format::<F>(0, rm.for_negation()).0.neg()
        } else {
            c.to_format::<F>(0, rm).0
        }
    };

    // Inf handling.
    if x.is_infinite() && y.is_infinite() {
        // ±π/4 or ±3π/4 depending on signs.
        return if x.is_sign_negative() {
            let three_quarter_pi = E::pi_over_four().mul(E::from_i32(3));
            (signed_const(three_quarter_pi), Status::INEXACT)
        } else {
            (signed_const(E::pi_over_four()), Status::INEXACT)
        };
    }
    if y.is_infinite() {
        // ±π/2.
        return (signed_const(E::pi_over_two()), Status::INEXACT);
    }
    if x.is_infinite() {
        return if x.is_sign_negative() {
            (signed_const(E::pi()), Status::INEXACT)
        } else {
            (if y_neg { F::NEG_ZERO } else { F::ZERO }, Status::OK)
        };
    }
    // Both finite. Cover x = 0.
    if x.is_zero() {
        if y.is_zero() {
            // atan2(±0, +0) = ±0; atan2(±0, -0) = ±π. The ±π result is
            // a rounded irrational and raises INEXACT like the finite
            // x < 0 arm below (fd-aqs.5 flag-fidelity fix; the zero
            // result is exact and stays OK).
            if x.is_sign_negative() {
                return (signed_const(E::pi()), Status::INEXACT);
            }
            return (if y_neg { F::NEG_ZERO } else { F::ZERO }, Status::OK);
        }
        return (signed_const(E::pi_over_two()), Status::INEXACT);
    }
    if y.is_zero() {
        // atan2(±0, x): 0 if x > 0, ±π if x < 0.
        return if x.is_sign_negative() {
            (signed_const(E::pi()), Status::INEXACT)
        } else {
            (if y_neg { F::NEG_ZERO } else { F::ZERO }, Status::OK)
        };
    }
    // Both finite non-zero. Compute y/x at working precision, run
    // atan, then quadrant-shift.
    let y_ext = E::from_format(y);
    let x_ext = E::from_format(x);
    let q = y_ext.div::<F>(x_ext);
    let mut result_ext = atan_ext::<F, E>(q);
    if x.is_sign_negative() {
        // atan2 in quadrants 2 / 3: shift by ±π.
        if y_neg {
            result_ext = result_ext.sub(E::pi());
        } else {
            result_ext = result_ext.add(E::pi());
        }
    }
    let (result, status) = result_ext.to_format::<F>(0, rm);
    (result, status | Status::INEXACT)
}

/// `atan(x)` at working precision. Pre-conditions: `x` is finite
/// and non-zero (zero handled in the caller's special-case path).
fn atan_ext<F: DecimalFormat, E: ExtNum>(x: E) -> E {
    let neg = x.sign();
    let mut t = x.abs();
    let mut shift = E::ZERO;

    // Stage 1: |x| > 1 → atan(x) = π/2 - atan(1/x) (with sign).
    let mut inverted = false;
    if t.cmp(E::ONE) == core::cmp::Ordering::Greater {
        t = t.recip::<F>();
        inverted = true;
    }

    // Stage 2: tan(π/8) < |x| ≤ 1 → atan(x) = π/4 + atan((x-1)/(x+1)).
    let tan_eighth = E::tan_pi_over_eight();
    if t.cmp(tan_eighth) == core::cmp::Ordering::Greater {
        let num = t.sub(E::ONE);
        let den = t.add(E::ONE);
        t = num.div::<F>(den); // signed: in [-tan(π/8), 0]
        shift = E::pi_over_four();
    }

    // Taylor: atan(t) = t - t³/3 + t⁵/5 - t⁷/7 + …
    let mut sum = t;
    let mut t_pow = t; // t^(2k+1); initially t^1
    let t_sq = t.square();
    let mut alt = true; // next term subtracts
    for n in 1u32..=E::ATAN_SERIES_TERMS {
        t_pow = t_pow.mul(t_sq);
        let denom = 2 * n + 1;
        let term = t_pow.div_u32(denom);
        let signed_term = if alt { term.neg() } else { term };
        let next_sum = sum.add(signed_term);
        alt = !alt;
        if next_sum.cmp(sum) == core::cmp::Ordering::Equal {
            sum = next_sum;
            break;
        }
        sum = next_sum;
        if t_pow.is_zero() {
            break;
        }
    }

    // Apply Stage 2 shift.
    let mut result = if shift.is_zero() { sum } else { sum.add(shift) };
    // Apply Stage 1 inversion.
    if inverted {
        result = E::pi_over_two().sub(result);
    }
    // Apply original sign.
    if neg {
        result = result.neg();
    }
    result
}

/// `asin(x)` at extended precision for `|x| < 1`. Uses
/// `2 · atan(x / (1 + sqrt(1 - x²)))` with the radicand factored
/// exactly as `(1 − |x|)(1 + |x|)` — numerically stable across the
/// full domain (no blow-up and no absolute-error residue at
/// `|x| = 1`; ADR-0050).
fn asin_ext<F: DecimalFormat, E: ExtNum>(x: E) -> E {
    if x.is_zero() {
        return x;
    }
    // `1 − x²` factored as `(1 − |x|)(1 + |x|)` (fd-aqs.6): squaring
    // first rounds `x²` at 50 significant digits, and for `|x|` near
    // 1 the subsequent subtraction turns that absolute ~1e-50 residue
    // into a *relative* error of the small radicand — ~1e-50/(2δ)
    // for `|x| = 1 − δ` — which the 2026-06-09 review measured
    // breaching the proof envelope. The factors are exact for
    // format-sourced coefficients (leading-digit cancellation only
    // shortens `1 − |x|`), so the product, and everything downstream,
    // stays relative-accurate.
    let abs_x = x.abs();
    let one_minus_x_sq = E::ONE.sub(abs_x).mul(E::ONE.add(abs_x));
    let sqrt_term = one_minus_x_sq.sqrt::<F>();
    let denom = E::ONE.add(sqrt_term);
    let inner = x.div::<F>(denom);
    let half_atan = atan_ext::<F, E>(inner);
    half_atan.add(half_atan)
}
