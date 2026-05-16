//! IEEE 754-2019 §9.2 exponential functions for [`Decimal32`].
//!
//! `exp` and `ln` route through `f64` via `libm` (pure Rust, `no_std`,
//! no FFI). The f64 round-trip introduces at most a faint
//! double-rounding error: f64 carries ~15.95 decimal digits of
//! precision, far more than Decimal32's 7, so the worst-case
//! rounding-direction divergence at the Decimal32 ULP boundary is
//! under 1 ULP. For correctly-rounded transcendentals at every
//! representable input, a future commit can replace the f64 path
//! with a pure-decimal Taylor / Newton kernel — the change is
//! drop-in.
//!
//! # Special cases (IEEE 754-2019 §9.2)
//!
//! * NaN propagates (sNaN raises INVALID).
//! * `exp(±∞)`: `+∞ → +∞`, `−∞ → +0`.
//! * `exp(±0) = 1`.
//! * Out of range: `exp(x)` for `x` above the format's overflow
//!   threshold (~221) → `+∞ + OVERFLOW`. For `x` below the underflow
//!   threshold (~−233) → `+0 + UNDERFLOW + INEXACT`.
//!
//! # Special cases for `ln`
//!
//! * `ln(NaN)` propagates.
//! * `ln(±0) = −∞ + DIV_BY_ZERO`.
//! * `ln(negative)` → NaN + INVALID.
//! * `ln(+∞) = +∞`.
//! * `ln(1) = +0`.

use crate::bid::{classify_bits, Class};
use crate::decimal::Decimal32;
use ferrodec_ieee::{RoundingMode, Status};

impl Decimal32 {
    /// IEEE 754-2019 §9.2 `exp(self)` rounded by `rm`.
    #[must_use]
    pub fn exp(self, rm: RoundingMode) -> (Self, Status) {
        let class = classify_bits(self.0);
        match class {
            Class::SignalingNaN { sign, payload } => (
                Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::INVALID,
            ),
            Class::QuietNaN { sign, payload } => (
                Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::OK,
            ),
            Class::Infinity { sign: false } => (Decimal32::INFINITY, Status::OK),
            Class::Infinity { sign: true } => (Decimal32::ZERO, Status::OK),
            Class::Zero { .. } => (Decimal32::ONE, Status::OK),
            Class::Finite { .. } => {
                let x = self.to_f64();
                let r = libm::exp(x);
                if r.is_infinite() {
                    return (Decimal32::INFINITY, Status::OVERFLOW | Status::INEXACT);
                }
                if r == 0.0 {
                    return (Decimal32::ZERO, Status::UNDERFLOW | Status::INEXACT);
                }
                let (val, mut status) = Decimal32::from_f64(r, rm);
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
                Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::INVALID,
            ),
            Class::QuietNaN { sign, payload } => (
                Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::OK,
            ),
            Class::Infinity { sign: false } => (Decimal32::INFINITY, Status::OK),
            Class::Infinity { sign: true } => (Decimal32::NAN, Status::INVALID),
            Class::Zero { .. } => (Decimal32::NEG_INFINITY, Status::DIV_BY_ZERO),
            Class::Finite { sign: true, .. } => (Decimal32::NAN, Status::INVALID),
            Class::Finite { sign: false, .. } => {
                let x = self.to_f64();
                let r = libm::log(x);
                let (val, mut status) = Decimal32::from_f64(r, rm);
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
    use crate::bid::{pack_finite, BiasedExp, Coefficient, BIAS};

    fn from_int(n: i32, exp: i32) -> Decimal32 {
        Decimal32::try_new(n, exp).unwrap()
    }

    fn approx_equal(a: Decimal32, b: Decimal32, max_ulp: u32) -> bool {
        // Convert both to f64 and check relative tolerance proportional
        // to max_ulp at Decimal32 precision (~10^-7 per ULP).
        let af = a.to_f64();
        let bf = b.to_f64();
        let tol = 1e-6 * f64::from(max_ulp);
        (af - bf).abs() <= tol * (1.0 + bf.abs())
    }

    #[test]
    fn exp_zero_is_one() {
        let (r, s) = Decimal32::ZERO.exp(RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), Decimal32::ONE.to_bits());
        assert!(s.is_ok());

        let (r, _) = Decimal32::NEG_ZERO.exp(RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), Decimal32::ONE.to_bits());
    }

    #[test]
    fn exp_one_is_e() {
        let (r, _) = Decimal32::ONE.exp(RoundingMode::NearestEven);
        // e ≈ 2.718282 at 7 digits.
        let expected = Decimal32::from_bits(pack_finite(
            false,
            BiasedExp::try_from_biased(BIAS - 6).unwrap(),
            Coefficient::try_new(2_718_282).unwrap(),
        ));
        assert!(approx_equal(r, expected, 1));
    }

    #[test]
    fn exp_negative_one_is_reciprocal_e() {
        let (r, _) = Decimal32::NEG_ONE.exp(RoundingMode::NearestEven);
        // 1/e ≈ 0.3678794
        let expected = Decimal32::from_bits(pack_finite(
            false,
            BiasedExp::try_from_biased(BIAS - 7).unwrap(),
            Coefficient::try_new(3_678_794).unwrap(),
        ));
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
        let (r, _) = Decimal32::INFINITY.exp(RoundingMode::NearestEven);
        assert!(r.is_infinite() && !r.is_sign_negative());

        let (r, _) = Decimal32::NEG_INFINITY.exp(RoundingMode::NearestEven);
        assert!(r.is_zero() && !r.is_sign_negative());

        let (r, s) = Decimal32::NAN.exp(RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.is_ok());

        let (r, s) = Decimal32::SIGNALING_NAN.exp(RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());
    }

    #[test]
    fn ln_one_is_zero() {
        let (r, _) = Decimal32::ONE.ln(RoundingMode::NearestEven);
        assert!(r.is_zero());
    }

    #[test]
    fn ln_e_is_one() {
        // ln(2.718282) ≈ 1.000000 at 7 digits (slight rounding).
        let e_approx = Decimal32::from_bits(pack_finite(
            false,
            BiasedExp::try_from_biased(BIAS - 6).unwrap(),
            Coefficient::try_new(2_718_282).unwrap(),
        ));
        let (r, _) = e_approx.ln(RoundingMode::NearestEven);
        assert!(approx_equal(r, Decimal32::ONE, 1));
    }

    #[test]
    fn ln_ten_is_ln10() {
        let (r, _) = Decimal32::TEN.ln(RoundingMode::NearestEven);
        // ln(10) ≈ 2.302585
        let expected = Decimal32::from_bits(pack_finite(
            false,
            BiasedExp::try_from_biased(BIAS - 6).unwrap(),
            Coefficient::try_new(2_302_585).unwrap(),
        ));
        assert!(approx_equal(r, expected, 1));
    }

    #[test]
    fn ln_specials() {
        let (r, s) = Decimal32::ZERO.ln(RoundingMode::NearestEven);
        assert!(r.is_infinite() && r.is_sign_negative());
        assert!(s.div_by_zero());

        let (r, _) = Decimal32::INFINITY.ln(RoundingMode::NearestEven);
        assert!(r.is_infinite() && !r.is_sign_negative());

        let (r, s) = Decimal32::NEG_INFINITY.ln(RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());

        let (r, s) = Decimal32::NEG_ONE.ln(RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());

        let (r, s) = Decimal32::NAN.ln(RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.is_ok());

        let (r, s) = Decimal32::SIGNALING_NAN.ln(RoundingMode::NearestEven);
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
