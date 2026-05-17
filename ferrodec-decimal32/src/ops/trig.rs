//! IEEE 754-2019 §9.2 trigonometric functions for [`Decimal32`].
//!
//! `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2` route their
//! finite path through the shared faithful `ferrodec-transcend`
//! Extended-precision kernel: 50-digit `Extended` working precision
//! with Payne-Hanek argument reduction for `sin` / `cos` / `tan`,
//! rounded once at the format boundary, giving faithfully-rounded
//! (≤ 1 ULP at 7 digits) results without the pre-fd-r0l lossy
//! `f64` / `libm` detour. The kernel is the same verified
//! implementation the `ferrodec` (Decimal128) parent and the
//! `ferrodec-decimal64` sibling use, instantiated at `F = Decimal32`
//! via the `DecimalFormat` seam.
//!
//! The special-value short-circuits (`sin_special_cases` etc.) stay
//! in this module ahead of the kernel call: they are shared with the
//! ADR-0016 Kani shims (which must never reach the Extended kernel)
//! and keep Decimal32's special-value semantics byte-identical across
//! the rewire.
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
//! * `atan2(y, x)` follows the IEEE 754-2019 §9.2.1 quadrant
//!   conventions; NaN inputs produce NaN.
//!
//! # Argument reduction
//!
//! `sin` / `cos` / `tan` reduce the argument with the same
//! Payne-Hanek module the Decimal128 parent uses, which
//! parameterises correctly to Decimal32's narrower exponent range
//! (the spike fd-57z confirmed the 2/π table over-covers the
//! siblings). The reduction is faithful across the full Decimal32
//! magnitude range, replacing the pre-fd-r0l f64-round-trip
//! accuracy limitation.

use crate::bid::{classify_bits, Class};
use crate::decimal::Decimal32;
use ferrodec_ieee::{RoundingMode, Status};

impl Decimal32 {
    /// IEEE 754-2019 §9.2 `sin(self)` rounded by `rm`.
    ///
    /// Finite inputs route through the shared faithful
    /// `ferrodec-transcend` Extended-precision kernel (Payne-Hanek
    /// reduction, ≤ 1 ULP across the true Decimal32 domain),
    /// replacing the pre-fd-r0l lossy `f64` / `libm::sin` detour. The
    /// `sin_special_cases` short-circuit is kept ahead of the kernel
    /// call so Decimal32's special-value semantics (and the ADR-0016
    /// Kani shim, which shares `sin_special_cases`) are byte-identical
    /// to before; only the finite result path changes.
    #[must_use]
    pub fn sin(self, rm: RoundingMode) -> (Self, Status) {
        if let Some(special) = sin_special_cases(classify_bits(self.0)) {
            return special;
        }
        // Finite non-zero: faithful shared kernel.
        ferrodec_transcend::sincos::sin_kernel::<Decimal32>(self, rm)
    }

    /// IEEE 754-2019 §9.2 `cos(self)` rounded by `rm`.
    ///
    /// Finite inputs route through the shared faithful
    /// `ferrodec-transcend` Extended-precision kernel (Payne-Hanek
    /// reduction, ≤ 1 ULP across the true Decimal32 domain),
    /// replacing the pre-fd-r0l lossy `f64` / `libm::cos` detour. The
    /// `cos_special_cases` short-circuit is kept ahead of the kernel
    /// call so Decimal32's special-value semantics (and the ADR-0016
    /// Kani shim, which shares `cos_special_cases`) are byte-identical
    /// to before; only the finite result path changes.
    #[must_use]
    pub fn cos(self, rm: RoundingMode) -> (Self, Status) {
        if let Some(special) = cos_special_cases(classify_bits(self.0)) {
            return special;
        }
        // Finite non-zero: faithful shared kernel.
        ferrodec_transcend::sincos::cos_kernel::<Decimal32>(self, rm)
    }

    /// IEEE 754-2019 §9.2 `tan(self)` rounded by `rm`.
    ///
    /// Finite inputs route through the shared faithful
    /// `ferrodec-transcend` Extended-precision kernel (Payne-Hanek
    /// reduction, ≤ 1 ULP across the true Decimal32 domain),
    /// replacing the pre-fd-r0l lossy `f64` / `libm::tan` detour. The
    /// `tan_special_cases` short-circuit is kept ahead of the kernel
    /// call so Decimal32's special-value semantics (and the ADR-0016
    /// Kani shim, which shares `tan_special_cases`) are byte-identical
    /// to before; only the finite result path changes.
    #[must_use]
    pub fn tan(self, rm: RoundingMode) -> (Self, Status) {
        if let Some(special) = tan_special_cases(classify_bits(self.0)) {
            return special;
        }
        // Finite non-zero: faithful shared kernel.
        ferrodec_transcend::sincos::tan_kernel::<Decimal32>(self, rm)
    }

    /// IEEE 754-2019 §9.2 `asin(self)` rounded by `rm`.
    /// Domain: `[-1, +1]`. Outside the domain raises INVALID.
    ///
    /// Finite inputs route through the shared faithful
    /// `ferrodec-transcend` Extended-precision kernel (≤ 1 ULP across
    /// the true Decimal32 domain), replacing the pre-fd-r0l lossy
    /// `f64` / `libm::asin` detour. The `|x| > 1` domain INVALID is
    /// now decided inside the kernel at Extended precision (not on a
    /// rounded f64 value); the `asin_special_cases` short-circuit is
    /// kept ahead of the kernel for the NaN / Inf / zero classes and
    /// the ADR-0016 Kani shim still shares `asin_special_cases`.
    #[must_use]
    pub fn asin(self, rm: RoundingMode) -> (Self, Status) {
        if let Some(special) = asin_special_cases(classify_bits(self.0)) {
            return special;
        }
        // Finite non-zero: faithful shared kernel (the kernel decides
        // the `|x| > 1` domain INVALID at Extended precision).
        ferrodec_transcend::inverse_trig::asin_kernel::<Decimal32>(self, rm)
    }

    /// IEEE 754-2019 §9.2 `acos(self)` rounded by `rm`.
    /// Domain: `[-1, +1]`. Outside the domain raises INVALID.
    ///
    /// Finite inputs route through the shared faithful
    /// `ferrodec-transcend` Extended-precision kernel (≤ 1 ULP across
    /// the true Decimal32 domain), replacing the pre-fd-r0l lossy
    /// `f64` / `libm::acos` detour. The `|x| > 1` domain INVALID is
    /// now decided inside the kernel at Extended precision (not on a
    /// rounded f64 value); the `acos_special_cases` short-circuit is
    /// kept ahead of the kernel for the NaN / Inf classes and the
    /// ADR-0016 Kani shim still shares it.
    #[must_use]
    pub fn acos(self, rm: RoundingMode) -> (Self, Status) {
        if let Some(special) = acos_special_cases(classify_bits(self.0)) {
            return special;
        }
        // `Zero` and finite non-zero: faithful shared kernel (the
        // kernel decides the `|x| > 1` domain INVALID at Extended
        // precision and `acos(±0) = π/2`).
        ferrodec_transcend::inverse_trig::acos_kernel::<Decimal32>(self, rm)
    }

    /// IEEE 754-2019 §9.2 `atan(self)` rounded by `rm`.
    ///
    /// Finite inputs and `±∞` route through the shared faithful
    /// `ferrodec-transcend` Extended-precision kernel (≤ 1 ULP across
    /// the true Decimal32 domain, `atan(±∞) = ±π/2`), replacing the
    /// pre-fd-r0l lossy `f64` / `libm::atan` detour. The
    /// `atan_special_cases` short-circuit is kept ahead of the kernel
    /// for the NaN / zero classes (it returns `None` for `Infinity`
    /// and finite non-zero, the cases the kernel resolves); the
    /// ADR-0016 Kani shim still shares `atan_special_cases`.
    #[must_use]
    pub fn atan(self, rm: RoundingMode) -> (Self, Status) {
        if let Some(special) = atan_special_cases(classify_bits(self.0)) {
            return special;
        }
        // `Infinity` and finite non-zero: faithful shared kernel
        // (the kernel computes `atan(±∞) = ±π/2`).
        ferrodec_transcend::inverse_trig::atan_kernel::<Decimal32>(self, rm)
    }

    /// `atan2(self, x)` — the angle whose tangent is `self / x`,
    /// resolved into the correct quadrant by the signs of both
    /// arguments. Returns radians in `(-π, π]`. Special cases follow
    /// the IEEE 754-2019 §9.2.1 quadrant convention (NaN propagates,
    /// axis cases are exact). When either operand is NaN the operands
    /// are inspected in the fixed order `[self, x]`; the first NaN
    /// encountered determines the result, pinning IEEE 754-2019
    /// §6.2.3 ordering.
    ///
    /// Finite, infinite, and zero operands route through the shared
    /// faithful `ferrodec-transcend` Extended-precision kernel (≤ 1
    /// ULP), replacing the pre-fd-r0l lossy `f64` / `libm::atan2`
    /// detour. The `atan2_special_cases` short-circuit is kept ahead
    /// of the kernel for the NaN-propagation branch only (it returns
    /// `None` when neither operand is NaN, the single case the kernel
    /// resolves); the ADR-0016 Kani shim still shares
    /// `atan2_special_cases`.
    #[must_use]
    pub fn atan2(self, x: Self, rm: RoundingMode) -> (Self, Status) {
        if let Some(special) = atan2_special_cases(self, x) {
            return special;
        }
        // Neither operand is NaN: faithful shared kernel.
        ferrodec_transcend::inverse_trig::atan2_kernel::<Decimal32>(self, x, rm)
    }

    /// Kani-only entry returning the `sin` special-case branch without
    /// invoking the `libm::sin` + `from_f64` pipeline. CBMC never
    /// encodes the f64 path. ADR-0016.
    #[cfg(kani)]
    #[doc(hidden)]
    #[must_use]
    pub fn sin_special_only_for_kani(self) -> Option<(Self, Status)> {
        sin_special_cases(classify_bits(self.0))
    }

    /// Kani-only entry for the `cos` special-case branch. ADR-0016.
    #[cfg(kani)]
    #[doc(hidden)]
    #[must_use]
    pub fn cos_special_only_for_kani(self) -> Option<(Self, Status)> {
        cos_special_cases(classify_bits(self.0))
    }

    /// Kani-only entry for the `tan` special-case branch. ADR-0016.
    #[cfg(kani)]
    #[doc(hidden)]
    #[must_use]
    pub fn tan_special_only_for_kani(self) -> Option<(Self, Status)> {
        tan_special_cases(classify_bits(self.0))
    }

    /// Kani-only entry for the `asin` special-case branch. ADR-0016.
    #[cfg(kani)]
    #[doc(hidden)]
    #[must_use]
    pub fn asin_special_only_for_kani(self) -> Option<(Self, Status)> {
        asin_special_cases(classify_bits(self.0))
    }

    /// Kani-only entry for the `acos` special-case branch. ADR-0016.
    #[cfg(kani)]
    #[doc(hidden)]
    #[must_use]
    pub fn acos_special_only_for_kani(self) -> Option<(Self, Status)> {
        acos_special_cases(classify_bits(self.0))
    }

    /// Kani-only entry for the `atan` special-case branch. ADR-0016.
    #[cfg(kani)]
    #[doc(hidden)]
    #[must_use]
    pub fn atan_special_only_for_kani(self) -> Option<(Self, Status)> {
        atan_special_cases(classify_bits(self.0))
    }

    /// Kani-only entry for the binary `atan2` NaN-propagation branch.
    /// ADR-0016.
    #[cfg(kani)]
    #[doc(hidden)]
    #[must_use]
    pub fn atan2_special_only_for_kani(self, x: Self) -> Option<(Self, Status)> {
        atan2_special_cases(self, x)
    }
}

/// Resolve every `sin` input class that does not reach the
/// `libm::sin` + `from_f64` pipeline. `None` only for finite
/// non-zero. Shared by production `sin` and the Kani shim so the two
/// cannot drift.
fn sin_special_cases(class: Class) -> Option<(Decimal32, Status)> {
    match class {
        Class::SignalingNaN { sign, payload } => Some((
            Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::INVALID,
        )),
        Class::QuietNaN { sign, payload } => Some((
            Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::OK,
        )),
        Class::Infinity { .. } => Some((Decimal32::NAN, Status::INVALID)),
        Class::Zero { sign, .. } => Some((
            if sign {
                Decimal32::NEG_ZERO
            } else {
                Decimal32::ZERO
            },
            Status::OK,
        )),
        Class::Finite { .. } => None,
    }
}

/// Resolve every `cos` input class that does not reach the
/// `libm::cos` + `from_f64` pipeline. `None` only for finite
/// non-zero. `cos(±0) = +1` (sign not preserved, unlike `sin`).
fn cos_special_cases(class: Class) -> Option<(Decimal32, Status)> {
    match class {
        Class::SignalingNaN { sign, payload } => Some((
            Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::INVALID,
        )),
        Class::QuietNaN { sign, payload } => Some((
            Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::OK,
        )),
        Class::Infinity { .. } => Some((Decimal32::NAN, Status::INVALID)),
        Class::Zero { .. } => Some((Decimal32::ONE, Status::OK)),
        Class::Finite { .. } => None,
    }
}

/// Resolve every `tan` input class that does not reach the
/// `libm::tan` + `from_f64` pipeline. `None` only for finite
/// non-zero. Same special-case shape as `sin`.
fn tan_special_cases(class: Class) -> Option<(Decimal32, Status)> {
    match class {
        Class::SignalingNaN { sign, payload } => Some((
            Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::INVALID,
        )),
        Class::QuietNaN { sign, payload } => Some((
            Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::OK,
        )),
        Class::Infinity { .. } => Some((Decimal32::NAN, Status::INVALID)),
        Class::Zero { sign, .. } => Some((
            if sign {
                Decimal32::NEG_ZERO
            } else {
                Decimal32::ZERO
            },
            Status::OK,
        )),
        Class::Finite { .. } => None,
    }
}

/// Resolve every `asin` input class that does not reach the
/// `libm::asin` + `from_f64` pipeline. `None` only for finite
/// non-zero; the `|x| > 1` domain INVALID is part of that f64 path
/// (it depends on the rounded f64 value), not a pure special.
fn asin_special_cases(class: Class) -> Option<(Decimal32, Status)> {
    match class {
        Class::SignalingNaN { sign, payload } => Some((
            Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::INVALID,
        )),
        Class::QuietNaN { sign, payload } => Some((
            Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::OK,
        )),
        Class::Infinity { .. } => Some((Decimal32::NAN, Status::INVALID)),
        Class::Zero { sign, .. } => Some((
            if sign {
                Decimal32::NEG_ZERO
            } else {
                Decimal32::ZERO
            },
            Status::OK,
        )),
        Class::Finite { .. } => None,
    }
}

/// Resolve every `acos` input class that does not reach the
/// `libm::acos` + `from_f64` pipeline. `None` for both `Zero` and
/// finite non-zero: `acos` has no exact zero-result special, and the
/// `|x| > 1` domain check depends on the rounded f64 value.
fn acos_special_cases(class: Class) -> Option<(Decimal32, Status)> {
    match class {
        Class::SignalingNaN { sign, payload } => Some((
            Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::INVALID,
        )),
        Class::QuietNaN { sign, payload } => Some((
            Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::OK,
        )),
        Class::Infinity { .. } => Some((Decimal32::NAN, Status::INVALID)),
        Class::Zero { .. } | Class::Finite { .. } => None,
    }
}

/// Resolve every `atan` input class that does not reach `libm::atan`.
/// `None` for both `Infinity` and finite non-zero: `atan(±∞) = ±π/2`
/// is computed by `libm::atan(±inf)`, not a pure special.
fn atan_special_cases(class: Class) -> Option<(Decimal32, Status)> {
    match class {
        Class::SignalingNaN { sign, payload } => Some((
            Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::INVALID,
        )),
        Class::QuietNaN { sign, payload } => Some((
            Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::OK,
        )),
        Class::Zero { sign, .. } => Some((
            if sign {
                Decimal32::NEG_ZERO
            } else {
                Decimal32::ZERO
            },
            Status::OK,
        )),
        Class::Infinity { .. } | Class::Finite { .. } => None,
    }
}

/// Resolve the binary `atan2` NaN-propagation branch. The operands
/// are inspected in the fixed order `[y, x]`; the first NaN
/// encountered determines the result (signaling → INVALID, quiet →
/// OK), pinning the IEEE 754-2019 §6.2.3 ordering. `None` when
/// neither operand is NaN, the single case that reaches the
/// `libm::atan2` + `from_f64` pipeline. Shared by production `atan2`
/// and the Kani shim so the two cannot drift.
fn atan2_special_cases(y: Decimal32, x: Decimal32) -> Option<(Decimal32, Status)> {
    for arg in [y, x] {
        match classify_bits(arg.0) {
            Class::SignalingNaN { sign, payload } => {
                return Some((
                    Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                    Status::INVALID,
                ));
            }
            Class::QuietNaN { sign, payload } => {
                return Some((
                    Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                    Status::OK,
                ));
            }
            _ => {}
        }
    }
    None
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
