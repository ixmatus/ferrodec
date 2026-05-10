//! IEEE 754-2019 §9.2 trigonometric functions for [`Decimal64`].
//!
//! `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2`. All route
//! through `f64` via `libm` (pure Rust, `no_std`, no FFI). Decimal64
//! carries 16 digits while f64 carries ~15.95, so the bottom Decimal64
//! digit may be lost in the f64 round-trip. v1.0 ships this baseline;
//! a future commit can replace it with a pure-decimal kernel at u128
//! working precision (the public surface is drop-in compatible).
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
use crate::decimal::Decimal64;
use ferrodec_ieee::{RoundingMode, Status};

use super::f64_bridge::{f64_unary, f64_unary_via_value};

impl Decimal64 {
    /// IEEE 754-2019 §9.2 `sin(self)` rounded by `rm`.
    #[must_use]
    pub fn sin(self, rm: RoundingMode) -> (Self, Status) {
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
            Class::Finite { .. } => f64_unary(self, libm::sin, rm),
        }
    }

    /// IEEE 754-2019 §9.2 `cos(self)` rounded by `rm`.
    #[must_use]
    pub fn cos(self, rm: RoundingMode) -> (Self, Status) {
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
            Class::Zero { .. } => (Decimal64::ONE, Status::OK),
            Class::Finite { .. } => f64_unary(self, libm::cos, rm),
        }
    }

    /// IEEE 754-2019 §9.2 `tan(self)` rounded by `rm`.
    #[must_use]
    pub fn tan(self, rm: RoundingMode) -> (Self, Status) {
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
            Class::Finite { .. } => f64_unary(self, libm::tan, rm),
        }
    }

    /// IEEE 754-2019 §9.2 `asin(self)` rounded by `rm`.
    /// Domain: `[-1, +1]`. Outside the domain raises INVALID.
    #[must_use]
    pub fn asin(self, rm: RoundingMode) -> (Self, Status) {
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
                if x.abs() > 1.0 {
                    return (Decimal64::NAN, Status::INVALID);
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
                Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::INVALID,
            ),
            Class::QuietNaN { sign, payload } => (
                Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::OK,
            ),
            Class::Infinity { .. } => (Decimal64::NAN, Status::INVALID),
            Class::Zero { .. } | Class::Finite { .. } => {
                let x = self.to_f64();
                if x.abs() > 1.0 {
                    return (Decimal64::NAN, Status::INVALID);
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
                Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::INVALID,
            ),
            Class::QuietNaN { sign, payload } => (
                Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::OK,
            ),
            // atan(±∞) = ±π/2.
            Class::Infinity { sign } => {
                let x = if sign { f64::NEG_INFINITY } else { f64::INFINITY };
                f64_unary_via_value(x, libm::atan, rm)
            }
            Class::Zero { sign, .. } => (
                if sign { Decimal64::NEG_ZERO } else { Decimal64::ZERO },
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
                        Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                        Status::INVALID,
                    );
                }
                Class::QuietNaN { sign, payload } => {
                    return (
                        Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                        Status::OK,
                    );
                }
                _ => {}
            }
        }
        let y_f = self.to_f64();
        let x_f = x.to_f64();
        let r = libm::atan2(y_f, x_f);
        let (val, mut status) = Decimal64::from_f64(r, rm);
        if !val.is_zero() {
            status |= Status::INEXACT;
        }
        (val, status)
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    fn from_int(n: i64, exp: i32) -> Decimal64 {
        Decimal64::try_new(n, exp).unwrap()
    }

    fn approx_equal(a: Decimal64, b: Decimal64) -> bool {
        // Decimal64 carries 16 digits but the f64 round-trip caps
        // effective precision at ~10⁻¹⁵; widen the tolerance to 1e-13
        // to absorb the worst-case double-rounding noise.
        let af = a.to_f64();
        let bf = b.to_f64();
        let tol = 1e-13;
        (af - bf).abs() <= tol * (1.0 + bf.abs())
    }

    #[test]
    fn sin_cos_at_zero() {
        let (s, _) = Decimal64::ZERO.sin(RoundingMode::NearestEven);
        assert!(s.is_zero() && !s.is_sign_negative());

        let (s, _) = Decimal64::NEG_ZERO.sin(RoundingMode::NearestEven);
        assert!(s.is_zero() && s.is_sign_negative());

        let (c, _) = Decimal64::ZERO.cos(RoundingMode::NearestEven);
        assert_eq!(c.to_bits(), Decimal64::ONE.to_bits());
    }

    #[test]
    fn sin_pi_over_two() {
        // sin(π/2) ≈ 1
        let pi_2 = Decimal64::parse_str("1.570796326794897", RoundingMode::NearestEven)
            .unwrap()
            .0;
        let (r, _) = pi_2.sin(RoundingMode::NearestEven);
        assert!(approx_equal(r, Decimal64::ONE));
    }

    #[test]
    fn cos_pi() {
        // cos(π) ≈ -1
        let pi = Decimal64::parse_str("3.141592653589793", RoundingMode::NearestEven)
            .unwrap()
            .0;
        let (r, _) = pi.cos(RoundingMode::NearestEven);
        assert!(approx_equal(r, Decimal64::NEG_ONE));
    }

    #[test]
    fn sin_cos_infinity_invalid() {
        let (r, s) = Decimal64::INFINITY.sin(RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());

        let (r, s) = Decimal64::INFINITY.cos(RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());
    }

    #[test]
    fn tan_at_zero() {
        let (r, _) = Decimal64::ZERO.tan(RoundingMode::NearestEven);
        assert!(r.is_zero() && !r.is_sign_negative());
    }

    #[test]
    fn asin_pi_over_two() {
        // asin(1) = π/2
        let (r, _) = Decimal64::ONE.asin(RoundingMode::NearestEven);
        let expected = Decimal64::parse_str("1.570796326794897", RoundingMode::NearestEven)
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
        let (r, _) = Decimal64::ONE.acos(RoundingMode::NearestEven);
        assert!(r.is_zero());
    }

    #[test]
    fn atan_at_one_is_pi_over_four() {
        let (r, _) = Decimal64::ONE.atan(RoundingMode::NearestEven);
        let expected = Decimal64::parse_str("0.7853981633974483", RoundingMode::NearestEven)
            .unwrap()
            .0;
        assert!(approx_equal(r, expected));
    }

    #[test]
    fn atan_infinity_is_pi_over_two() {
        let (r, _) = Decimal64::INFINITY.atan(RoundingMode::NearestEven);
        let expected = Decimal64::parse_str("1.570796326794897", RoundingMode::NearestEven)
            .unwrap()
            .0;
        assert!(approx_equal(r, expected));
    }

    #[test]
    fn atan2_basic() {
        // atan2(1, 1) = π/4
        let (r, _) = Decimal64::ONE.atan2(Decimal64::ONE, RoundingMode::NearestEven);
        let expected = Decimal64::parse_str("0.7853981633974483", RoundingMode::NearestEven)
            .unwrap()
            .0;
        assert!(approx_equal(r, expected));
    }

    #[test]
    fn trig_nan_propagation() {
        let (r, s) = Decimal64::NAN.sin(RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.is_ok());

        let (r, s) = Decimal64::SIGNALING_NAN.cos(RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());
    }
}
