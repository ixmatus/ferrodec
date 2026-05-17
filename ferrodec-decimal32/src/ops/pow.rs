//! IEEE 754-2019 §9.2 `pow` and `cbrt` for [`Decimal32`].
//!
//! `pow(x, y)` follows IEEE 754-2019 §9.2 and the ISO C `pow` rules.
//! `cbrt(x)` is the real cube root, defined for all real x including
//! negatives.

use crate::bid::{classify_bits, Class, BIAS};
use crate::decimal::Decimal32;
use ferrodec_ieee::{RoundingMode, Status};

/// `true` iff `d` numerically equals `+1`, regardless of cohort.
///
/// Matches every Form A / Form B encoding of `1 × 10⁰`, `10 × 10⁻¹`,
/// `100 × 10⁻²`, ..., up to the largest power-of-10 coefficient that
/// fits in 7 digits (`10⁶ × 10⁻⁶`).
fn equals_one(d: Decimal32) -> bool {
    if let Class::Finite {
        sign: false,
        biased_exp,
        coefficient,
    } = classify_bits(d.0)
    {
        let exp = biased_exp as i32 - BIAS as i32;
        if exp > 0 {
            return false;
        }
        let k = (-exp) as u32;
        // Coefficient must equal 10^k. k is bounded by PRECISION-1 = 6
        // for the largest power-of-10 cohort that fits.
        if k > 6 {
            return false;
        }
        return coefficient == 10u32.pow(k);
    }
    false
}

impl Decimal32 {
    /// IEEE 754-2019 §9.2 `pow(self, exponent)` rounded by `rm`.
    ///
    /// The finite non-special path routes through the shared faithful
    /// `ferrodec-transcend` Extended-precision kernel: `pow(x, y)` is
    /// evaluated as `exp(y · ln(|x|))` entirely at `Extended` working
    /// precision (with the bit-exact integer-exponent fast path the
    /// Decimal128 parent proves), rounded once at the format boundary,
    /// giving faithfully-rounded (≤ 1 ULP at 7 digits) results
    /// without the pre-fd-r0l lossy `f64` / `libm::pow` detour. The
    /// kernel is the same verified implementation the `ferrodec`
    /// (Decimal128) parent uses, instantiated at `F = Decimal32` via
    /// the `DecimalFormat` seam; it decides the negative-base /
    /// non-integer-exponent INVALID at `Extended` precision rather
    /// than on a rounded `f64` exponent, so the dead f64 domain
    /// plumbing is removed.
    ///
    /// The `pow_special_cases` short-circuit is kept ahead of the
    /// kernel call so Decimal32's special-value semantics (and the
    /// ADR-0016 Kani shim, which shares `pow_special_cases`) are
    /// byte-identical to before; only the finite non-special result
    /// path changes.
    ///
    /// Special cases (IEEE 754-2019 §9.2):
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
        if let Some(special) = pow_special_cases(self, exponent) {
            return special;
        }

        // Finite non-special: faithful shared kernel. The kernel
        // resolves the negative-base / non-integer-exponent INVALID
        // and the integer-exponent fast path at Extended precision.
        ferrodec_transcend::pow::pow_kernel::<Decimal32>(self, exponent, rm)
    }

    /// IEEE 754-2019 §9.2 `cbrt(self)` rounded by `rm`. Defined for
    /// all real x including negatives. `cbrt(±0) = ±0`,
    /// `cbrt(±∞) = ±∞`, NaN propagates.
    ///
    /// The finite non-zero path routes through the shared faithful
    /// `ferrodec-transcend` Extended-precision kernel (≤ 1 ULP at 7
    /// digits), replacing the pre-fd-r0l lossy `f64` / `libm::cbrt`
    /// detour. The `cbrt_special_cases` short-circuit is kept ahead of
    /// the kernel call so Decimal32's special-value semantics (and the
    /// ADR-0016 Kani shim, which shares `cbrt_special_cases`) are
    /// byte-identical to before; only the finite non-zero result path
    /// changes.
    #[must_use]
    pub fn cbrt(self, rm: RoundingMode) -> (Self, Status) {
        if let Some(special) = cbrt_special_cases(classify_bits(self.0)) {
            return special;
        }
        // Finite non-zero: faithful shared kernel.
        ferrodec_transcend::cbrt::cbrt_kernel::<Decimal32>(self, rm)
    }

    /// Kani-only entry for the binary `pow` special-case branch
    /// without invoking the negative-base integer test or the
    /// `libm::pow` + `from_f64` pipeline. CBMC never encodes the f64
    /// path. ADR-0016.
    #[cfg(kani)]
    #[doc(hidden)]
    #[must_use]
    pub fn pow_special_only_for_kani(self, exponent: Self) -> Option<(Self, Status)> {
        pow_special_cases(self, exponent)
    }

    /// Kani-only entry for the `cbrt` special-case branch. ADR-0016.
    #[cfg(kani)]
    #[doc(hidden)]
    #[must_use]
    pub fn cbrt_special_only_for_kani(self) -> Option<(Self, Status)> {
        cbrt_special_cases(classify_bits(self.0))
    }

    /// Kani-only accessor for the value-equality `+1` predicate, so
    /// the resolution-set harness keys on numeric value (every cohort
    /// of `1`), not on the canonical `ONE` bit pattern. ADR-0016.
    #[cfg(kani)]
    #[doc(hidden)]
    #[must_use]
    pub fn equals_one_for_kani(self) -> bool {
        equals_one(self)
    }
}

/// Resolve every `pow` input combination that does not reach the
/// negative-base integer test or the `libm::pow` + `from_f64`
/// pipeline. Returns `None` for the fall-through (a base / exponent
/// pair that needs the f64 path, including the negative-base
/// non-integer INVALID, which depends on the rounded f64 exponent).
/// The resolution order is fixed and mirrors IEEE 754-2019 §9.2:
/// `pow(x, 0) = 1` (even `pow(NaN, 0)`); then `pow(1, y) = 1` by
/// value not cohort (`sNaN` exponent still raises INVALID); then
/// `sNaN` propagation over `[base, exponent]`; then `qNaN`
/// propagation. Shared by production `pow` and the Kani shim so the
/// two cannot drift.
fn pow_special_cases(base: Decimal32, exponent: Decimal32) -> Option<(Decimal32, Status)> {
    // pow(x, 0) = 1, including pow(NaN, 0) = 1.
    if exponent.is_zero() {
        return Some((Decimal32::ONE, Status::OK));
    }
    // pow(1, y) = 1 (even for y = NaN, including signaling NaN).
    // §9.2 ties this to *value*, not cohort, so we must catch every
    // cohort of 1 (`1×10⁰`, `10×10⁻¹`, `100×10⁻²`, ...), not just the
    // canonical bit pattern.
    if equals_one(base) {
        // sNaN exponent still raises INVALID per the §9.2 rule.
        if let Class::SignalingNaN { .. } = classify_bits(exponent.0) {
            return Some((Decimal32::ONE, Status::INVALID));
        }
        return Some((Decimal32::ONE, Status::OK));
    }
    // sNaN propagation.
    for arg in [base, exponent] {
        if let Class::SignalingNaN { sign, payload } = classify_bits(arg.0) {
            return Some((
                Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::INVALID,
            ));
        }
    }
    // qNaN propagation (a preferred per §6.2.3).
    for arg in [base, exponent] {
        if let Class::QuietNaN { sign, payload } = classify_bits(arg.0) {
            return Some((
                Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::OK,
            ));
        }
    }
    None
}

/// Resolve every `cbrt` input class that does not reach the
/// `libm::cbrt` + `from_f64` pipeline. `None` only for finite
/// non-zero. `cbrt(±∞) = ±∞`, `cbrt(±0) = ±0` (sign preserved); the
/// real cube root has no domain restriction. Shared by production
/// `cbrt` and the Kani shim so the two cannot drift.
fn cbrt_special_cases(class: Class) -> Option<(Decimal32, Status)> {
    match class {
        Class::SignalingNaN { sign, payload } => Some((
            Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::INVALID,
        )),
        Class::QuietNaN { sign, payload } => Some((
            Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::OK,
        )),
        Class::Infinity { sign } => Some((
            if sign {
                Decimal32::NEG_INFINITY
            } else {
                Decimal32::INFINITY
            },
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
        Class::Finite { .. } => None,
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
    fn pow_basic() {
        // 2^3 = 8
        let (r, _) = from_int(2, 0).pow(from_int(3, 0), RoundingMode::NearestEven);
        assert!(approx_equal(r, from_int(8, 0)));

        // 10^2 = 100
        let (r, _) = Decimal32::TEN.pow(from_int(2, 0), RoundingMode::NearestEven);
        assert!(approx_equal(r, from_int(100, 0)));
    }

    #[test]
    fn pow_x_zero_is_one() {
        let (r, _) = from_int(5, 0).pow(Decimal32::ZERO, RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), Decimal32::ONE.to_bits());

        // pow(NaN, 0) = 1
        let (r, _) = Decimal32::NAN.pow(Decimal32::ZERO, RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), Decimal32::ONE.to_bits());
    }

    #[test]
    fn pow_one_y_is_one() {
        let (r, _) = Decimal32::ONE.pow(from_int(5, 0), RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), Decimal32::ONE.to_bits());

        let (r, _) = Decimal32::ONE.pow(Decimal32::NAN, RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), Decimal32::ONE.to_bits());
    }

    #[test]
    fn pow_non_canonical_one_cohort_short_circuits() {
        // Regression: §9.2 ties pow(1, y) = 1 to *value*, not cohort.
        // The earlier bit-pattern check missed `10 × 10⁻¹`, `100 ×
        // 10⁻²`, etc. — non-canonical cohorts of the value 1.
        for (coef, exp) in [(10i32, -1), (100, -2), (1_000_000, -6)] {
            let one_cohort = Decimal32::try_new(coef, exp).unwrap();
            // pow(this-cohort-of-1, 5) = 1
            let (r, s) = one_cohort.pow(from_int(5, 0), RoundingMode::NearestEven);
            assert_eq!(
                r.to_bits(),
                Decimal32::ONE.to_bits(),
                "pow({coef}E{exp}, 5)"
            );
            assert!(s.is_ok());
            // pow(this-cohort-of-1, qNaN) = 1
            let (r, s) = one_cohort.pow(Decimal32::NAN, RoundingMode::NearestEven);
            assert_eq!(
                r.to_bits(),
                Decimal32::ONE.to_bits(),
                "pow({coef}E{exp}, NaN)"
            );
            assert!(s.is_ok());
            // pow(this-cohort-of-1, sNaN) = 1 + INVALID per §9.2
            let (r, s) = one_cohort.pow(Decimal32::SIGNALING_NAN, RoundingMode::NearestEven);
            assert_eq!(
                r.to_bits(),
                Decimal32::ONE.to_bits(),
                "pow({coef}E{exp}, sNaN)"
            );
            assert!(s.invalid());
        }
    }

    #[test]
    fn pow_negative_base_non_integer_invalid() {
        // (-2)^0.5 = NaN + INVALID
        let half = Decimal32::parse_str("0.5", RoundingMode::NearestEven)
            .unwrap()
            .0;
        let (r, s) = from_int(-2, 0).pow(half, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());
    }

    #[test]
    fn pow_zero_negative_div_by_zero() {
        // 0^-1 = +∞ + DIV_BY_ZERO
        let (r, s) = Decimal32::ZERO.pow(from_int(-1, 0), RoundingMode::NearestEven);
        assert!(r.is_infinite());
        assert!(s.div_by_zero());
    }

    #[test]
    fn pow_overflow() {
        // 10^100 overflows.
        let (r, s) = Decimal32::TEN.pow(from_int(100, 0), RoundingMode::NearestEven);
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
        let (r, _) = Decimal32::ZERO.cbrt(RoundingMode::NearestEven);
        assert!(r.is_zero() && !r.is_sign_negative());

        let (r, _) = Decimal32::NEG_ZERO.cbrt(RoundingMode::NearestEven);
        assert!(r.is_zero() && r.is_sign_negative());

        let (r, _) = Decimal32::INFINITY.cbrt(RoundingMode::NearestEven);
        assert!(r.is_infinite() && !r.is_sign_negative());

        let (r, _) = Decimal32::NEG_INFINITY.cbrt(RoundingMode::NearestEven);
        assert!(r.is_infinite() && r.is_sign_negative());

        let (r, s) = Decimal32::NAN.cbrt(RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.is_ok());
    }
}
