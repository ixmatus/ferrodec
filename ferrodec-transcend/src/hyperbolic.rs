//! Moved from `ferrodec/src/math/hyperbolic.rs` @ commit 82a7fe1
//! (P0a.2 c10). Behaviour-neutral: genericized over [`DecimalFormat`];
//! the `Decimal128` instantiation is byte-identical to the pre-move
//! kernel.
//!
//! Hyperbolic functions and their inverses.
//!
//! ## Forward
//!
//! * `sinh(x) = (eˣ − e⁻ˣ) / 2`
//! * `cosh(x) = (eˣ + e⁻ˣ) / 2`
//! * `tanh(x) = sinh(x) / cosh(x)`
//!
//! For large `|x|` (≳ 14000) `eˣ` overflows; both `sinh` and `cosh`
//! saturate to `±∞`, and `tanh` saturates to `±1`.
//!
//! For small `|x|` (`|x| < 0.5`), the naive `(eˣ − e⁻ˣ)/2` formula
//! suffers cancellation (eˣ and e⁻ˣ are both ≈ 1). We use Taylor
//! directly there: `sinh(x) = x + x³/3! + x⁵/5! + …`. `cosh` is even
//! so the same concern doesn't apply (no cancellation between
//! adjacent terms).
//!
//! ## Inverse
//!
//! * `asinh(x) = ln(x + √(x² + 1))` for `|x| ≥ 0.3`; for the small
//!   band, `log1p(|x| + x²/(1 + √(1 + x²)))` with the sign
//!   re-applied, so the result stays *relative*-accurate where the
//!   ln form would absorb the argument into the `1` anchor
//!   (ADR-0050).
//! * `acosh(x) = ln(x + √(x² − 1))` for `x ≥ 1.01`; the log1p form
//!   with the factored `(x−1)(x+1)` radicand below that; NaN under
//!   the domain.
//! * `atanh(x) = ½·ln((1 + x) / (1 − x))` for `0.15 ≤ |x| < 1`; the
//!   equivalent `½·log1p(2x/(1 − x))` in the small band (ADR-0050);
//!   ±∞ at `±1`; NaN otherwise.
//!
//! All routines run at [`Extended`] precision and round once at the
//! format boundary.
//!
//! ## Accuracy
//!
//! Correctly rounded across each function's domain (ADR-0032;
//! supersedes ADR-0024's faithful contract). The forward family
//! (`sinh`, `cosh`, `tanh`) derives through `exp` (two evaluations
//! at `±x` plus the combining arithmetic); the inverse family
//! (`asinh`, `acosh`, `atanh`) derives through `ln` (plus the
//! sqrt or fraction inside). The worst case half ULP margins per
//! format precision are:
//!
//! - `sinh`: `1.243979e-08` at `Decimal32` (ADR-0033 Plan C4
//!   exhaustive sweep at input `-0.4426808`;
//!   `tests/vectors/transcend/exhaustive/sinh.txt`), `3.166e-3` at
//!   `Decimal64`, `1.648e-2` at `Decimal128`.
//! - `cosh`: `3.515891e-08` at `Decimal32` (Plan C4 exhaustive at
//!   `9.848818e-3`), `4.044e-2` at `Decimal64`, `4.372e-3` at
//!   `Decimal128`. Note: `cosh`'s exhaustive sweep tightens the
//!   sampled corpus minimum (`4.167e-8`) only by roughly 20
//!   percent; the sampled corpus's random TMD search happened to
//!   capture a candidate close to the true worst case by luck. The
//!   other functions in this family tighten by 4 to 7 orders of
//!   magnitude under Plan C4. `cosh` is the campaign's single
//!   sampled-corpus-already-near-optimal outlier.
//! - `tanh`: `6.460895e-09` at `Decimal32` (Plan C4 exhaustive at
//!   `8.752195`), `7.198e-3` at `Decimal64`, `2.550e-3` at
//!   `Decimal128`.
//! - `asinh`: `1.528369e-10` at `Decimal32` (Plan C4 exhaustive at
//!   `2.102146e44`), `8.484e-4` at `Decimal64`, `1.752e-3` at
//!   `Decimal128`. The `Decimal32` worst case input is
//!   byte-identical to `acosh`'s (next item) because both reduce
//!   to ~`ln(2x)` for large positive `x` and the hardest case is
//!   shared by structure; the proven kernel output at that input
//!   is the same value `1.027499e2`.
//! - `acosh`: `1.528369e-10` at `Decimal32` (Plan C4 exhaustive at
//!   `2.102146e44`, byte-identical to `asinh`), `2.755e-5` at
//!   `Decimal64`, `1.844e-3` at `Decimal128`.
//! - `atanh`: `3.956666e-08` at `Decimal32` (Plan C4 exhaustive at
//!   `0.6085038`), `1.113e-3` at `Decimal64`, `6.005e-3` at
//!   `Decimal128`.
//!
//! The `Decimal32` figures are proven correctly rounded across the
//! full canonical input set by Arb; the `Decimal64` and `Decimal128`
//! figures are sampled corpus minima from
//! `tests/vectors/transcend/{sinh,cosh,tanh,asinh,acosh,atanh}.prov`
//! (ADR-0026 fd-97a) under the ADR-0033 Slice A corpus integrity
//! discipline. For `asinh` and `atanh` the margin-to-every-input
//! inference additionally relies on the relative error model the
//! small-band log1p forms restore (ADR-0050; the 2026-06-09 review
//! found the previous ln forms absorbing small arguments at the `1`
//! anchor, and the band corpus
//! `tests/vectors/transcend/anchor_bands/` is the standing witness).
//!
//! The tightest empirical margin across the campaign is the
//! `asinh`/`acosh` shared `1.528369e-10`. At 50 digit kernel working
//! precision the cumulative error is bounded by `K · 10^(p − 50)`
//! with `K` the operation count (under ~150 for any of these
//! functions); at `Decimal32` (`p = 7`) this is `≤ 1.5e-41`, which
//! clears the tightest margin by more than thirty orders of
//! magnitude. The `|x| < 0.5` direct Taylor branch for `sinh` is
//! precisely the cancellation avoidance the bound depends on.
//!
//! `acosh` has one TMD hard candidate at input `1`: `acosh(1) = 0`
//! exactly. The certified Arb ball around the true value 0 has
//! nonzero radius at every Arb precision and straddles the format's
//! underflow boundary, so `_decisive` cannot resolve. The kernel
//! short circuits `acosh(1)` to 0 exactly; this is an oracle side
//! limitation, not a kernel defect. The other five functions in
//! this family have no TMD hard candidates: `sinh(0)`, `cosh(0)`,
//! `tanh(0)`, `asinh(0)` are all zero or one but coef = 0 is not
//! in the canonical enumeration; `atanh(0) = 0` similarly; and the
//! function values at nonzero canonical inputs are transcendental.
//!
//! The shared error model lives in ADR-0032 §Decision; the sampled
//! corpus test, the ADR-0033 exhaustive worst case kernel
//! verification gate
//! (`ferrodec-decimal32/tests/transcend_vectors_exhaustive.rs`,
//! 18/18 exact), and the MPFR cross-validation gate
//! (`ferrodec-test-support/tests/mpfr_gate.rs`, 0 disagreements)
//! are the empirical witnesses.

use crate::exp::exp_extended;
use crate::extended::Extended;
use crate::format::DecimalFormat;
use crate::ln::{ln_from_extended, log1p_extended};
use ferrodec_ieee::IeeeDecodedClass as Class;
use ferrodec_ieee::{RoundingMode, Status};
use ferrodec_multiword::U256;

/// Hyperbolic sine.
pub fn sinh_kernel<F: DecimalFormat>(x: F, rm: RoundingMode) -> (F, Status) {
    match x.classify() {
        Class::SignalingNaN { .. } => return (x.nan_from(), Status::INVALID),
        Class::QuietNaN { .. } => return (x, Status::OK),
        Class::Infinity { .. } => return (x, Status::OK),
        Class::Zero { .. } => return (x, Status::OK),
        Class::Finite { .. } => {}
    }
    let x_ext = Extended::from_format(x);
    let result_ext = sinh_ext::<F>(x_ext);
    let (result, status) = result_ext.to_format::<F>(0, rm);
    (result, status | Status::INEXACT)
}

/// Hyperbolic cosine.
pub fn cosh_kernel<F: DecimalFormat>(x: F, rm: RoundingMode) -> (F, Status) {
    match x.classify() {
        Class::SignalingNaN { .. } => return (x.nan_from(), Status::INVALID),
        Class::QuietNaN { .. } => return (x, Status::OK),
        Class::Infinity { .. } => return (F::INFINITY, Status::OK),
        Class::Zero { .. } => return (F::ONE, Status::OK),
        Class::Finite { .. } => {}
    }
    let x_ext = Extended::from_format(x).abs();
    let result_ext = cosh_ext::<F>(x_ext);
    let (result, status) = result_ext.to_format::<F>(0, rm);
    (result, status | Status::INEXACT)
}

/// Hyperbolic tangent.
pub fn tanh_kernel<F: DecimalFormat>(x: F, rm: RoundingMode) -> (F, Status) {
    match x.classify() {
        Class::SignalingNaN { .. } => return (x.nan_from(), Status::INVALID),
        Class::QuietNaN { .. } => return (x, Status::OK),
        Class::Infinity { sign } => {
            return (if sign { F::NEG_ONE } else { F::ONE }, Status::OK);
        }
        Class::Zero { .. } => return (x, Status::OK),
        Class::Finite { .. } => {}
    }
    // Saturation band, |x| > 45: here `1 − |tanh x| = 2e^{−2|x|} /
    // (1 + e^{−2|x|}) < 2e^{−90} ≈ 1.7 × 10^{−39}`, so the true
    // magnitude lies strictly inside `(1 − 2×10^{−39}, 1)`. Every
    // value in that interval — and in the `(1 − 10^{−50}, 1)`
    // interval the proxy below denotes — rounds identically at every
    // format precision (≤ 34 digits) and every direction: to 1 at
    // the nearest modes and toward the result's own sign of
    // infinity, to the all-nines neighbour toward zero. Feeding the
    // 50-nines coefficient with a sticky residue through the format
    // rounder therefore delivers the §4.3.3 answer per mode, where
    // the previous mode-blind `±1` return mis-rounded `TowardZero`
    // and the directed mode on the result's own side (fd-aqs.5).
    //
    // Below the threshold the quotient path is decisive on its own:
    // at |x| ≤ 45 the extended quotient sits below 1 by at least
    // `~1.6 × 10^{−39}`, four orders of magnitude above the 10^{−50}
    // working resolution and the Newton division error, so its
    // boundary round cannot collapse to exactly 1. (The previous 80
    // threshold left a `~58 < |x| ≤ 80` band where the quotient
    // rounded to 1 at 50 digits and reproduced the saturation defect.)
    let abs_ext = Extended::from_format::<F>(x).abs();
    if abs_ext.cmp(Extended::parse_str("45")) == core::cmp::Ordering::Greater {
        let nines = Extended::parse_str("0.99999999999999999999999999999999999999999999999999");
        let (result, status) = F::round_and_pack_finite(
            nines.coef,
            nines.exp,
            0,
            x.is_sign_negative(),
            true,
            rm,
            Status::OK,
        );
        return (result, status | Status::INEXACT);
    }
    let x_ext = Extended::from_format(x);
    let s = sinh_ext::<F>(x_ext);
    let c = cosh_ext::<F>(x_ext.abs());
    // tanh inherits the sign of x via sinh; cosh is symmetric.
    let result_ext = s.div::<F>(c);
    let (result, status) = result_ext.to_format::<F>(0, rm);
    (result, status | Status::INEXACT)
}

/// Inverse hyperbolic sine, defined for all real `x`.
pub fn asinh_kernel<F: DecimalFormat>(x: F, rm: RoundingMode) -> (F, Status) {
    match x.classify() {
        Class::SignalingNaN { .. } => return (x.nan_from(), Status::INVALID),
        Class::QuietNaN { .. } => return (x, Status::OK),
        Class::Infinity { .. } => return (x, Status::OK),
        Class::Zero { .. } => return (x, Status::OK),
        Class::Finite { .. } => {}
    }
    // asinh(x) = sign(x) · ln(|x| + sqrt(x² + 1))
    // Working on |x| keeps the inner sum strictly positive.
    let neg = x.is_sign_negative();
    let abs_x_ext = Extended::from_format(x).abs();
    // Small-|x| band (fd-aqs.6): `|x| + sqrt(x² + 1)` hands `1 + |x|`
    // to the 50-significant-digit representation, absorbing the
    // argument once it sinks below the working resolution (up to
    // ~3e8 ULP of error in the band, exact `+0` for tiny arguments;
    // the 2026-06-09 review). The equivalent
    // `asinh(x) = log1p(|x| + x² / (1 + sqrt(1 + x²)))` builds the
    // log1p argument from `|x|` directly, so its accuracy — and the
    // series result's — stays *relative* however small `x` is. The
    // 0.3 threshold keeps `u ≤ ~0.344` inside the series budget;
    // above it the original path is well-conditioned
    // (`asinh 0.3 ≈ 0.296` against ~1e-49 absolute error).
    const LOG1P_THRESHOLD: Extended = Extended {
        coef: U256::from_u128(3),
        exp: -1,
        sign: false,
    };
    let result_ext = if abs_x_ext.cmp(LOG1P_THRESHOLD) == core::cmp::Ordering::Less {
        let x_sq = abs_x_ext.square();
        let denom = Extended::ONE.add(x_sq.add(Extended::ONE).sqrt::<F>());
        let u = abs_x_ext.add(x_sq.div::<F>(denom));
        log1p_extended(u)
    } else {
        let x_sq_plus_one = abs_x_ext.square().add(Extended::ONE);
        let inner = abs_x_ext.add(x_sq_plus_one.sqrt::<F>());
        // Pass `inner` to `ln_from_extended` directly — keeping the
        // argument at 50-digit working precision avoids a 34-digit
        // round trip that would propagate ≤ 1 ULP through `ln` to
        // the result.
        ln_from_extended(inner)
    };
    let signed_ext = if neg { result_ext.neg() } else { result_ext };
    let (result, status) = signed_ext.to_format::<F>(0, rm);
    (result, status | Status::INEXACT)
}

/// Inverse hyperbolic cosine, defined for `x ≥ 1`.
pub fn acosh_kernel<F: DecimalFormat>(x: F, rm: RoundingMode) -> (F, Status) {
    match x.classify() {
        Class::SignalingNaN { .. } => return (x.nan_from(), Status::INVALID),
        Class::QuietNaN { .. } => return (x, Status::OK),
        Class::Infinity { sign } => {
            return if sign {
                (F::NAN, Status::INVALID)
            } else {
                (F::INFINITY, Status::OK)
            };
        }
        Class::Zero { .. } => return (F::NAN, Status::INVALID),
        Class::Finite { .. } => {}
    }
    let (cmp, _) = x.partial_cmp_fmt(F::ONE);
    match cmp {
        Some(core::cmp::Ordering::Less) => return (F::NAN, Status::INVALID),
        Some(core::cmp::Ordering::Equal) => return (F::ZERO, Status::OK),
        _ => {}
    }
    // Two paths, picked by how close x is to 1:
    //
    // * For x near 1, computing `x² − 1` directly cancels and costs
    //   ~`digit_count(x − 1)` digits of precision. Extended carries
    //   ~16 digits of headroom over Decimal128, so the original
    //   formula is fine for `x − 1 ≥ 10⁻¹⁶` but loses the envelope
    //   below that. The log1p path keeps `(x − 1)` explicit and
    //   factors `x² − 1 = (x − 1)(x + 1)`, avoiding the cancellation
    //   entirely:
    //
    //       acosh(x) = ln(1 + (x − 1) + sqrt((x − 1)(x + 1)))
    //                = log1p((x − 1) + sqrt((x − 1)(x + 1)))
    //
    // * For x further from 1 the original `ln(x + sqrt(x² − 1))`
    //   path runs entirely at Extended precision (commit f43ce0e)
    //   and stays within ≤ 1 ULP at 34 digits.
    //
    // The threshold `0.01` keeps `inner` comfortably inside log1p's
    // Taylor convergence window (`inner ≤ ~0.15` at this y).
    // Cross-checked against the cancellation budget: at the
    // boundary `x − 1 = 0.01` the direct `x² − 1` formulation loses
    // only `digit_count(x − 1) ≈ 2` digits, comfortably inside
    // Extended's ~16-digit headroom over Decimal128. Lowering the
    // threshold further would shift the work back to the direct
    // path without breaking anything; raising it would force
    // log1p past its smooth convergence window.
    let x_ext = Extended::from_format(x);
    let y = x_ext.sub(Extended::ONE);
    const LOG1P_THRESHOLD: Extended = Extended {
        coef: U256::from_u128(1),
        exp: -2,
        sign: false,
    };
    let result_ext = if y.cmp(LOG1P_THRESHOLD) == core::cmp::Ordering::Less {
        let x_plus_one = x_ext.add(Extended::ONE);
        let inner = y.add(y.mul(x_plus_one).sqrt::<F>());
        log1p_extended(inner)
    } else {
        let x_sq_minus_one = x_ext.square().sub(Extended::ONE);
        let inner = x_ext.add(x_sq_minus_one.sqrt::<F>());
        ln_from_extended(inner)
    };
    let (result, status) = result_ext.to_format::<F>(0, rm);
    (result, status | Status::INEXACT)
}

/// Inverse hyperbolic tangent, defined for `|x| < 1`.
pub fn atanh_kernel<F: DecimalFormat>(x: F, rm: RoundingMode) -> (F, Status) {
    match x.classify() {
        Class::SignalingNaN { .. } => return (x.nan_from(), Status::INVALID),
        Class::QuietNaN { .. } => return (x, Status::OK),
        Class::Infinity { .. } => return (F::NAN, Status::INVALID),
        Class::Zero { .. } => return (x, Status::OK),
        Class::Finite { .. } => {}
    }
    let abs_x = x.abs();
    let (cmp, _) = abs_x.partial_cmp_fmt(F::ONE);
    match cmp {
        Some(core::cmp::Ordering::Greater) => return (F::NAN, Status::INVALID),
        Some(core::cmp::Ordering::Equal) => {
            // atanh(±1) = ±∞, raise DIV_BY_ZERO (the formula has
            // 1/(1−|x|) at the singularity).
            return (
                if x.is_sign_negative() {
                    F::NEG_INFINITY
                } else {
                    F::INFINITY
                },
                Status::DIV_BY_ZERO,
            );
        }
        _ => {}
    }
    let x_ext = Extended::from_format(x);
    // Small-|x| band (fd-aqs.6): the ratio form hands `1 ± x` to the
    // 50-significant-digit representation, absorbing `x` (and the
    // `x²`-order correction) once `|x|` sinks below the working
    // resolution — the 2026-06-09 review measured up to ~3e8 ULP of
    // error in the band and exact `+0` for tiny arguments. The
    // equivalent `atanh(x) = ½·log1p(2x / (1 − x))` keeps the
    // argument's accuracy *relative*: `2x` is exact, `1 − x` is
    // exact for format-sourced coefficients (and its absorption for
    // tiny `x` perturbs `u` only by `x` *relatively*), so the series
    // result is relative-accurate however small `x` is. The 0.15
    // threshold keeps `|u| ≤ 0.3/0.85 ≈ 0.353`, comfortably inside
    // the log1p series' convergence budget; above it the ratio path
    // is well-conditioned (`|atanh x| ≥ 0.15` against ~1e-49
    // absolute error).
    const LOG1P_THRESHOLD: Extended = Extended {
        coef: U256::from_u128(15),
        exp: -2,
        sign: false,
    };
    let result_ext = if x_ext.abs().cmp(LOG1P_THRESHOLD) == core::cmp::Ordering::Less {
        let two_x = x_ext.add(x_ext);
        let one_minus = Extended::ONE.sub(x_ext);
        let u = two_x.div::<F>(one_minus);
        log1p_extended(u).div_u32(2)
    } else {
        // atanh(x) = ½·ln((1 + x) / (1 − x)) — ratio stays at
        // extended precision through the ln call.
        let one_plus = Extended::ONE.add(x_ext);
        let one_minus = Extended::ONE.sub(x_ext);
        let ratio = one_plus.div::<F>(one_minus);
        ln_from_extended(ratio).div_u32(2)
    };
    let (result, status) = result_ext.to_format::<F>(0, rm);
    (result, status | Status::INEXACT)
}

/// `sinh(x)` at [`Extended`] precision.
fn sinh_ext<F: DecimalFormat>(x: Extended) -> Extended {
    if x.is_zero() {
        return x;
    }
    // For |x| < 0.5 use Taylor directly to avoid cancellation in
    // (eˣ − e⁻ˣ)/2. The threshold 0.5 keeps Taylor convergence at
    // ≤ ~40 iterations for 50-digit precision.
    if x.abs().cmp(Extended::HALF) == core::cmp::Ordering::Less {
        return sinh_taylor(x);
    }
    // Saturation: |x| past the format's exp convergence ceiling lands
    // outside the format's range. Return a pre-overflow magnitude with
    // the sign of x; the boundary round produces ±∞ + OVERFLOW.
    if x.abs().cmp(F::exp_overflow_limit()) == core::cmp::Ordering::Greater {
        return Extended::saturate_overflow(x.sign);
    }
    // sinh(x) = (e^x − e^{-x}) / 2, evaluated entirely at extended
    // precision so the cancellation is bounded by Extended's 50-digit
    // working envelope rather than Decimal128's 34-digit one. Combined
    // with the |x| < 0.5 Taylor branch above, this gives ≤ 1 ULP at the
    // 34-digit boundary across the whole representable domain.
    let e_pos = exp_extended(x);
    let e_neg = exp_extended(x.neg());
    e_pos.sub(e_neg).div_u32(2)
}

/// `sinh(x)` Taylor series for `|x| < 0.5`.
/// `sinh(x) = x + x³/3! + x⁵/5! + …` (all positive — no
/// cancellation).
fn sinh_taylor(x: Extended) -> Extended {
    let mut sum = x;
    let mut term = x;
    let x_sq = x.square();
    let mut n: u32 = 1;
    for _ in 0..120 {
        n += 1;
        let denom = (2 * n - 2) * (2 * n - 1);
        term = term.mul(x_sq).div_u32(denom);
        let next_sum = sum.add(term);
        if next_sum.cmp(sum) == core::cmp::Ordering::Equal {
            sum = next_sum;
            break;
        }
        sum = next_sum;
        if term.is_zero() {
            break;
        }
    }
    sum
}

/// `cosh(x)` at [`Extended`] precision. Caller passes the absolute
/// value (cosh is even).
fn cosh_ext<F: DecimalFormat>(abs_x: Extended) -> Extended {
    if abs_x.is_zero() {
        return Extended::ONE;
    }
    // For small |x| (<0.5), Taylor is more accurate (no cancellation).
    if abs_x.cmp(Extended::HALF) == core::cmp::Ordering::Less {
        return cosh_taylor(abs_x);
    }
    // Saturation: |x| past the format's exp convergence ceiling lands
    // outside the format's range. cosh is always positive.
    if abs_x.cmp(F::exp_overflow_limit()) == core::cmp::Ordering::Greater {
        return Extended::saturate_overflow(false);
    }
    // cosh(x) = (e^x + e^{-x}) / 2, end-to-end at extended precision.
    let e_pos = exp_extended(abs_x);
    let e_neg = exp_extended(abs_x.neg());
    e_pos.add(e_neg).div_u32(2)
}

/// `cosh(x) = 1 + x²/2! + x⁴/4! + …` for small `|x|`.
fn cosh_taylor(x: Extended) -> Extended {
    let mut sum = Extended::ONE;
    let mut term = Extended::ONE;
    let x_sq = x.square();
    let mut n: u32 = 0;
    for _ in 0..120 {
        n += 1;
        let denom = (2 * n - 1) * (2 * n);
        term = term.mul(x_sq).div_u32(denom);
        let next_sum = sum.add(term);
        if next_sum.cmp(sum) == core::cmp::Ordering::Equal {
            sum = next_sum;
            break;
        }
        sum = next_sum;
        if term.is_zero() {
            break;
        }
    }
    sum
}
