//! IEEE 754-2019 §9.2 hyperbolic functions for [`Decimal64`].
//!
//! `sinh`, `cosh`, `tanh`, `asinh`, `acosh`, `atanh`. Routed through
//! `f64` via `libm`. Same precision posture as the rest of the
//! transcendental cluster: Decimal64 carries 16 digits, f64 ~15.95,
//! so the f64 round-trip caps achievable precision at ~10⁻¹⁵
//! relative. v1.0 ships this baseline; a future commit can replace
//! it with a pure-decimal kernel at u128 working precision.
//!
//! # f64-pipeline range limits
//!
//! `sinh` and `cosh` grow as `±e^|x| / 2`, so they saturate `f64` at
//! the same threshold as [`Decimal64::exp`]: `|x| ≳ 710` overflows
//! `f64` and the implementation returns `±∞ + OVERFLOW + INEXACT`.
//! Decimal64's exponent range in principle supports magnitudes up to
//! `~e^885`, but the f64 pipeline gives up first. A future
//! pure-decimal kernel would close the gap (see
//! [`Decimal64::exp`]'s module doc for the same discussion).
//!
//! `tanh`, `asinh`, `acosh`, `atanh` saturate well inside the f64
//! range and are unaffected.

use crate::bid::{classify_bits, Class};
use crate::decimal::Decimal64;
use ferrodec_ieee::{RoundingMode, Status};

impl Decimal64 {
    /// IEEE 754-2019 §9.2 `sinh(self)` rounded by `rm`.
    #[must_use]
    pub fn sinh(self, rm: RoundingMode) -> (Self, Status) {
        match classify_bits(self.0) {
            Class::SignalingNaN { sign, payload } => (
                Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::INVALID,
            ),
            Class::QuietNaN { sign, payload } => (
                Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::OK,
            ),
            Class::Infinity { sign } => (
                if sign { Decimal64::NEG_INFINITY } else { Decimal64::INFINITY },
                Status::OK,
            ),
            Class::Zero { sign, .. } => (
                if sign { Decimal64::NEG_ZERO } else { Decimal64::ZERO },
                Status::OK,
            ),
            Class::Finite { .. } => f64_unary(self, libm::sinh, rm),
        }
    }

    /// IEEE 754-2019 §9.2 `cosh(self)` rounded by `rm`.
    #[must_use]
    pub fn cosh(self, rm: RoundingMode) -> (Self, Status) {
        match classify_bits(self.0) {
            Class::SignalingNaN { sign, payload } => (
                Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::INVALID,
            ),
            Class::QuietNaN { sign, payload } => (
                Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::OK,
            ),
            Class::Infinity { .. } => (Decimal64::INFINITY, Status::OK),
            Class::Zero { .. } => (Decimal64::ONE, Status::OK),
            Class::Finite { .. } => f64_unary(self, libm::cosh, rm),
        }
    }

    /// IEEE 754-2019 §9.2 `tanh(self)` rounded by `rm`.
    #[must_use]
    pub fn tanh(self, rm: RoundingMode) -> (Self, Status) {
        match classify_bits(self.0) {
            Class::SignalingNaN { sign, payload } => (
                Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::INVALID,
            ),
            Class::QuietNaN { sign, payload } => (
                Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::OK,
            ),
            // tanh(±∞) = ±1.
            Class::Infinity { sign } => (
                if sign { Decimal64::NEG_ONE } else { Decimal64::ONE },
                Status::OK,
            ),
            Class::Zero { sign, .. } => (
                if sign { Decimal64::NEG_ZERO } else { Decimal64::ZERO },
                Status::OK,
            ),
            Class::Finite { .. } => f64_unary(self, libm::tanh, rm),
        }
    }

    /// IEEE 754-2019 §9.2 `asinh(self)` rounded by `rm`.
    #[must_use]
    pub fn asinh(self, rm: RoundingMode) -> (Self, Status) {
        match classify_bits(self.0) {
            Class::SignalingNaN { sign, payload } => (
                Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::INVALID,
            ),
            Class::QuietNaN { sign, payload } => (
                Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::OK,
            ),
            // asinh(±∞) = ±∞.
            Class::Infinity { sign } => (
                if sign { Decimal64::NEG_INFINITY } else { Decimal64::INFINITY },
                Status::OK,
            ),
            Class::Zero { sign, .. } => (
                if sign { Decimal64::NEG_ZERO } else { Decimal64::ZERO },
                Status::OK,
            ),
            Class::Finite { .. } => f64_unary(self, libm::asinh, rm),
        }
    }

    /// IEEE 754-2019 §9.2 `acosh(self)` rounded by `rm`. Domain:
    /// `[1, +∞)`. Inputs below 1 raise INVALID.
    #[must_use]
    pub fn acosh(self, rm: RoundingMode) -> (Self, Status) {
        match classify_bits(self.0) {
            Class::SignalingNaN { sign, payload } => (
                Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::INVALID,
            ),
            Class::QuietNaN { sign, payload } => (
                Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::OK,
            ),
            // acosh(+∞) = +∞; acosh(-∞) = NaN.
            Class::Infinity { sign: false } => (Decimal64::INFINITY, Status::OK),
            Class::Infinity { sign: true } => (Decimal64::NAN, Status::INVALID),
            Class::Zero { .. } | Class::Finite { sign: true, .. } => {
                (Decimal64::NAN, Status::INVALID)
            }
            Class::Finite { sign: false, .. } => {
                let x = self.to_f64();
                if x < 1.0 {
                    return (Decimal64::NAN, Status::INVALID);
                }
                f64_unary_via_value(x, libm::acosh, rm)
            }
        }
    }

    /// IEEE 754-2019 §9.2 `atanh(self)` rounded by `rm`. Domain:
    /// `(-1, +1)`. `atanh(±1) = ±∞ + DIV_BY_ZERO`. Outside the open
    /// interval raises INVALID.
    #[must_use]
    pub fn atanh(self, rm: RoundingMode) -> (Self, Status) {
        match classify_bits(self.0) {
            Class::SignalingNaN { sign, payload } => (
                Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::INVALID,
            ),
            Class::QuietNaN { sign, payload } => (
                Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::OK,
            ),
            Class::Infinity { .. } => (Decimal64::NAN, Status::INVALID),
            Class::Zero { sign, .. } => (
                if sign { Decimal64::NEG_ZERO } else { Decimal64::ZERO },
                Status::OK,
            ),
            Class::Finite { .. } => {
                let x = self.to_f64();
                if x.abs() == 1.0 {
                    return (
                        if x > 0.0 { Decimal64::INFINITY } else { Decimal64::NEG_INFINITY },
                        Status::DIV_BY_ZERO,
                    );
                }
                if x.abs() > 1.0 {
                    return (Decimal64::NAN, Status::INVALID);
                }
                f64_unary_via_value(x, libm::atanh, rm)
            }
        }
    }
}

fn f64_unary(d: Decimal64, op: fn(f64) -> f64, rm: RoundingMode) -> (Decimal64, Status) {
    f64_unary_via_value(d.to_f64(), op, rm)
}

fn f64_unary_via_value(x: f64, op: fn(f64) -> f64, rm: RoundingMode) -> (Decimal64, Status) {
    let r = op(x);
    if r.is_infinite() {
        return (
            if r > 0.0 { Decimal64::INFINITY } else { Decimal64::NEG_INFINITY },
            Status::OVERFLOW | Status::INEXACT,
        );
    }
    let (val, mut status) = Decimal64::from_f64(r, rm);
    if !val.is_zero() {
        status |= Status::INEXACT;
    }
    (val, status)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_equal(a: Decimal64, b: Decimal64) -> bool {
        let af = a.to_f64();
        let bf = b.to_f64();
        let tol = 1e-13;
        (af - bf).abs() <= tol * (1.0 + bf.abs())
    }

    #[test]
    fn sinh_cosh_at_zero() {
        let (r, _) = Decimal64::ZERO.sinh(RoundingMode::NearestEven);
        assert!(r.is_zero() && !r.is_sign_negative());

        let (r, _) = Decimal64::ZERO.cosh(RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), Decimal64::ONE.to_bits());
    }

    #[test]
    fn tanh_at_infinity_is_one() {
        let (r, _) = Decimal64::INFINITY.tanh(RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), Decimal64::ONE.to_bits());

        let (r, _) = Decimal64::NEG_INFINITY.tanh(RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), Decimal64::NEG_ONE.to_bits());
    }

    #[test]
    fn cosh_one() {
        // cosh(1) ≈ 1.543080634815244
        let (r, _) = Decimal64::ONE.cosh(RoundingMode::NearestEven);
        let expected = Decimal64::parse_str("1.543080634815244", RoundingMode::NearestEven)
            .unwrap()
            .0;
        assert!(approx_equal(r, expected));
    }

    #[test]
    fn acosh_one_is_zero() {
        let (r, _) = Decimal64::ONE.acosh(RoundingMode::NearestEven);
        assert!(r.is_zero());
    }

    #[test]
    fn acosh_below_one_invalid() {
        let (r, s) = Decimal64::ZERO.acosh(RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());

        let half = Decimal64::parse_str("0.5", RoundingMode::NearestEven).unwrap().0;
        let (r, s) = half.acosh(RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());
    }

    #[test]
    fn atanh_at_one_is_infinity() {
        let (r, s) = Decimal64::ONE.atanh(RoundingMode::NearestEven);
        assert!(r.is_infinite() && !r.is_sign_negative());
        assert!(s.div_by_zero());

        let (r, s) = Decimal64::NEG_ONE.atanh(RoundingMode::NearestEven);
        assert!(r.is_infinite() && r.is_sign_negative());
        assert!(s.div_by_zero());
    }

    #[test]
    fn atanh_outside_domain_invalid() {
        let two = Decimal64::try_new(2, 0).unwrap();
        let (r, s) = two.atanh(RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());
    }

    #[test]
    fn asinh_basic() {
        // asinh(0) = 0; asinh(±∞) = ±∞.
        let (r, _) = Decimal64::ZERO.asinh(RoundingMode::NearestEven);
        assert!(r.is_zero());

        let (r, _) = Decimal64::INFINITY.asinh(RoundingMode::NearestEven);
        assert!(r.is_infinite());
    }

    #[test]
    fn hyperbolic_nan_propagation() {
        let (r, s) = Decimal64::NAN.sinh(RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.is_ok());

        let (r, s) = Decimal64::SIGNALING_NAN.cosh(RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());
    }
}
