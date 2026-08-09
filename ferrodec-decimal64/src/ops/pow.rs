//! IEEE 754-2019 §9.2 `pow` and `cbrt` for [`Decimal64`].
//!
//! `pow(x, y)` follows IEEE 754-2019 §9.2 and the ISO C `pow` rules.
//! `cbrt(x)` is the real cube root, defined for all real x including
//! negatives.

use crate::bid::{classify_bits, Class, BIAS, PRECISION};
use crate::decimal::Decimal64;
use ferrodec_ieee::{RoundingMode, Status};

/// `true` iff `d` numerically equals `+1`, regardless of cohort.
///
/// Matches every Form A / Form B encoding of `1 × 10⁰`, `10 × 10⁻¹`,
/// `100 × 10⁻²`, ..., up to the largest power-of-10 coefficient that
/// fits in 16 digits (`10¹⁵ × 10⁻¹⁵`).
fn equals_one(d: Decimal64) -> bool {
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
        // Coefficient must equal 10^k. The largest power-of-ten
        // cohort of the value 1 that fits in `PRECISION` digits is
        // `10^(PRECISION-1) × 10^-(PRECISION-1)`, so `k` cannot
        // exceed `PRECISION - 1` (L17: derive from `PRECISION`, not
        // a hardcoded 15).
        if k > PRECISION - 1 {
            return false;
        }
        return coefficient == 10u64.pow(k);
    }
    false
}

impl Decimal64 {
    /// IEEE 754-2019 §9.2 `pow(self, exponent)` rounded by `rm`.
    ///
    /// The finite non-special path routes through the shared faithful
    /// `ferrodec-transcend` Extended-precision kernel: `pow(x, y)` is
    /// evaluated as `exp(y · ln(|x|))` entirely at `Extended` working
    /// precision (with the bit-exact integer-exponent fast path the
    /// Decimal128 parent proves), rounded once at the format boundary,
    /// giving faithfully-rounded (≤ 1 ULP at 16 digits) results
    /// without the pre-fd-r0l lossy `f64` / `libm::pow` detour. The
    /// kernel is the same verified implementation the `ferrodec`
    /// (Decimal128) parent uses, instantiated at `F = Decimal64` via
    /// the `DecimalFormat` seam; it decides the negative-base /
    /// non-integer-exponent INVALID at `Extended` precision rather
    /// than on a rounded `f64` exponent, so the dead f64 domain
    /// plumbing is removed.
    ///
    /// The `pow_special_cases` short-circuit is kept ahead of the
    /// kernel call so Decimal64's special-value semantics (and the
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
        ferrodec_transcend::pow::pow_kernel::<Decimal64>(self, exponent, rm)
    }

    /// IEEE 754-2019 §9.2 `pown(self, n)`: `self` raised to the
    /// integer power `n`.
    ///
    /// A negative base is legal for every `n` (the exponent is an
    /// integer by type, so `pow`'s negative-base `INVALID` has no
    /// analog); the result is negative exactly when `self` is negative
    /// and `n` is odd. Pure delegation onto the shared kernel, which
    /// resolves every §9.2.1 special value internally and runs the
    /// ADR-0059 escalation ladder from this operation's first release.
    /// The special-value table, the two-arm kernel (working-precision
    /// powering for `|n| ≤ 6`, `exp(n·ln|self|)` beyond), the
    /// exactness and tie classification, and the ADR-0060 operand
    /// ranges over which the correct-rounding claim is unconditional
    /// all live on `ferrodec_transcend::pow::powi_kernel`.
    ///
    /// Preferred exponent (§9.2.2): `Q(pown(x, n))` is
    /// `floor(n × Q(x))` where the result is exact.
    #[must_use]
    #[doc(alias = "pown")]
    pub fn powi(self, n: i32, rm: RoundingMode) -> (Self, Status) {
        ferrodec_transcend::pow::powi_kernel::<Decimal64>(self, n, rm)
    }

    /// IEEE 754-2019 §9.2 `cbrt(self)` rounded by `rm`. Defined for
    /// all real x including negatives. `cbrt(±0) = ±0`,
    /// `cbrt(±∞) = ±∞`, NaN propagates.
    ///
    /// The finite non-zero path routes through the shared faithful
    /// `ferrodec-transcend` Extended-precision kernel (≤ 1 ULP at 16
    /// digits), replacing the pre-fd-r0l lossy `f64` / `libm::cbrt`
    /// detour. The `cbrt_special_cases` short-circuit is kept ahead of
    /// the kernel call so Decimal64's special-value semantics (and the
    /// ADR-0016 Kani shim, which shares `cbrt_special_cases`) are
    /// byte-identical to before; only the finite non-zero result path
    /// changes.
    #[must_use]
    pub fn cbrt(self, rm: RoundingMode) -> (Self, Status) {
        if let Some(special) = cbrt_special_cases(classify_bits(self.0)) {
            return special;
        }
        // Finite non-zero: faithful shared kernel.
        ferrodec_transcend::cbrt::cbrt_kernel::<Decimal64>(self, rm)
    }

    /// Kani-only entry for the binary `pow` special-case branch
    /// without invoking the `ferrodec-transcend` Extended-precision
    /// kernel (which resolves the negative-base / non-integer INVALID).
    /// CBMC cannot tractably encode the bignum kernel path. ADR-0016.
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
}

/// Resolve every `pow` input combination that does not need the
/// `ferrodec-transcend` Extended-precision kernel. Returns `None` for
/// the fall-through (a base / exponent pair the kernel evaluates,
/// including the negative-base non-integer INVALID, which the kernel
/// decides at Extended precision rather than on a rounded f64 exponent).
/// The resolution order is fixed and mirrors IEEE 754-2019 §9.2:
/// `pow(x, 0) = 1` (even `pow(NaN, 0)`); then `pow(1, y) = 1` by
/// value not cohort (`sNaN` exponent still raises INVALID); then
/// `sNaN` propagation over `[base, exponent]`; then `qNaN`
/// propagation. Shared by production `pow` and the Kani shim so the
/// two cannot drift.
fn pow_special_cases(base: Decimal64, exponent: Decimal64) -> Option<(Decimal64, Status)> {
    // pow(x, 0) = 1, including pow(NaN, 0) = 1.
    if exponent.is_zero() {
        return Some((Decimal64::ONE, Status::OK));
    }
    // pow(1, y) = 1 (even for y = NaN, including signaling NaN).
    // §9.2 ties this to *value*, not cohort, so we must catch every
    // cohort of 1 (`1×10⁰`, `10×10⁻¹`, `100×10⁻²`, ...), not just the
    // canonical bit pattern.
    if equals_one(base) {
        // sNaN exponent still raises INVALID per the §9.2 rule.
        if let Class::SignalingNaN { .. } = classify_bits(exponent.0) {
            return Some((Decimal64::ONE, Status::INVALID));
        }
        return Some((Decimal64::ONE, Status::OK));
    }
    // sNaN propagation.
    for arg in [base, exponent] {
        if let Class::SignalingNaN { sign, payload } = classify_bits(arg.0) {
            return Some((
                Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::INVALID,
            ));
        }
    }
    // qNaN propagation (a preferred per §6.2.3).
    for arg in [base, exponent] {
        if let Class::QuietNaN { sign, payload } = classify_bits(arg.0) {
            return Some((
                Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::OK,
            ));
        }
    }
    None
}

/// Resolve every `cbrt` input class the `ferrodec-transcend`
/// Extended-precision kernel does not need to see. `None` only for
/// finite non-zero. `cbrt(±∞) = ±∞`, `cbrt(±0) = ±0` (sign preserved); the
/// real cube root has no domain restriction. Shared by production
/// `cbrt` and the Kani shim so the two cannot drift.
fn cbrt_special_cases(class: Class) -> Option<(Decimal64, Status)> {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn from_int(n: i64, exp: i32) -> Decimal64 {
        Decimal64::try_new(n, exp).unwrap()
    }

    fn approx_equal(a: Decimal64, b: Decimal64) -> bool {
        let af = a.to_f64(RoundingMode::NearestEven).0;
        let bf = b.to_f64(RoundingMode::NearestEven).0;
        let tol = 1e-13;
        (af - bf).abs() <= tol * (1.0 + bf.abs())
    }

    #[test]
    fn pow_basic() {
        // 2^3 = 8
        let (r, _) = from_int(2, 0).pow(from_int(3, 0), RoundingMode::NearestEven);
        assert!(approx_equal(r, from_int(8, 0)));

        // 10^2 = 100
        let (r, _) = Decimal64::TEN.pow(from_int(2, 0), RoundingMode::NearestEven);
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
    fn pow_non_canonical_one_cohort_short_circuits() {
        // Regression: §9.2 ties pow(1, y) = 1 to *value*, not cohort.
        // The earlier bit-pattern check missed `10 × 10⁻¹`, `100 ×
        // 10⁻²`, etc. — non-canonical cohorts of the value 1.
        for (coef, exp) in [
            (10i64, -1),
            (100, -2),
            (10_000_000, -7),
            (1_000_000_000_000_000, -15),
        ] {
            let one_cohort = Decimal64::try_new(coef, exp).unwrap();
            // pow(this-cohort-of-1, 5) = 1
            let (r, s) = one_cohort.pow(from_int(5, 0), RoundingMode::NearestEven);
            assert_eq!(
                r.to_bits(),
                Decimal64::ONE.to_bits(),
                "pow({coef}E{exp}, 5)"
            );
            assert!(s.is_ok());
            // pow(this-cohort-of-1, qNaN) = 1
            let (r, s) = one_cohort.pow(Decimal64::NAN, RoundingMode::NearestEven);
            assert_eq!(
                r.to_bits(),
                Decimal64::ONE.to_bits(),
                "pow({coef}E{exp}, NaN)"
            );
            assert!(s.is_ok());
            // pow(this-cohort-of-1, sNaN) = 1 + INVALID per §9.2
            let (r, s) = one_cohort.pow(Decimal64::SIGNALING_NAN, RoundingMode::NearestEven);
            assert_eq!(
                r.to_bits(),
                Decimal64::ONE.to_bits(),
                "pow({coef}E{exp}, sNaN)"
            );
            assert!(s.invalid());
        }
    }

    #[test]
    fn pow_negative_base_non_integer_invalid() {
        // (-2)^0.5 = NaN + INVALID
        let half = Decimal64::parse_str("0.5", RoundingMode::NearestEven)
            .unwrap()
            .0;
        let (r, s) = from_int(-2, 0).pow(half, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());
    }

    #[test]
    fn pow_zero_negative_div_by_zero() {
        // 0^-1 = +∞ + DIV_BY_ZERO
        let (r, s) = Decimal64::ZERO.pow(from_int(-1, 0), RoundingMode::NearestEven);
        assert!(r.is_infinite());
        assert!(s.div_by_zero());
    }

    #[test]
    fn pow_overflow() {
        // Decimal64's E_MAX is 384; 10^400 exceeds Decimal64's range,
        // so the Extended-precision kernel overflows the format and pow
        // propagates OVERFLOW.
        let (r, s) = Decimal64::TEN.pow(from_int(400, 0), RoundingMode::NearestEven);
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

    fn parse(s: &str) -> Decimal64 {
        Decimal64::parse_str(s, RoundingMode::NearestEven)
            .unwrap()
            .0
    }

    fn value_eq(a: Decimal64, b: Decimal64) -> bool {
        matches!(a.partial_cmp(b).0, Some(core::cmp::Ordering::Equal))
    }

    #[test]
    fn cbrt_perfect_cubes_are_exact() {
        // IEEE 754-2019 §7.5: a perfect cube root is exact, no INEXACT (fd-92w.8).
        let cases = [
            ("8", "2"),
            ("27", "3"),
            ("-27", "-3"),
            ("1", "1"),
            ("1000", "10"),
            ("0.001", "0.1"),
            ("1000000", "100"),
        ];
        for (input, want) in cases {
            let (r, s) = parse(input).cbrt(RoundingMode::NearestEven);
            assert!(
                value_eq(r, parse(want)),
                "cbrt({input}) = {r:?}, want {want}"
            );
            assert!(!s.inexact(), "cbrt({input}) must not raise INEXACT");
        }
    }

    #[test]
    fn cbrt_non_cubes_are_inexact() {
        for input in ["2", "9", "7"] {
            let (_, s) = parse(input).cbrt(RoundingMode::NearestEven);
            assert!(s.inexact(), "cbrt({input}) must raise INEXACT");
        }
    }

    #[test]
    fn pow_exact_results_are_not_inexact() {
        // Rational and integer exact powers must not raise INEXACT.
        // Decimal64 E_MAX is 384, so 10^300 is in range.
        let cases = [
            ("4", "0.5", "2"),
            ("9", "0.5", "3"),
            ("100", "0.5", "10"),
            ("4", "-0.5", "0.5"),
            ("2", "3", "8"),
            ("10", "300", "1E+300"),
        ];
        for (base, exp, want) in cases {
            let (r, s) = parse(base).pow(parse(exp), RoundingMode::NearestEven);
            assert!(
                value_eq(r, parse(want)),
                "pow({base}, {exp}) = {r:?}, want {want}"
            );
            assert!(!s.inexact(), "pow({base}, {exp}) must not raise INEXACT");
        }
    }

    #[test]
    fn pow_irrational_results_are_inexact() {
        for (base, exp) in [("2", "0.5"), ("3", "0.5"), ("2", "0.1"), ("7", "2.5")] {
            let (_, s) = parse(base).pow(parse(exp), RoundingMode::NearestEven);
            assert!(s.inexact(), "pow({base}, {exp}) must raise INEXACT");
        }
    }
}
