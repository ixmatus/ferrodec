//! `f64`-routed kernel adapter for the transcendental ops.
//!
//! Every `f64`-routed Decimal64 kernel (sin / cos / tan / asin /
//! acos / atan / sinh / cosh / tanh / asinh / acosh / atanh) follows
//! the same shape: classify the input, dispatch IEEE 754-2019 §9.2
//! special cases, route the finite arm through `libm`, then convert
//! the f64 back to `Decimal64` and tag `INEXACT`. The conversion +
//! tagging step is what this module factors out.
//!
//! Both `f64_unary` (Decimal64 in) and `f64_unary_via_value` (f64
//! in — for callers that already extracted `to_f64()` to check
//! domain before dispatch) cover the same logical operation.
//! `from_f64` returns `(Decimal64, Status)`; we patch the status to
//! reflect the §9.2 conventions:
//!
//! * NaN out → `NaN + INVALID`. Trig functions never produce NaN
//!   for in-domain inputs, but defensive in case `libm` does.
//! * ±∞ out → `±∞ + OVERFLOW | INEXACT`. Hyperbolic kernels
//!   (`sinh`, `cosh`) can saturate `f64` at |x| ≳ 710.
//! * Otherwise → the `from_f64` result with `INEXACT` set when
//!   the converted Decimal64 is non-zero (transcendentals are
//!   exact at zero and irrational almost everywhere else).

use crate::decimal::Decimal64;
use ferrodec_ieee::{RoundingMode, Status};

/// Route `op` over the `f64` value of `d`, converting back to
/// `Decimal64` with the §9.2 status-tagging convention.
pub(crate) fn f64_unary(d: Decimal64, op: fn(f64) -> f64, rm: RoundingMode) -> (Decimal64, Status) {
    f64_unary_via_value(d.to_f64(), op, rm)
}

/// Route `op` over an already-extracted `f64`. Useful when the
/// caller checked the domain on the f64 directly (e.g. `asin`
/// rejecting `|x| > 1`).
pub(crate) fn f64_unary_via_value(
    x: f64,
    op: fn(f64) -> f64,
    rm: RoundingMode,
) -> (Decimal64, Status) {
    let r = op(x);
    if r.is_nan() {
        return (Decimal64::NAN, Status::INVALID);
    }
    if r.is_infinite() {
        return (
            if r > 0.0 {
                Decimal64::INFINITY
            } else {
                Decimal64::NEG_INFINITY
            },
            Status::OVERFLOW | Status::INEXACT,
        );
    }
    let (val, mut status) = Decimal64::from_f64(r, rm);
    if !val.is_zero() {
        status |= Status::INEXACT;
    }
    (val, status)
}
