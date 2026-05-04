//! `cbrt(x)` — cube root, defined for all real `x`.
//!
//! `cbrt(x) = sign(x) · |x|^(1/3)`, computed via `pow` at the
//! `Extended`-precision pipeline so the result is faithfully rounded
//! (≤ 1 ULP) for typical inputs.

use crate::bid::{classify_bits, Class};
use crate::decimal::Decimal128;
use crate::math::exp::exp_from_extended;
use crate::math::ln::ln_extended;
use crate::status::{RoundingMode, Status};

impl Decimal128 {
    /// Cube root. Defined for all real `self`:
    /// `cbrt(0) = 0`, `cbrt(-x) = -cbrt(x)`.
    #[must_use]
    pub fn cbrt(self, rm: RoundingMode) -> (Self, Status) {
        match classify_bits(self.to_bits()) {
            Class::SignalingNaN { .. } => return (Decimal128::NAN, Status::INVALID),
            Class::QuietNaN { .. } => return (self, Status::OK),
            Class::Infinity { .. } => return (self, Status::OK),
            Class::Zero { .. } => return (self, Status::OK),
            Class::Finite { .. } => {}
        }
        // cbrt(x) = sign(x) · exp(ln(|x|) / 3) — the negative-argument
        // case where `pow` would return NaN (non-integer exponent on
        // negative base) is handled here by working on |x| and
        // re-applying the sign.
        let sign_neg = self.is_sign_negative();
        let abs_x = self.abs();

        // ln(|x|) at extended precision.
        let ln_x_ext = ln_extended(abs_x);
        // Divide by 3 at extended precision.
        let one_third_ln_x = ln_x_ext.div_u32(3);
        // exp(...) → Decimal128, threading OVERFLOW / UNDERFLOW.
        let (mut result, mut status) = exp_from_extended(one_third_ln_x, rm);
        if sign_neg {
            result = result.neg();
        }
        status |= Status::INEXACT;
        (result, status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Decimal128 {
        Decimal128::parse_str(s, RoundingMode::NearestEven).unwrap().0
    }

    fn within_ulps(got: Decimal128, want: Decimal128, ulps: u32) -> bool {
        let (diff, _) = got.sub(want, RoundingMode::NearestEven);
        let diff = diff.abs();
        let abs_want = want.abs();
        if abs_want.is_zero() {
            let bound = parse(&alloc::format!("{ulps}e-30"));
            let (cmp, _) = diff.partial_cmp(bound);
            return matches!(
                cmp,
                Some(core::cmp::Ordering::Less | core::cmp::Ordering::Equal)
            );
        }
        let (rel, _) = diff.div(abs_want, RoundingMode::NearestEven);
        let bound = parse(&alloc::format!("{ulps}e-33"));
        let (cmp, _) = rel.partial_cmp(bound);
        matches!(
            cmp,
            Some(core::cmp::Ordering::Less | core::cmp::Ordering::Equal)
        )
    }

    extern crate alloc;

    #[test]
    fn cbrt_zero() {
        let (r, _) = Decimal128::ZERO.cbrt(RoundingMode::NearestEven);
        assert!(r.is_zero());
        assert!(!r.is_sign_negative());
        let (r, _) = Decimal128::NEG_ZERO.cbrt(RoundingMode::NearestEven);
        assert!(r.is_zero());
        assert!(r.is_sign_negative());
    }

    #[test]
    fn cbrt_one() {
        let (r, _) = Decimal128::ONE.cbrt(RoundingMode::NearestEven);
        assert!(within_ulps(r, Decimal128::ONE, 1));
    }

    #[test]
    fn cbrt_eight() {
        let (r, _) = parse("8").cbrt(RoundingMode::NearestEven);
        assert!(within_ulps(r, parse("2"), 1));
    }

    #[test]
    fn cbrt_negative() {
        let (r, _) = parse("-27").cbrt(RoundingMode::NearestEven);
        assert!(within_ulps(r, parse("-3"), 1));
    }

    #[test]
    fn cbrt_fractional() {
        let (r, _) = parse("0.001").cbrt(RoundingMode::NearestEven);
        assert!(within_ulps(r, parse("0.1"), 1));
    }

    #[test]
    fn cbrt_inf() {
        let (r, _) = Decimal128::INFINITY.cbrt(RoundingMode::NearestEven);
        assert!(r.is_infinite() && !r.is_sign_negative());
        let (r, _) = Decimal128::NEG_INFINITY.cbrt(RoundingMode::NearestEven);
        assert!(r.is_infinite() && r.is_sign_negative());
    }

    #[test]
    fn cbrt_nan_propagates() {
        let (r, _) = Decimal128::NAN.cbrt(RoundingMode::NearestEven);
        assert!(r.is_nan());
        let (r, st) = Decimal128::SIGNALING_NAN.cbrt(RoundingMode::NearestEven);
        assert!(r.is_nan());
        assert!(st.invalid());
    }
}
