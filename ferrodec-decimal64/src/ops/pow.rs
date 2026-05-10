//! IEEE 754-2019 §9.2 `pow` and `cbrt` for [`Decimal64`].
//!
//! `pow(x, y)` follows IEEE 754-2019 §9.2 and the ISO C `pow` rules.
//! `cbrt(x)` is the real cube root, defined for all real x including
//! negatives.

use crate::bid::{classify_bits, Class};
use crate::decimal::Decimal64;
use ferrodec_ieee::{RoundingMode, Status};

impl Decimal64 {
    /// IEEE 754-2019 §9.2 `pow(self, exponent)` rounded by `rm`.
    ///
    /// Special cases (mirrors `f64::powf` semantics):
    /// * `pow(±0, +y)` for finite y > 0 → +0 (with appropriate sign
    ///   for odd-integer y).
    /// * `pow(±0, -y)` for finite y > 0 → ±∞ + `DIV_BY_ZERO`.
    /// * `pow(1, y) = 1` for any y (including NaN).
    /// * `pow(x, 0) = 1` for any x (including NaN).
    /// * `pow(-1, ±∞) = 1`.
    /// * `pow(NaN, y)` and `pow(x, NaN)` propagate NaN unless
    ///   handled above.
    /// * `pow(negative finite, non-integer y)` → NaN + INVALID.
    #[must_use]
    pub fn pow(self, exponent: Self, rm: RoundingMode) -> (Self, Status) {
        // pow(x, 0) = 1, including pow(NaN, 0) = 1.
        if exponent.is_zero() {
            return (Decimal64::ONE, Status::OK);
        }
        // pow(1, y) = 1 (even for y = NaN, including signaling NaN).
        if self.to_bits() == Decimal64::ONE.to_bits() {
            // sNaN exponent still raises INVALID per the §9.2 rule.
            if let Class::SignalingNaN { .. } = classify_bits(exponent.0) {
                return (Decimal64::ONE, Status::INVALID);
            }
            return (Decimal64::ONE, Status::OK);
        }
        // sNaN propagation.
        for arg in [self, exponent] {
            if let Class::SignalingNaN { sign, payload } = classify_bits(arg.0) {
                return (
                    Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                    Status::INVALID,
                );
            }
        }
        // qNaN propagation (a preferred per §6.2.3).
        for arg in [self, exponent] {
            if let Class::QuietNaN { sign, payload } = classify_bits(arg.0) {
                return (
                    Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                    Status::OK,
                );
            }
        }

        // Negative base with non-integer exponent → NaN + INVALID.
        if self.is_sign_negative() && !self.is_zero() {
            // Check if exponent is an integer.
            let y_f = exponent.to_f64();
            if y_f.is_finite() && libm::trunc(y_f) != y_f {
                return (Decimal64::NAN, Status::INVALID);
            }
        }

        let x = self.to_f64();
        let y = exponent.to_f64();
        let r = libm::pow(x, y);
        if r.is_nan() {
            return (Decimal64::NAN, Status::INVALID);
        }
        if r.is_infinite() {
            // Distinguish overflow from divide-by-zero pole. f64 doesn't
            // raise the IEEE flag directly; we infer from the inputs:
            // pow(0, negative) → ±∞ DIV_BY_ZERO.
            if self.is_zero() && y < 0.0 {
                return (
                    if r > 0.0 { Decimal64::INFINITY } else { Decimal64::NEG_INFINITY },
                    Status::DIV_BY_ZERO,
                );
            }
            return (
                if r > 0.0 { Decimal64::INFINITY } else { Decimal64::NEG_INFINITY },
                Status::OVERFLOW | Status::INEXACT,
            );
        }
        if r == 0.0 && !self.is_zero() && y.is_finite() {
            // Underflow.
            return (Decimal64::ZERO, Status::UNDERFLOW | Status::INEXACT);
        }
        let (val, mut status) = Decimal64::from_f64(r, rm);
        if !val.is_zero() {
            status |= Status::INEXACT;
        }
        (val, status)
    }

    /// IEEE 754-2019 §9.2 `cbrt(self)` rounded by `rm`. Defined for
    /// all real x including negatives. `cbrt(±0) = ±0`,
    /// `cbrt(±∞) = ±∞`, NaN propagates.
    #[must_use]
    pub fn cbrt(self, rm: RoundingMode) -> (Self, Status) {
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
            Class::Finite { .. } => {
                let x = self.to_f64();
                let r = libm::cbrt(x);
                let (val, mut status) = Decimal64::from_f64(r, rm);
                if !val.is_zero() {
                    status |= Status::INEXACT;
                }
                (val, status)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn from_int(n: i64, exp: i32) -> Decimal64 {
        Decimal64::try_new(n, exp).unwrap()
    }

    fn approx_equal(a: Decimal64, b: Decimal64) -> bool {
        let af = a.to_f64();
        let bf = b.to_f64();
        let tol = 1e-13;
        (af - bf).abs() <= tol * (1.0 + bf.abs())
    }

    #[test]
    fn pow_basic() {
        // 2^3 = 8
        let (r, _) =
            from_int(2, 0).pow(from_int(3, 0), RoundingMode::NearestEven);
        assert!(approx_equal(r, from_int(8, 0)));

        // 10^2 = 100
        let (r, _) =
            Decimal64::TEN.pow(from_int(2, 0), RoundingMode::NearestEven);
        assert!(approx_equal(r, from_int(100, 0)));
    }

    #[test]
    fn pow_x_zero_is_one() {
        let (r, _) = from_int(5, 0).pow(Decimal64::ZERO, RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), Decimal64::ONE.to_bits());

        // pow(NaN, 0) = 1
        let (r, _) = Decimal64::NAN.pow(Decimal64::ZERO, RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), Decimal64::ONE.to_bits());
    }

    #[test]
    fn pow_one_y_is_one() {
        let (r, _) = Decimal64::ONE.pow(from_int(5, 0), RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), Decimal64::ONE.to_bits());

        let (r, _) = Decimal64::ONE.pow(Decimal64::NAN, RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), Decimal64::ONE.to_bits());
    }

    #[test]
    fn pow_negative_base_non_integer_invalid() {
        // (-2)^0.5 = NaN + INVALID
        let half = Decimal64::parse_str("0.5", RoundingMode::NearestEven).unwrap().0;
        let (r, s) = from_int(-2, 0).pow(half, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());
    }

    #[test]
    fn pow_zero_negative_div_by_zero() {
        // 0^-1 = +∞ + DIV_BY_ZERO
        let (r, s) =
            Decimal64::ZERO.pow(from_int(-1, 0), RoundingMode::NearestEven);
        assert!(r.is_infinite());
        assert!(s.div_by_zero());
    }

    #[test]
    fn pow_overflow() {
        // Decimal64's E_MAX is 384; 10^400 exceeds both Decimal64 and
        // f64 ranges, so libm::pow returns +∞ and pow propagates
        // OVERFLOW.
        let (r, s) =
            Decimal64::TEN.pow(from_int(400, 0), RoundingMode::NearestEven);
        assert!(r.is_infinite());
        assert!(s.overflow() && s.inexact());
    }

    #[test]
    fn cbrt_basic() {
        // cbrt(8) = 2
        let (r, _) = from_int(8, 0).cbrt(RoundingMode::NearestEven);
        assert!(approx_equal(r, from_int(2, 0)));

        // cbrt(-27) = -3
        let (r, _) = from_int(-27, 0).cbrt(RoundingMode::NearestEven);
        assert!(approx_equal(r, from_int(-3, 0)));
    }

    #[test]
    fn cbrt_specials() {
        let (r, _) = Decimal64::ZERO.cbrt(RoundingMode::NearestEven);
        assert!(r.is_zero() && !r.is_sign_negative());

        let (r, _) = Decimal64::NEG_ZERO.cbrt(RoundingMode::NearestEven);
        assert!(r.is_zero() && r.is_sign_negative());

        let (r, _) = Decimal64::INFINITY.cbrt(RoundingMode::NearestEven);
        assert!(r.is_infinite() && !r.is_sign_negative());

        let (r, _) = Decimal64::NEG_INFINITY.cbrt(RoundingMode::NearestEven);
        assert!(r.is_infinite() && r.is_sign_negative());

        let (r, s) = Decimal64::NAN.cbrt(RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.is_ok());
    }
}
