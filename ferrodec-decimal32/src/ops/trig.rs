//! IEEE 754-2019 §9.2 trigonometric functions for [`Decimal32`].
//!
//! `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2`. All route
//! through `f64` via `libm` (pure Rust, `no_std`, no FFI). The f64
//! round-trip introduces at most a faint double-rounding error,
//! bounded under 1 ULP at Decimal32's 7-digit precision because
//! `f64` carries ~15.95 digits.
//!
//! # Special cases (IEEE 754-2019 §9.2)
//!
//! * NaN propagates (sNaN raises INVALID).
//! * `sin / cos / tan(±0) = ±0 / +1 / ±0` (sign preserved on sin/tan).
//! * `sin / cos / tan(±∞) = NaN + INVALID` (the result is undefined).
//! * `asin(±0) = ±0`. `asin(|x| > 1) = NaN + INVALID`.
//!   `asin(±1) = ±π/2`.
//! * `acos(1) = 0`. `acos(±|x| > 1) = NaN + INVALID`.
//! * `atan(±0) = ±0`. `atan(±∞) = ±π/2`.
//! * `atan2(y, x)` follows the f64 conventions; NaN inputs produce
//!   NaN.

use crate::bid::{classify_bits, Class};
use crate::decimal::Decimal32;
use ferrodec_ieee::{RoundingMode, Status};

use super::f64_bridge::{f64_unary, f64_unary_via_value};

impl Decimal32 {
    /// IEEE 754-2019 §9.2 `sin(self)` rounded by `rm`.
    #[must_use]
    pub fn sin(self, rm: RoundingMode) -> (Self, Status) {
        match classify_bits(self.0) {
            Class::SignalingNaN { sign, payload } => (
                Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::INVALID,
            ),
            Class::QuietNaN { sign, payload } => (
                Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::OK,
            ),
            Class::Infinity { .. } => (Decimal32::NAN, Status::INVALID),
            Class::Zero { sign, .. } => (
                if sign {
                    Decimal32::NEG_ZERO
                } else {
                    Decimal32::ZERO
                },
                Status::OK,
            ),
            Class::Finite { .. } => f64_unary(self, libm::sin, rm),
        }
    }

    /// IEEE 754-2019 §9.2 `cos(self)` rounded by `rm`.
    #[must_use]
    pub fn cos(self, rm: RoundingMode) -> (Self, Status) {
        match classify_bits(self.0) {
            Class::SignalingNaN { sign, payload } => (
                Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::INVALID,
            ),
            Class::QuietNaN { sign, payload } => (
                Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::OK,
            ),
            Class::Infinity { .. } => (Decimal32::NAN, Status::INVALID),
            Class::Zero { .. } => (Decimal32::ONE, Status::OK),
            Class::Finite { .. } => f64_unary(self, libm::cos, rm),
        }
    }

    /// IEEE 754-2019 §9.2 `tan(self)` rounded by `rm`.
    #[must_use]
    pub fn tan(self, rm: RoundingMode) -> (Self, Status) {
        match classify_bits(self.0) {
            Class::SignalingNaN { sign, payload } => (
                Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::INVALID,
            ),
            Class::QuietNaN { sign, payload } => (
                Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::OK,
            ),
            Class::Infinity { .. } => (Decimal32::NAN, Status::INVALID),
            Class::Zero { sign, .. } => (
                if sign {
                    Decimal32::NEG_ZERO
                } else {
                    Decimal32::ZERO
                },
                Status::OK,
            ),
            Class::Finite { .. } => f64_unary(self, libm::tan, rm),
        }
    }

    /// IEEE 754-2019 §9.2 `asin(self)` rounded by `rm`.
    /// Domain: `[-1, +1]`. Outside the domain raises INVALID.
    #[must_use]
    pub fn asin(self, rm: RoundingMode) -> (Self, Status) {
        match classify_bits(self.0) {
            Class::SignalingNaN { sign, payload } => (
                Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::INVALID,
            ),
            Class::QuietNaN { sign, payload } => (
                Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::OK,
            ),
            Class::Infinity { .. } => (Decimal32::NAN, Status::INVALID),
            Class::Zero { sign, .. } => (
                if sign {
                    Decimal32::NEG_ZERO
                } else {
                    Decimal32::ZERO
                },
                Status::OK,
            ),
            Class::Finite { .. } => {
                let x = self.to_f64(RoundingMode::NearestEven).0;
                if x.abs() > 1.0 {
                    return (Decimal32::NAN, Status::INVALID);
                }
                f64_unary_via_value(x, libm::asin, rm)
            }
        }
    }

    /// IEEE 754-2019 §9.2 `acos(self)` rounded by `rm`.
    /// Domain: `[-1, +1]`. Outside the domain raises INVALID.
    #[must_use]
    pub fn acos(self, rm: RoundingMode) -> (Self, Status) {
        match classify_bits(self.0) {
            Class::SignalingNaN { sign, payload } => (
                Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::INVALID,
            ),
            Class::QuietNaN { sign, payload } => (
                Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::OK,
            ),
            Class::Infinity { .. } => (Decimal32::NAN, Status::INVALID),
            Class::Zero { .. } | Class::Finite { .. } => {
                let x = self.to_f64(RoundingMode::NearestEven).0;
                if x.abs() > 1.0 {
                    return (Decimal32::NAN, Status::INVALID);
                }
                f64_unary_via_value(x, libm::acos, rm)
            }
        }
    }

    /// IEEE 754-2019 §9.2 `atan(self)` rounded by `rm`.
    #[must_use]
    pub fn atan(self, rm: RoundingMode) -> (Self, Status) {
        match classify_bits(self.0) {
            Class::SignalingNaN { sign, payload } => (
                Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::INVALID,
            ),
            Class::QuietNaN { sign, payload } => (
                Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::OK,
            ),
            // atan(±∞) = ±π/2.
            Class::Infinity { sign } => {
                let x = if sign {
                    f64::NEG_INFINITY
                } else {
                    f64::INFINITY
                };
                f64_unary_via_value(x, libm::atan, rm)
            }
            Class::Zero { sign, .. } => (
                if sign {
                    Decimal32::NEG_ZERO
                } else {
                    Decimal32::ZERO
                },
                Status::OK,
            ),
            Class::Finite { .. } => f64_unary(self, libm::atan, rm),
        }
    }

    /// `atan2(self, x)` — the angle whose tangent is `self / x`,
    /// resolved into the correct quadrant by the signs of both
    /// arguments. Returns radians in `(-π, π]`. Special cases follow
    /// the f64 atan2 convention (NaN propagates, axis cases are
    /// exact).
    #[must_use]
    pub fn atan2(self, x: Self, rm: RoundingMode) -> (Self, Status) {
        // NaN propagation.
        for arg in [self, x] {
            match classify_bits(arg.0) {
                Class::SignalingNaN { sign, payload } => {
                    return (
                        Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                        Status::INVALID,
                    );
                }
                Class::QuietNaN { sign, payload } => {
                    return (
                        Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                        Status::OK,
                    );
                }
                _ => {}
            }
        }
        let y_f = self.to_f64(RoundingMode::NearestEven).0;
        let x_f = x.to_f64(RoundingMode::NearestEven).0;
        let r = libm::atan2(y_f, x_f);
        let (val, mut status) = Decimal32::from_f64(r, rm);
        if !val.is_zero() {
            status |= Status::INEXACT;
        }
        (val, status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn from_int(n: i32, exp: i32) -> Decimal32 {
        Decimal32::try_new(n, exp).unwrap()
    }

    fn approx_equal(a: Decimal32, b: Decimal32) -> bool {
        let af = a.to_f64(RoundingMode::NearestEven).0;
        let bf = b.to_f64(RoundingMode::NearestEven).0;
        let tol = 1e-6;
        (af - bf).abs() <= tol * (1.0 + bf.abs())
    }

    #[test]
    fn sin_cos_at_zero() {
        let (s, _) = Decimal32::ZERO.sin(RoundingMode::NearestEven);
        assert!(s.is_zero() && !s.is_sign_negative());

        let (s, _) = Decimal32::NEG_ZERO.sin(RoundingMode::NearestEven);
        assert!(s.is_zero() && s.is_sign_negative());

        let (c, _) = Decimal32::ZERO.cos(RoundingMode::NearestEven);
        assert_eq!(c.to_bits(), Decimal32::ONE.to_bits());
    }

    #[test]
    fn sin_pi_over_two() {
        // sin(π/2) ≈ 1
        let pi_2 = Decimal32::parse_str("1.570796", RoundingMode::NearestEven)
            .unwrap()
            .0;
        let (r, _) = pi_2.sin(RoundingMode::NearestEven);
        assert!(approx_equal(r, Decimal32::ONE));
    }

    #[test]
    fn cos_pi() {
        // cos(π) ≈ -1
        let pi = Decimal32::parse_str("3.141593", RoundingMode::NearestEven)
            .unwrap()
            .0;
        let (r, _) = pi.cos(RoundingMode::NearestEven);
        assert!(approx_equal(r, Decimal32::NEG_ONE));
    }

    #[test]
    fn sin_cos_infinity_invalid() {
        let (r, s) = Decimal32::INFINITY.sin(RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());

        let (r, s) = Decimal32::INFINITY.cos(RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());
    }

    #[test]
    fn tan_at_zero() {
        let (r, _) = Decimal32::ZERO.tan(RoundingMode::NearestEven);
        assert!(r.is_zero() && !r.is_sign_negative());
    }

    #[test]
    fn asin_pi_over_two() {
        // asin(1) = π/2
        let (r, _) = Decimal32::ONE.asin(RoundingMode::NearestEven);
        let expected = Decimal32::parse_str("1.570796", RoundingMode::NearestEven)
            .unwrap()
            .0;
        assert!(approx_equal(r, expected));
    }

    #[test]
    fn asin_out_of_domain_invalid() {
        let (r, s) = from_int(2, 0).asin(RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());

        let (r, s) = from_int(-2, 0).asin(RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());
    }

    #[test]
    fn acos_one_is_zero() {
        let (r, _) = Decimal32::ONE.acos(RoundingMode::NearestEven);
        assert!(r.is_zero());
    }

    #[test]
    fn atan_at_one_is_pi_over_four() {
        let (r, _) = Decimal32::ONE.atan(RoundingMode::NearestEven);
        let expected = Decimal32::parse_str("0.7853982", RoundingMode::NearestEven)
            .unwrap()
            .0;
        assert!(approx_equal(r, expected));
    }

    #[test]
    fn atan_infinity_is_pi_over_two() {
        let (r, _) = Decimal32::INFINITY.atan(RoundingMode::NearestEven);
        let expected = Decimal32::parse_str("1.570796", RoundingMode::NearestEven)
            .unwrap()
            .0;
        assert!(approx_equal(r, expected));
    }

    #[test]
    fn atan2_basic() {
        // atan2(1, 1) = π/4
        let (r, _) = Decimal32::ONE.atan2(Decimal32::ONE, RoundingMode::NearestEven);
        let expected = Decimal32::parse_str("0.7853982", RoundingMode::NearestEven)
            .unwrap()
            .0;
        assert!(approx_equal(r, expected));
    }

    #[test]
    fn trig_nan_propagation() {
        let (r, s) = Decimal32::NAN.sin(RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.is_ok());

        let (r, s) = Decimal32::SIGNALING_NAN.cos(RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());
    }
}
