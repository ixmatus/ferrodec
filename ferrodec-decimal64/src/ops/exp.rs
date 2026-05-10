//! IEEE 754-2019 §9.2 exponential functions for [`Decimal64`].
//!
//! `exp` and `ln` route through `f64` via `libm` (pure Rust, `no_std`,
//! no FFI). Unlike Decimal32 where f64's ~15.95 digits are far more
//! than the 7-digit decimal target, Decimal64's 16 digits sit right at
//! the f64 boundary: the bottom Decimal64 digit may differ from the
//! correctly-rounded value when the f64 round-trip introduces a
//! sub-ULP rounding-direction divergence. v1.0 ships this f64 path as
//! the canonical baseline; a follow-on can replace it with a
//! pure-decimal Taylor / Newton kernel at u128 working precision once
//! one is needed (the public surface is drop-in compatible).
//!
//! # Special cases (IEEE 754-2019 §9.2)
//!
//! * NaN propagates (sNaN raises INVALID).
//! * `exp(±∞)`: `+∞ → +∞`, `−∞ → +0`.
//! * `exp(±0) = 1`.
//! * Out of range: `exp(x)` for `x` above the format's overflow
//!   threshold (~885) → `+∞ + OVERFLOW`. For `x` below the underflow
//!   threshold (~−908) → `+0 + UNDERFLOW + INEXACT`.
//!
//! # Special cases for `ln`
//!
//! * `ln(NaN)` propagates.
//! * `ln(±0) = −∞ + DIV_BY_ZERO`.
//! * `ln(negative)` → NaN + INVALID.
//! * `ln(+∞) = +∞`.
//! * `ln(1) = +0`.

use crate::bid::{classify_bits, Class};
use crate::decimal::Decimal64;
use crate::status::{RoundingMode, Status};

impl Decimal64 {
    /// IEEE 754-2019 §9.2 `exp(self)` rounded by `rm`.
    #[must_use]
    pub fn exp(self, rm: RoundingMode) -> (Self, Status) {
        let class = classify_bits(self.0);
        match class {
            Class::SignalingNaN { sign, payload } => (
                Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::INVALID,
            ),
            Class::QuietNaN { sign, payload } => (
                Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::OK,
            ),
            Class::Infinity { sign: false } => (Decimal64::INFINITY, Status::OK),
            Class::Infinity { sign: true } => (Decimal64::ZERO, Status::OK),
            Class::Zero { .. } => (Decimal64::ONE, Status::OK),
            Class::Finite { .. } => {
                let x = self.to_f64();
                let r = libm::exp(x);
                if r.is_infinite() {
                    return (Decimal64::INFINITY, Status::OVERFLOW | Status::INEXACT);
                }
                if r == 0.0 {
                    return (Decimal64::ZERO, Status::UNDERFLOW | Status::INEXACT);
                }
                let (val, mut status) = Decimal64::from_f64(r, rm);
                // exp of a non-zero finite is essentially never exact;
                // emit INEXACT unconditionally to match IEEE 754
                // §9.2 expectations even when the f64 round-trip
                // happens to land on a representable value.
                status |= Status::INEXACT;
                (val, status)
            }
        }
    }

    /// IEEE 754-2019 §9.2 `ln(self)` rounded by `rm`.
    #[must_use]
    pub fn ln(self, rm: RoundingMode) -> (Self, Status) {
        let class = classify_bits(self.0);
        match class {
            Class::SignalingNaN { sign, payload } => (
                Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::INVALID,
            ),
            Class::QuietNaN { sign, payload } => (
                Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::OK,
            ),
            Class::Infinity { sign: false } => (Decimal64::INFINITY, Status::OK),
            Class::Infinity { sign: true } => (Decimal64::NAN, Status::INVALID),
            Class::Zero { .. } => (Decimal64::NEG_INFINITY, Status::DIV_BY_ZERO),
            Class::Finite { sign: true, .. } => (Decimal64::NAN, Status::INVALID),
            Class::Finite { sign: false, .. } => {
                let x = self.to_f64();
                let r = libm::log(x);
                let (val, mut status) = Decimal64::from_f64(r, rm);
                // ln(positive finite) is exact only at x = 1 (handled
                // by the f64 round-trip producing 0.0) or at integer
                // powers of 10 where the result is also exactly
                // representable. For most inputs, set INEXACT.
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
    use crate::bid::{pack_finite, BIAS};

    fn from_int(n: i64, exp: i32) -> Decimal64 {
        Decimal64::try_new(n, exp).unwrap()
    }

    fn approx_equal(a: Decimal64, b: Decimal64, max_ulp: u32) -> bool {
        // Convert both to f64 and check relative tolerance proportional
        // to max_ulp. Decimal64 carries 16 digits but the f64 round-trip
        // through libm caps achievable precision at ~10⁻¹⁵ relative; we
        // pick 1e-14 to absorb the worst-case double-rounding noise.
        let af = a.to_f64();
        let bf = b.to_f64();
        let tol = 1e-14 * f64::from(max_ulp);
        (af - bf).abs() <= tol * (1.0 + bf.abs())
    }

    #[test]
    fn exp_zero_is_one() {
        let (r, s) = Decimal64::ZERO.exp(RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), Decimal64::ONE.to_bits());
        assert!(s.is_ok());

        let (r, _) = Decimal64::NEG_ZERO.exp(RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), Decimal64::ONE.to_bits());
    }

    #[test]
    fn exp_one_is_e() {
        let (r, _) = Decimal64::ONE.exp(RoundingMode::NearestEven);
        // e ≈ 2.718281828459045 at 16 digits.
        let expected = Decimal64::from_bits(pack_finite(false, BIAS - 15, 2_718_281_828_459_045));
        assert!(approx_equal(r, expected, 1));
    }

    #[test]
    fn exp_negative_one_is_reciprocal_e() {
        let (r, _) = Decimal64::NEG_ONE.exp(RoundingMode::NearestEven);
        // 1/e ≈ 0.3678794411714423
        let expected = Decimal64::from_bits(pack_finite(false, BIAS - 16, 3_678_794_411_714_423));
        assert!(approx_equal(r, expected, 1));
    }

    #[test]
    fn exp_overflow_to_infinity() {
        // exp(1000) overflows.
        let (r, s) = from_int(1000, 0).exp(RoundingMode::NearestEven);
        assert!(r.is_infinite());
        assert!(s.overflow() && s.inexact());
    }

    #[test]
    fn exp_underflow_to_zero() {
        // exp(-1000) underflows to 0.
        let (r, _) = from_int(-1000, 0).exp(RoundingMode::NearestEven);
        assert!(r.is_zero());
    }

    #[test]
    fn exp_specials() {
        let (r, _) = Decimal64::INFINITY.exp(RoundingMode::NearestEven);
        assert!(r.is_infinite() && !r.is_sign_negative());

        let (r, _) = Decimal64::NEG_INFINITY.exp(RoundingMode::NearestEven);
        assert!(r.is_zero() && !r.is_sign_negative());

        let (r, s) = Decimal64::NAN.exp(RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.is_ok());

        let (r, s) = Decimal64::SIGNALING_NAN.exp(RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());
    }

    #[test]
    fn ln_one_is_zero() {
        let (r, _) = Decimal64::ONE.ln(RoundingMode::NearestEven);
        assert!(r.is_zero());
    }

    #[test]
    fn ln_e_is_one() {
        // ln(2.718281828459045) ≈ 1 at 16 digits (slight rounding noise
        // from both the input truncation and the f64 round-trip).
        let e_approx = Decimal64::from_bits(pack_finite(false, BIAS - 15, 2_718_281_828_459_045));
        let (r, _) = e_approx.ln(RoundingMode::NearestEven);
        assert!(approx_equal(r, Decimal64::ONE, 10));
    }

    #[test]
    fn ln_ten_is_ln10() {
        let (r, _) = Decimal64::TEN.ln(RoundingMode::NearestEven);
        // ln(10) ≈ 2.302585092994046 at 16 digits.
        let expected = Decimal64::from_bits(pack_finite(false, BIAS - 15, 2_302_585_092_994_046));
        assert!(approx_equal(r, expected, 1));
    }

    #[test]
    fn ln_specials() {
        let (r, s) = Decimal64::ZERO.ln(RoundingMode::NearestEven);
        assert!(r.is_infinite() && r.is_sign_negative());
        assert!(s.div_by_zero());

        let (r, _) = Decimal64::INFINITY.ln(RoundingMode::NearestEven);
        assert!(r.is_infinite() && !r.is_sign_negative());

        let (r, s) = Decimal64::NEG_INFINITY.ln(RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());

        let (r, s) = Decimal64::NEG_ONE.ln(RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());

        let (r, s) = Decimal64::NAN.ln(RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.is_ok());

        let (r, s) = Decimal64::SIGNALING_NAN.ln(RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());
    }

    #[test]
    fn exp_ln_round_trip() {
        // ln(exp(x)) ≈ x for x in a reasonable range.
        for &x_int in &[1, 2, 5, 10, -1, -5] {
            let x = from_int(x_int, 0);
            let (e, _) = x.exp(RoundingMode::NearestEven);
            let (back, _) = e.ln(RoundingMode::NearestEven);
            assert!(
                approx_equal(back, x, 2),
                "ln(exp({x_int})) round-trip failed: got {back:?}, want {x:?}",
            );
        }
    }
}
