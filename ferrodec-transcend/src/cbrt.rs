//! Moved from `ferrodec/src/math/cbrt.rs` @ commit 82a7fe1 (P0a.2 c6).
//! Behaviour-neutral: genericized over [`DecimalFormat`]; the
//! `Decimal128` instantiation is byte-identical to the pre-move kernel.
//!
//! `cbrt(x)` — cube root, defined for all real `x`.
//!
//! `cbrt(x) = sign(x) · |x|^(1/3)`, computed as
//! `sign(x) · exp(ln(|x|) / 3)` at the
//! [`Extended`] precision pipeline.
//!
//! ## Accuracy
//!
//! Correctly rounded across the function's domain (ADR-0032;
//! supersedes ADR-0024's faithful contract). Derived from `exp` and
//! `ln` via three composition steps (one `ln`, one division by 3,
//! one `exp`); the bound is the worse of the `exp` and `ln` bounds
//! plus the composition rounding. The worst case half ULP margins
//! per format precision are `2.102016e-08` at `Decimal32` (proven
//! across the full canonical Decimal32 input set by the ADR-0033
//! Plan C4 exhaustive Arb sweep at input `-3.804522e-87`;
//! `tests/vectors/transcend/exhaustive/cbrt.txt`), `3.411e-4` at
//! `Decimal64`, and `2.942e-4` at `Decimal128` (both sampled corpus
//! minima from `tests/vectors/transcend/cbrt.prov`, ADR-0026
//! fd-97a). The 50 digit kernel clears the smallest margin by more
//! than thirty orders of magnitude on every format. The directed
//! mode handling at `eff_rm` reflects the rounding for negative
//! arguments (fd-r5m); the bound holds for every IEEE 754 directed
//! mode. cbrt has no TMD hard candidates in the canonical
//! enumeration: cbrt(1) = 1 and cube roots of perfect cubes
//! (cbrt(8), cbrt(27), ...) all resolve decisively at low Arb
//! precision because the certified ball is centred well inside the
//! format's range, not at the underflow boundary.

use crate::exp::exp_from_extended_body;
use crate::extended::{ExtNum, Extended};
use crate::format::DecimalFormat;
use crate::ln::ln_extended_body;
use ferrodec_ieee::IeeeDecodedClass as Class;
use ferrodec_ieee::{RoundingMode, Status};

/// Cube root. Defined for all real `x`:
/// `cbrt(0) = 0`, `cbrt(-x) = -cbrt(x)`.
///
/// ## Exactness and ties (ADR-0059 classification leg)
///
/// Exactness is decided from the input alone — stripped
/// `|x| = c · 10^e` is a perfect cube iff `c = t³` and `3 | e` — and
/// `cbrt` provably has no nearest-mode ties; both proofs live on
/// `exact::cbrt_exact_input`. Past the classifier the result is
/// irrational and the unconditional `INEXACT` is correct in every
/// mode.
pub fn cbrt_kernel<F: DecimalFormat>(x: F, rm: RoundingMode) -> (F, Status) {
    cbrt_kernel_body::<F, Extended>(x, rm)
}

/// Generic body of [`cbrt_kernel`] (M4, ADR-0059).
pub(crate) fn cbrt_kernel_body<F: DecimalFormat, E: ExtNum>(x: F, rm: RoundingMode) -> (F, Status) {
    match x.classify() {
        Class::SignalingNaN { .. } => return (x.nan_from(), Status::INVALID),
        Class::QuietNaN { .. } => return (x, Status::OK),
        Class::Infinity { .. } => return (x, Status::OK),
        Class::Zero { .. } => return (x, Status::OK),
        Class::Finite { .. } => {}
    }
    // cbrt(x) = sign(x) · exp(ln(|x|) / 3) — the negative-argument
    // case where `pow` would return NaN (non-integer exponent on
    // negative base) is handled here by working on |x| and
    // re-applying the sign.
    let sign_neg = x.is_sign_negative();
    let abs_x = x.abs();

    // Input-side exactness (ADR-0059 M7): a perfect cube delivers its
    // exact root here — every rounding direction, status OK — before
    // any approximation runs. This replaces the ADR-0047 post-hoc
    // proof, which was circular: it could only recognise an exact root
    // the kernel had already delivered exactly, so at TowardZero /
    // TowardNegative the kernel's 50-digit error landed cbrt(0.027)
    // on 0.2999…9, the cube-back check saw a non-cube, and the wrong
    // value shipped with a spurious INEXACT. cbrt has no ties (see
    // `exact::cbrt_exact_input`), so past this point the kernel's
    // unconditional INEXACT is correct in every mode. The root is
    // exact, so re-applying the sign needs no rounding-direction
    // reflection.
    if let Some((root, status)) = crate::exact::cbrt_exact_input::<F>(abs_x, rm) {
        return (if sign_neg { root.neg() } else { root }, status);
    }

    // ln(|x|) at working precision.
    let ln_x_ext = ln_extended_body::<F, E>(abs_x);
    // Divide by 3 at working precision.
    let one_third_ln_x = ln_x_ext.div_u32(3);
    // exp(...) → format datum, threading OVERFLOW / UNDERFLOW. For a
    // negative argument the magnitude is rounded and then negated,
    // so the rounding direction must be reflected first: rounding
    // `|cbrt(x)|` toward `+∞` and negating yields `cbrt(x)` rounded
    // toward `−∞` (and vice versa). Without this, the two directed
    // modes round a negative cube root the wrong way by up to one
    // ULP (fd-r5m, found by the S5 faithful-rounding oracle).
    let eff_rm = if sign_neg { rm.for_negation() } else { rm };
    let (mut result, status) = exp_from_extended_body::<F, E>(one_third_ln_x, eff_rm);
    if sign_neg {
        result = result.neg();
    }
    // `exp_from_extended` raised INEXACT, and here that is correct
    // unconditionally: the perfect cubes returned above, and cbrt has
    // no other exact cases and no ties (`exact::cbrt_exact_input`).
    (result, status)
}
