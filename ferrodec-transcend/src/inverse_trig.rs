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
//! `asin(x) = atan(x / sqrt(1 − x²))` near zero; uses the numerically-
//! stable `2 · atan(x / (1 + sqrt(1 − x²)))` form near `|x| = 1`.
//! `acos(x) = π/2 − asin(x)`.
//!
//! ## atan2
//!
//! Quadrant dispatch as IEEE 754-2019 §9.2.1 specifies, plus the
//! `atan2(±0, ±0)` corner cases.
//!
//! ## Accuracy
//!
//! Correctly rounded across each function's domain (ADR-0032;
//! supersedes ADR-0024's faithful contract). The Arb empirical
//! worst case half ULP margins from the per function provenance
//! files (ADR-0026, fd-97a) are:
//!
//! - `atan.prov`: `1.177e-2` at `Decimal32`, `4.038e-4` at
//!   `Decimal64`, `1.242e-3` at `Decimal128`.
//! - `asin.prov`: `8.427e-4` at `Decimal32`, `3.553e-4` at
//!   `Decimal64`, `1.052e-3` at `Decimal128`.
//! - `acos.prov`: `9.306e-4` at `Decimal32`, `7.313e-4` at
//!   `Decimal64`, `6.409e-3` at `Decimal128`.
//! - `atan2.prov`: `1.602e-3` at `Decimal32`, `3.017e-3` at
//!   `Decimal64`, `3.701e-3` at `Decimal128`.
//!
//! At 50 digit kernel working precision, the cumulative two stage
//! argument reduction and Taylor series error (`atan`) and the
//! composition error (`asin`, `acos`, `atan2`) clears the smallest
//! margin by more than thirty orders of magnitude on every format.
//! `asin` near `|x| = 1` uses the numerically stable
//! `2 · atan(x / (1 + sqrt(1 − x²)))` form so the cancellation
//! that would otherwise tighten the bound is structurally absent.
//! The shared error model lives in ADR-0032 §Decision; the corpus
//! test is the standing empirical witness.

use crate::consts::{pi_ext, pi_over_four_ext, pi_over_two_ext, tan_pi_over_eight_ext};
use crate::extended::Extended;
use crate::format::DecimalFormat;
use ferrodec_ieee::IeeeDecodedClass as Class;
use ferrodec_ieee::{RoundingMode, Status};

/// Inverse tangent. Range `(-π/2, +π/2)`.
pub fn atan_kernel<F: DecimalFormat>(x: F, rm: RoundingMode) -> (F, Status) {
    match x.classify() {
        Class::SignalingNaN { .. } => return (x.nan_from(), Status::INVALID),
        Class::QuietNaN { .. } => return (x, Status::OK),
        Class::Infinity { sign } => {
            let half_pi = pi_over_two_ext().to_format::<F>(0, rm).0;
            return (if sign { half_pi.neg() } else { half_pi }, Status::INEXACT);
        }
        Class::Zero { .. } => return (x, Status::OK),
        Class::Finite { .. } => {}
    }
    let x_ext = Extended::from_format(x);
    let result_ext = atan_ext::<F>(x_ext);
    let (result, status) = result_ext.to_format::<F>(0, rm);
    (result, status | Status::INEXACT)
}

/// Inverse sine. Domain `[-1, +1]`; outside is NaN + INVALID.
/// Range `[-π/2, +π/2]`.
pub fn asin_kernel<F: DecimalFormat>(x: F, rm: RoundingMode) -> (F, Status) {
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
            // asin(±1) = ±π/2.
            let half_pi = pi_over_two_ext().to_format::<F>(0, rm).0;
            let signed = if x.is_sign_negative() {
                half_pi.neg()
            } else {
                half_pi
            };
            return (signed, Status::INEXACT);
        }
        _ => {}
    }
    let x_ext = Extended::from_format(x);
    let result_ext = asin_ext::<F>(x_ext);
    let (result, status) = result_ext.to_format::<F>(0, rm);
    (result, status | Status::INEXACT)
}

/// Inverse cosine. Domain `[-1, +1]`; outside is NaN + INVALID.
/// Range `[0, π]`.
pub fn acos_kernel<F: DecimalFormat>(x: F, rm: RoundingMode) -> (F, Status) {
    match x.classify() {
        Class::SignalingNaN { .. } => return (x.nan_from(), Status::INVALID),
        Class::QuietNaN { .. } => return (x, Status::OK),
        Class::Infinity { .. } => return (F::NAN, Status::INVALID),
        Class::Zero { .. } => {
            let half_pi = pi_over_two_ext().to_format::<F>(0, rm).0;
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
                let pi_d = pi_ext().to_format::<F>(0, rm).0;
                return (pi_d, Status::INEXACT);
            }
            return (F::ZERO, Status::OK);
        }
        _ => {}
    }
    let x_ext = Extended::from_format(x);
    // acos(x) = π/2 - asin(x).
    let asin_ext_v = asin_ext::<F>(x_ext);
    let result_ext = pi_over_two_ext().sub(asin_ext_v);
    let (result, status) = result_ext.to_format::<F>(0, rm);
    (result, status | Status::INEXACT)
}

/// Two-argument arctangent `atan2(y, x)`. Range `(-π, +π]`.
/// Quadrant per IEEE 754-2019 §9.2.1.
pub fn atan2_kernel<F: DecimalFormat>(y: F, x: F, rm: RoundingMode) -> (F, Status) {
    // NaN propagation (sNaN raises INVALID).
    if y.is_signaling_nan() || x.is_signaling_nan() {
        return (y.propagate_nan2(x), Status::INVALID);
    }
    if y.is_nan() || x.is_nan() {
        return (y.propagate_nan2(x), Status::OK);
    }
    let pi_d = pi_ext().to_format::<F>(0, rm).0;
    let half_pi = pi_over_two_ext().to_format::<F>(0, rm).0;
    let three_quarter_pi = pi_over_four_ext()
        .mul(Extended::from_i32(3))
        .to_format::<F>(0, rm)
        .0;
    let quarter_pi = pi_over_four_ext().to_format::<F>(0, rm).0;

    let y_neg = y.is_sign_negative();
    let signed = |v: F| if y_neg { v.neg() } else { v };

    // Inf handling.
    if x.is_infinite() && y.is_infinite() {
        // ±π/4 or ±3π/4 depending on signs.
        return if x.is_sign_negative() {
            (signed(three_quarter_pi), Status::INEXACT)
        } else {
            (signed(quarter_pi), Status::INEXACT)
        };
    }
    if y.is_infinite() {
        // ±π/2.
        return (signed(half_pi), Status::INEXACT);
    }
    if x.is_infinite() {
        return if x.is_sign_negative() {
            (signed(pi_d), Status::INEXACT)
        } else {
            (if y_neg { F::NEG_ZERO } else { F::ZERO }, Status::OK)
        };
    }
    // Both finite. Cover x = 0.
    if x.is_zero() {
        if y.is_zero() {
            // atan2(±0, +0) = ±0; atan2(±0, -0) = ±π.
            if x.is_sign_negative() {
                return (signed(pi_d), Status::OK);
            }
            return (if y_neg { F::NEG_ZERO } else { F::ZERO }, Status::OK);
        }
        return (signed(half_pi), Status::INEXACT);
    }
    if y.is_zero() {
        // atan2(±0, x): 0 if x > 0, ±π if x < 0.
        return if x.is_sign_negative() {
            (signed(pi_d), Status::INEXACT)
        } else {
            (if y_neg { F::NEG_ZERO } else { F::ZERO }, Status::OK)
        };
    }
    // Both finite non-zero. Compute y/x at extended precision, run
    // atan, then quadrant-shift.
    let y_ext = Extended::from_format(y);
    let x_ext = Extended::from_format(x);
    let q = y_ext.div::<F>(x_ext);
    let mut result_ext = atan_ext::<F>(q);
    if x.is_sign_negative() {
        // atan2 in quadrants 2 / 3: shift by ±π.
        if y_neg {
            result_ext = result_ext.sub(pi_ext());
        } else {
            result_ext = result_ext.add(pi_ext());
        }
    }
    let (result, status) = result_ext.to_format::<F>(0, rm);
    (result, status | Status::INEXACT)
}

/// `atan(x)` at `Extended` precision. Pre-conditions: `x` is finite
/// and non-zero (zero handled in the caller's special-case path).
fn atan_ext<F: DecimalFormat>(x: Extended) -> Extended {
    let neg = x.sign;
    let mut t = x.abs();
    let mut shift = Extended::ZERO;

    // Stage 1: |x| > 1 → atan(x) = π/2 - atan(1/x) (with sign).
    let mut inverted = false;
    if t.cmp(Extended::ONE) == core::cmp::Ordering::Greater {
        t = t.recip::<F>();
        inverted = true;
    }

    // Stage 2: tan(π/8) < |x| ≤ 1 → atan(x) = π/4 + atan((x-1)/(x+1)).
    let tan_eighth = tan_pi_over_eight_ext();
    if t.cmp(tan_eighth) == core::cmp::Ordering::Greater {
        let num = t.sub(Extended::ONE);
        let den = t.add(Extended::ONE);
        t = num.div::<F>(den); // signed: in [-tan(π/8), 0]
        shift = pi_over_four_ext();
    }

    // Taylor: atan(t) = t - t³/3 + t⁵/5 - t⁷/7 + …
    let mut sum = t;
    let mut t_pow = t; // t^(2k+1); initially t^1
    let t_sq = t.square();
    let mut alt = true; // next term subtracts
    for n in 1u32..=200 {
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
        result = pi_over_two_ext().sub(result);
    }
    // Apply original sign.
    if neg {
        result = result.neg();
    }
    result
}

/// `asin(x)` at extended precision for `|x| < 1`. Uses
/// `2 · atan(x / (1 + sqrt(1 - x²)))` — numerically stable across
/// the full domain (no blow-up at `|x| = 1`).
fn asin_ext<F: DecimalFormat>(x: Extended) -> Extended {
    if x.is_zero() {
        return x;
    }
    let one_minus_x_sq = Extended::ONE.sub(x.square());
    let sqrt_term = one_minus_x_sq.sqrt::<F>();
    let denom = Extended::ONE.add(sqrt_term);
    let inner = x.div::<F>(denom);
    let half_atan = atan_ext::<F>(inner);
    half_atan.add(half_atan)
}
