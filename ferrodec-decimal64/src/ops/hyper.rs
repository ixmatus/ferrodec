//! IEEE 754-2019 §9.2 hyperbolic functions for [`Decimal64`].
//!
//! `sinh`, `cosh`, `tanh`, `asinh`, `acosh`, `atanh` route their
//! finite path through the shared faithful `ferrodec-transcend`
//! Extended-precision kernel: 50-digit `Extended` working precision
//! built on the already-faithful `exp` / `ln` primitives, rounded
//! once at the format boundary, giving faithfully-rounded (≤ 1 ULP
//! at 16 digits) results without the pre-fd-r0l lossy `f64` / `libm`
//! detour. The kernel is the same verified implementation the
//! `ferrodec` (Decimal128) parent uses, instantiated at
//! `F = Decimal64` via the `DecimalFormat` seam.
//!
//! The special-value short-circuits (`sinh_special_cases` etc.) stay
//! in this module ahead of the kernel call: they are shared with the
//! ADR-0016 Kani shims (which must never reach the Extended kernel)
//! and keep Decimal64's special-value semantics byte-identical across
//! the rewire.
//!
//! # Special cases (IEEE 754-2019 §9.2)
//!
//! * `sinh(±0) = ±0`, `sinh(±∞) = ±∞` (sign preserved).
//! * `cosh(±0) = +1`, `cosh(±∞) = +∞` (even function).
//! * `tanh(±0) = ±0`, `tanh(±∞) = ±1` (sign preserved).
//! * `asinh(±0) = ±0`, `asinh(±∞) = ±∞`.
//! * `acosh(1) = 0`. `acosh(x < 1) = NaN + INVALID`,
//!   `acosh(+∞) = +∞`, `acosh(−∞) = NaN + INVALID`.
//! * `atanh(±0) = ±0`. `atanh(±1) = ±∞ + DIV_BY_ZERO`.
//!   `atanh(|x| > 1) = NaN + INVALID`, `atanh(±∞) = NaN + INVALID`.
//!
//! # Range
//!
//! The pre-fd-r0l `f64`-pipeline range cap (`sinh` / `cosh` overflow
//! at `|x| ≳ 710` because `eˣ` saturated `f64` before the Decimal64
//! exponent range was exhausted) is lifted: the kernel computes
//! `eˣ` at Extended precision and saturates only at the format's own
//! `exp_overflow_limit`, so `sinh` / `cosh` are faithful across the
//! full Decimal64 magnitude range up to the true overflow boundary.

use crate::bid::{classify_bits, Class};
use crate::decimal::Decimal64;
use ferrodec_ieee::{RoundingMode, Status};

impl Decimal64 {
    /// IEEE 754-2019 §9.2 `sinh(self)` rounded by `rm`.
    ///
    /// Finite inputs route through the shared faithful
    /// `ferrodec-transcend` Extended-precision kernel (≤ 1 ULP across
    /// the true Decimal64 domain), replacing the pre-fd-r0l lossy
    /// `f64` / `libm::sinh` detour. The `sinh_special_cases`
    /// short-circuit is kept ahead of the kernel call so Decimal64's
    /// special-value semantics (and the ADR-0016 Kani shim, which
    /// shares `sinh_special_cases`) are byte-identical to before;
    /// only the finite result path changes.
    #[must_use]
    pub fn sinh(self, rm: RoundingMode) -> (Self, Status) {
        if let Some(special) = sinh_special_cases(classify_bits(self.0)) {
            return special;
        }
        // Finite non-zero: faithful shared kernel.
        ferrodec_transcend::hyperbolic::sinh_kernel::<Decimal64>(self, rm)
    }

    /// IEEE 754-2019 §9.2 `cosh(self)` rounded by `rm`.
    ///
    /// Finite inputs route through the shared faithful
    /// `ferrodec-transcend` Extended-precision kernel (≤ 1 ULP across
    /// the true Decimal64 domain), replacing the pre-fd-r0l lossy
    /// `f64` / `libm::cosh` detour. The `cosh_special_cases`
    /// short-circuit is kept ahead of the kernel call so Decimal64's
    /// special-value semantics (and the ADR-0016 Kani shim, which
    /// shares `cosh_special_cases`) are byte-identical to before;
    /// only the finite result path changes.
    #[must_use]
    pub fn cosh(self, rm: RoundingMode) -> (Self, Status) {
        if let Some(special) = cosh_special_cases(classify_bits(self.0)) {
            return special;
        }
        // Finite non-zero: faithful shared kernel.
        ferrodec_transcend::hyperbolic::cosh_kernel::<Decimal64>(self, rm)
    }

    /// IEEE 754-2019 §9.2 `tanh(self)` rounded by `rm`.
    ///
    /// Finite inputs route through the shared faithful
    /// `ferrodec-transcend` Extended-precision kernel (≤ 1 ULP across
    /// the true Decimal64 domain), replacing the pre-fd-r0l lossy
    /// `f64` / `libm::tanh` detour. The `tanh_special_cases`
    /// short-circuit is kept ahead of the kernel call so Decimal64's
    /// special-value semantics (and the ADR-0016 Kani shim, which
    /// shares `tanh_special_cases`) are byte-identical to before;
    /// only the finite result path changes.
    #[must_use]
    pub fn tanh(self, rm: RoundingMode) -> (Self, Status) {
        if let Some(special) = tanh_special_cases(classify_bits(self.0)) {
            return special;
        }
        // Finite non-zero: faithful shared kernel.
        ferrodec_transcend::hyperbolic::tanh_kernel::<Decimal64>(self, rm)
    }

    /// IEEE 754-2019 §9.2 `asinh(self)` rounded by `rm`.
    ///
    /// Finite inputs route through the shared faithful
    /// `ferrodec-transcend` Extended-precision kernel (≤ 1 ULP across
    /// the true Decimal64 domain), replacing the pre-fd-r0l lossy
    /// `f64` / `libm::asinh` detour. The `asinh_special_cases`
    /// short-circuit is kept ahead of the kernel call so Decimal64's
    /// special-value semantics (and the ADR-0016 Kani shim, which
    /// shares `asinh_special_cases`) are byte-identical to before;
    /// only the finite result path changes.
    #[must_use]
    pub fn asinh(self, rm: RoundingMode) -> (Self, Status) {
        if let Some(special) = asinh_special_cases(classify_bits(self.0)) {
            return special;
        }
        // Finite non-zero: faithful shared kernel.
        ferrodec_transcend::hyperbolic::asinh_kernel::<Decimal64>(self, rm)
    }

    /// IEEE 754-2019 §9.2 `acosh(self)` rounded by `rm`. Domain:
    /// `[1, +∞)`. Inputs below 1 raise INVALID.
    ///
    /// Finite inputs route through the shared faithful
    /// `ferrodec-transcend` Extended-precision kernel (≤ 1 ULP across
    /// the true Decimal64 domain), replacing the pre-fd-r0l lossy
    /// `f64` / `libm::acosh` detour. The `x < 1` domain INVALID is
    /// now decided inside the kernel at Extended precision (not on a
    /// rounded f64 value), so the `acosh_special_cases` short-circuit
    /// is kept ahead of the kernel for the NaN / Inf / zero /
    /// negative-finite classes only; the ADR-0016 Kani shim still
    /// shares `acosh_special_cases`.
    #[must_use]
    pub fn acosh(self, rm: RoundingMode) -> (Self, Status) {
        if let Some(special) = acosh_special_cases(classify_bits(self.0)) {
            return special;
        }
        // Positive finite non-zero: faithful shared kernel (the
        // kernel decides the `x < 1` domain INVALID and
        // `acosh(1) = 0` at Extended precision).
        ferrodec_transcend::hyperbolic::acosh_kernel::<Decimal64>(self, rm)
    }

    /// IEEE 754-2019 §9.2 `atanh(self)` rounded by `rm`. Domain:
    /// `(-1, +1)`. `atanh(±1) = ±∞ + DIV_BY_ZERO`. Outside the open
    /// interval raises INVALID.
    ///
    /// Finite inputs route through the shared faithful
    /// `ferrodec-transcend` Extended-precision kernel (≤ 1 ULP across
    /// the true Decimal64 domain), replacing the pre-fd-r0l lossy
    /// `f64` / `libm::atanh` detour. The `|x| == 1` pole
    /// (`±∞ + DIV_BY_ZERO`) and the `|x| > 1` domain INVALID are now
    /// decided inside the kernel at Extended precision (not on a
    /// rounded f64 value), so the `atanh_special_cases` short-circuit
    /// is kept ahead of the kernel for the NaN / Inf / zero classes
    /// only; the ADR-0016 Kani shim still shares
    /// `atanh_special_cases`.
    #[must_use]
    pub fn atanh(self, rm: RoundingMode) -> (Self, Status) {
        if let Some(special) = atanh_special_cases(classify_bits(self.0)) {
            return special;
        }
        // Finite non-zero: faithful shared kernel (the kernel decides
        // the `|x| == 1` pole and the `|x| > 1` domain INVALID at
        // Extended precision).
        ferrodec_transcend::hyperbolic::atanh_kernel::<Decimal64>(self, rm)
    }

    /// Kani-only entry for the `sinh` special-case branch without
    /// invoking the `ferrodec-transcend` Extended-precision kernel.
    /// CBMC cannot tractably encode the bignum kernel path. ADR-0016.
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

/// Resolve every `sinh` input class the `ferrodec-transcend`
/// Extended-precision kernel does not need to see. `None` only for
/// finite non-zero. `sinh(±∞) = ±∞`, `sinh(±0) = ±0` (sign
/// preserved). Shared by production `sinh` and the Kani shim so the
/// two cannot drift.
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

/// Resolve every `cosh` input class the `ferrodec-transcend`
/// Extended-precision kernel does not need to see. `None` only for
/// finite non-zero. `cosh(±∞) = +∞`, `cosh(±0) = +1` (even
/// function).
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

/// Resolve every `tanh` input class the `ferrodec-transcend`
/// Extended-precision kernel does not need to see. `None` only for
/// finite non-zero. `tanh(±∞) = ±1`, `tanh(±0) = ±0` (sign
/// preserved).
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

/// Resolve every `asinh` input class the `ferrodec-transcend`
/// Extended-precision kernel does not need to see. `None` only for
/// finite non-zero. `asinh(±∞) = ±∞`, `asinh(±0) = ±0` (sign
/// preserved).
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

/// Resolve every `acosh` input class the `ferrodec-transcend`
/// Extended-precision kernel does not need to see. `None` only for
/// positive finite non-zero; `acosh` is defined on `[1, +∞)`, so
/// `Zero` and any negative finite are pure `NaN + INVALID` specials
/// and only the positive-finite `x < 1` boundary is decided by the
/// kernel at Extended precision. `acosh(+∞) = +∞`,
/// `acosh(−∞) = NaN + INVALID`.
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

/// Resolve every `atanh` input class the `ferrodec-transcend`
/// Extended-precision kernel does not need to see. `None` only for
/// finite non-zero; the `|x| == 1` pole (`±∞ + DIV_BY_ZERO`) and
/// `|x| > 1` domain INVALID are decided by the kernel at Extended
/// precision. `atanh(±∞) = NaN + INVALID`, `atanh(±0) = ±0`.
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
