//! Moved from `ferrodec/src/math/cbrt.rs` @ commit 82a7fe1 (P0a.2 c6).
//! Behaviour-neutral: genericized over [`DecimalFormat`]; the
//! `Decimal128` instantiation is byte-identical to the pre-move kernel.
//!
//! `cbrt(x)` — cube root, defined for all real `x`.
//!
//! `cbrt(x) = sign(x) · |x|^(1/3)`, computed via `pow` at the
//! [`Extended`](crate::extended::Extended)-precision pipeline so the
//! result is faithfully rounded (≤ 1 ULP) for typical inputs.

use crate::exp::exp_from_extended;
use crate::format::DecimalFormat;
use crate::ln::ln_extended;
use ferrodec_ieee::IeeeDecodedClass as Class;
use ferrodec_ieee::{RoundingMode, Status};

/// Cube root. Defined for all real `x`:
/// `cbrt(0) = 0`, `cbrt(-x) = -cbrt(x)`.
pub fn cbrt_kernel<F: DecimalFormat>(x: F, rm: RoundingMode) -> (F, Status) {
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

    // ln(|x|) at extended precision.
    let ln_x_ext = ln_extended(abs_x);
    // Divide by 3 at extended precision.
    let one_third_ln_x = ln_x_ext.div_u32(3);
    // exp(...) → format datum, threading OVERFLOW / UNDERFLOW. For a
    // negative argument the magnitude is rounded and then negated,
    // so the rounding direction must be reflected first: rounding
    // `|cbrt(x)|` toward `+∞` and negating yields `cbrt(x)` rounded
    // toward `−∞` (and vice versa). Without this, the two directed
    // modes round a negative cube root the wrong way by up to one
    // ULP (fd-r5m, found by the S5 faithful-rounding oracle).
    let eff_rm = if sign_neg { rm.for_negation() } else { rm };
    let (mut result, mut status) = exp_from_extended::<F>(one_third_ln_x, eff_rm);
    if sign_neg {
        result = result.neg();
    }
    status |= Status::INEXACT;
    (result, status)
}
