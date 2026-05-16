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

use super::f64_bridge::{f64_unary, f64_unary_via_value};

impl Decimal64 {
    /// IEEE 754-2019 §9.2 `sinh(self)` rounded by `rm`.
    #[must_use]
    pub fn sinh(self, rm: RoundingMode) -> (Self, Status) {
        if let Some(special) = sinh_special_cases(classify_bits(self.0)) {
            return special;
        }
        // Finite non-zero: route through f64.
        f64_unary(self, libm::sinh, rm)
    }

    /// IEEE 754-2019 §9.2 `cosh(self)` rounded by `rm`.
    #[must_use]
    pub fn cosh(self, rm: RoundingMode) -> (Self, Status) {
        if let Some(special) = cosh_special_cases(classify_bits(self.0)) {
            return special;
        }
        // Finite non-zero: route through f64.
        f64_unary(self, libm::cosh, rm)
    }

    /// IEEE 754-2019 §9.2 `tanh(self)` rounded by `rm`.
    #[must_use]
    pub fn tanh(self, rm: RoundingMode) -> (Self, Status) {
        if let Some(special) = tanh_special_cases(classify_bits(self.0)) {
            return special;
        }
        // Finite non-zero: route through f64.
        f64_unary(self, libm::tanh, rm)
    }

    /// IEEE 754-2019 §9.2 `asinh(self)` rounded by `rm`.
    #[must_use]
    pub fn asinh(self, rm: RoundingMode) -> (Self, Status) {
        if let Some(special) = asinh_special_cases(classify_bits(self.0)) {
            return special;
        }
        // Finite non-zero: route through f64.
        f64_unary(self, libm::asinh, rm)
    }

    /// IEEE 754-2019 §9.2 `acosh(self)` rounded by `rm`. Domain:
    /// `[1, +∞)`. Inputs below 1 raise INVALID.
    #[must_use]
    pub fn acosh(self, rm: RoundingMode) -> (Self, Status) {
        if let Some(special) = acosh_special_cases(classify_bits(self.0)) {
            return special;
        }
        // Positive finite non-zero: the `x < 1` domain check depends
        // on the rounded f64 value, so it is part of the f64
        // pipeline `_special_cases` returns `None` for.
        let x = self.to_f64(RoundingMode::NearestEven).0;
        if x < 1.0 {
            return (Decimal64::NAN, Status::INVALID);
        }
        f64_unary_via_value(x, libm::acosh, rm)
    }

    /// IEEE 754-2019 §9.2 `atanh(self)` rounded by `rm`. Domain:
    /// `(-1, +1)`. `atanh(±1) = ±∞ + DIV_BY_ZERO`. Outside the open
    /// interval raises INVALID.
    #[must_use]
    pub fn atanh(self, rm: RoundingMode) -> (Self, Status) {
        if let Some(special) = atanh_special_cases(classify_bits(self.0)) {
            return special;
        }
        // Finite non-zero: the `|x| == 1` pole and `|x| > 1` domain
        // checks depend on the rounded f64 value, so they are part of
        // the f64 pipeline.
        let x = self.to_f64(RoundingMode::NearestEven).0;
        if x.abs() == 1.0 {
            return (
                if x > 0.0 {
                    Decimal64::INFINITY
                } else {
                    Decimal64::NEG_INFINITY
                },
                Status::DIV_BY_ZERO,
            );
        }
        if x.abs() > 1.0 {
            return (Decimal64::NAN, Status::INVALID);
        }
        f64_unary_via_value(x, libm::atanh, rm)
    }

    /// Kani-only entry for the `sinh` special-case branch without the
    /// `libm` + `from_f64` pipeline. CBMC never encodes the f64 path.
    /// ADR-0016.
    #[cfg(kani)]
    #[doc(hidden)]
    #[must_use]
    pub fn sinh_special_only_for_kani(self) -> Option<(Self, Status)> {
        sinh_special_cases(classify_bits(self.0))
    }

    /// Kani-only entry for the `cosh` special-case branch. ADR-0016.
    #[cfg(kani)]
    #[doc(hidden)]
    #[must_use]
    pub fn cosh_special_only_for_kani(self) -> Option<(Self, Status)> {
        cosh_special_cases(classify_bits(self.0))
    }

    /// Kani-only entry for the `tanh` special-case branch. ADR-0016.
    #[cfg(kani)]
    #[doc(hidden)]
    #[must_use]
    pub fn tanh_special_only_for_kani(self) -> Option<(Self, Status)> {
        tanh_special_cases(classify_bits(self.0))
    }

    /// Kani-only entry for the `asinh` special-case branch. ADR-0016.
    #[cfg(kani)]
    #[doc(hidden)]
    #[must_use]
    pub fn asinh_special_only_for_kani(self) -> Option<(Self, Status)> {
        asinh_special_cases(classify_bits(self.0))
    }

    /// Kani-only entry for the `acosh` special-case branch. ADR-0016.
    #[cfg(kani)]
    #[doc(hidden)]
    #[must_use]
    pub fn acosh_special_only_for_kani(self) -> Option<(Self, Status)> {
        acosh_special_cases(classify_bits(self.0))
    }

    /// Kani-only entry for the `atanh` special-case branch. ADR-0016.
    #[cfg(kani)]
    #[doc(hidden)]
    #[must_use]
    pub fn atanh_special_only_for_kani(self) -> Option<(Self, Status)> {
        atanh_special_cases(classify_bits(self.0))
    }
}

/// Resolve every `sinh` input class that does not reach the
/// `libm::sinh` + `from_f64` pipeline. `None` only for finite
/// non-zero. `sinh(±∞) = ±∞`, `sinh(±0) = ±0` (sign preserved).
/// Shared by production `sinh` and the Kani shim so the two cannot
/// drift.
fn sinh_special_cases(class: Class) -> Option<(Decimal64, Status)> {
    match class {
        Class::SignalingNaN { sign, payload } => Some((
            Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::INVALID,
        )),
        Class::QuietNaN { sign, payload } => Some((
            Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::OK,
        )),
        Class::Infinity { sign } => Some((
            if sign {
                Decimal64::NEG_INFINITY
            } else {
                Decimal64::INFINITY
            },
            Status::OK,
        )),
        Class::Zero { sign, .. } => Some((
            if sign {
                Decimal64::NEG_ZERO
            } else {
                Decimal64::ZERO
            },
            Status::OK,
        )),
        Class::Finite { .. } => None,
    }
}

/// Resolve every `cosh` input class that does not reach the
/// `libm::cosh` + `from_f64` pipeline. `None` only for finite
/// non-zero. `cosh(±∞) = +∞`, `cosh(±0) = +1` (even function).
fn cosh_special_cases(class: Class) -> Option<(Decimal64, Status)> {
    match class {
        Class::SignalingNaN { sign, payload } => Some((
            Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::INVALID,
        )),
        Class::QuietNaN { sign, payload } => Some((
            Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::OK,
        )),
        Class::Infinity { .. } => Some((Decimal64::INFINITY, Status::OK)),
        Class::Zero { .. } => Some((Decimal64::ONE, Status::OK)),
        Class::Finite { .. } => None,
    }
}

/// Resolve every `tanh` input class that does not reach the
/// `libm::tanh` + `from_f64` pipeline. `None` only for finite
/// non-zero. `tanh(±∞) = ±1`, `tanh(±0) = ±0` (sign preserved).
fn tanh_special_cases(class: Class) -> Option<(Decimal64, Status)> {
    match class {
        Class::SignalingNaN { sign, payload } => Some((
            Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::INVALID,
        )),
        Class::QuietNaN { sign, payload } => Some((
            Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::OK,
        )),
        Class::Infinity { sign } => Some((
            if sign {
                Decimal64::NEG_ONE
            } else {
                Decimal64::ONE
            },
            Status::OK,
        )),
        Class::Zero { sign, .. } => Some((
            if sign {
                Decimal64::NEG_ZERO
            } else {
                Decimal64::ZERO
            },
            Status::OK,
        )),
        Class::Finite { .. } => None,
    }
}

/// Resolve every `asinh` input class that does not reach the
/// `libm::asinh` + `from_f64` pipeline. `None` only for finite
/// non-zero. `asinh(±∞) = ±∞`, `asinh(±0) = ±0` (sign preserved).
fn asinh_special_cases(class: Class) -> Option<(Decimal64, Status)> {
    match class {
        Class::SignalingNaN { sign, payload } => Some((
            Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::INVALID,
        )),
        Class::QuietNaN { sign, payload } => Some((
            Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::OK,
        )),
        Class::Infinity { sign } => Some((
            if sign {
                Decimal64::NEG_INFINITY
            } else {
                Decimal64::INFINITY
            },
            Status::OK,
        )),
        Class::Zero { sign, .. } => Some((
            if sign {
                Decimal64::NEG_ZERO
            } else {
                Decimal64::ZERO
            },
            Status::OK,
        )),
        Class::Finite { .. } => None,
    }
}

/// Resolve every `acosh` input class that does not reach the
/// `libm::acosh` + `from_f64` pipeline. `None` only for positive
/// finite non-zero; `acosh` is defined on `[1, +∞)`, so `Zero` and
/// any negative finite are pure `NaN + INVALID` specials and only
/// the positive-finite `x < 1` boundary needs the rounded f64 value.
/// `acosh(+∞) = +∞`, `acosh(−∞) = NaN + INVALID`.
fn acosh_special_cases(class: Class) -> Option<(Decimal64, Status)> {
    match class {
        Class::SignalingNaN { sign, payload } => Some((
            Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::INVALID,
        )),
        Class::QuietNaN { sign, payload } => Some((
            Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::OK,
        )),
        Class::Infinity { sign: false } => Some((Decimal64::INFINITY, Status::OK)),
        Class::Infinity { sign: true } => Some((Decimal64::NAN, Status::INVALID)),
        Class::Zero { .. } | Class::Finite { sign: true, .. } => {
            Some((Decimal64::NAN, Status::INVALID))
        }
        Class::Finite { sign: false, .. } => None,
    }
}

/// Resolve every `atanh` input class that does not reach the
/// `libm::atanh` + `from_f64` pipeline. `None` only for finite
/// non-zero; the `|x| == 1` pole (`±∞ + DIV_BY_ZERO`) and `|x| > 1`
/// domain INVALID depend on the rounded f64 value and live on that
/// path. `atanh(±∞) = NaN + INVALID`, `atanh(±0) = ±0`.
fn atanh_special_cases(class: Class) -> Option<(Decimal64, Status)> {
    match class {
        Class::SignalingNaN { sign, payload } => Some((
            Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::INVALID,
        )),
        Class::QuietNaN { sign, payload } => Some((
            Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::OK,
        )),
        Class::Infinity { .. } => Some((Decimal64::NAN, Status::INVALID)),
        Class::Zero { sign, .. } => Some((
            if sign {
                Decimal64::NEG_ZERO
            } else {
                Decimal64::ZERO
            },
            Status::OK,
        )),
        Class::Finite { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_equal(a: Decimal64, b: Decimal64) -> bool {
        let af = a.to_f64(RoundingMode::NearestEven).0;
        let bf = b.to_f64(RoundingMode::NearestEven).0;
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

        let half = Decimal64::parse_str("0.5", RoundingMode::NearestEven)
            .unwrap()
            .0;
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
