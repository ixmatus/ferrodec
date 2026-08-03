//! IEEE 754-2019 §9.2 exponential functions for [`Decimal32`].
//!
//! `exp` and `ln` route their finite-non-zero path through the shared
//! faithful `ferrodec-transcend` Extended-precision kernel. The base
//! variants `exp2` / `log2` / `log10` and the shifted logarithm
//! `ln_1p` (IEEE 754-2019 §9.2 `logp1`) are pure delegations onto the
//! same faithful kernel (it resolves every special case internally),
//! at exact parity with the `ferrodec` (Decimal128) parent and the
//! `ferrodec-decimal64` sibling.
//!
//! The shared kernel runs at 50-digit
//! `Extended` working precision, rounded once at the format boundary,
//! giving faithfully-rounded (≤ 1 ULP at 7 digits) results without
//! the pre-fd-r0l lossy `f64` / `libm` detour. The kernel is the same
//! verified implementation the `ferrodec` (Decimal128) parent and the
//! `ferrodec-decimal64` sibling use, instantiated at `F = Decimal32`
//! via the `DecimalFormat` seam.
//!
//! The special-value short-circuits (`exp_special_cases` /
//! `ln_special_cases`) stay in this module ahead of the kernel call:
//! they are shared with the ADR-0016 Kani shims (which must never
//! reach the Extended kernel) and keep Decimal32's special-value
//! semantics byte-identical across the rewire.
//!
//! # Special cases (IEEE 754-2019 §9.2)
//!
//! * NaN propagates (sNaN raises INVALID).
//! * `exp(±∞)`: `+∞ → +∞`, `−∞ → +0`.
//! * `exp(±0) = 1`.
//! * Out of range: Decimal32's exponent range supports `exp(x)` up to
//!   `x ≈ +223.35` (since `e^223.35 ≈ 10^97 = MAX`) and underflow to
//!   subnormals down to `x ≈ −233.25`. The faithful kernel's
//!   magnitude gate short-circuits to `+∞ + OVERFLOW` for `x > +224`
//!   and to `+0 + UNDERFLOW + INEXACT` for `x < −235`; inputs in
//!   `(−235, −224]` produce representable subnormals (the Taylor
//!   pipeline handles them). The thresholds are derived in
//!   `DecimalFormat for Decimal32` (`transcend_impl.rs`).
//!
//! # Special cases for `ln`
//!
//! * `ln(NaN)` propagates.
//! * `ln(±0) = −∞ + DIV_BY_ZERO`.
//! * `ln(negative)` → NaN + INVALID.
//! * `ln(+∞) = +∞`.
//! * `ln(1) = +0`.
//!
//! # Special cases for `ln_1p` (§9.2 `logp1`, §9.2.1)
//!
//! * `logp1(NaN)` propagates (sNaN raises INVALID).
//! * `logp1(±0) = ±0`, sign preserved, no exception.
//! * `logp1(−1) = −∞ + DIV_BY_ZERO`.
//! * `logp1(x) = NaN + INVALID` for every `x < −1`, `−∞` included.
//! * `logp1(+∞) = +∞`.
//! * A subnormal result raises UNDERFLOW alongside INEXACT, which a
//!   tiny argument reaches because the result hugs the argument.
//!
//! # Special cases for `log10_1p` (§9.2.1)
//!
//! * `log10_1p(NaN)` propagates (sNaN raises INVALID).
//! * `log10_1p(±0) = ±0`, sign preserved, no exception.
//! * `log10_1p(−1) = −∞ + DIV_BY_ZERO`.
//! * `log10_1p(x)` → NaN + INVALID for `x < −1`, `−∞` included.
//! * `log10_1p(+∞) = +∞`.

use crate::bid::{classify_bits, Class};
use crate::decimal::Decimal32;
use ferrodec_ieee::{RoundingMode, Status};

impl Decimal32 {
    /// IEEE 754-2019 §9.2 `exp(self)` rounded by `rm`.
    ///
    /// Finite non-zero inputs route through the shared faithful
    /// `ferrodec-transcend` Extended-precision kernel (≤ 1 ULP across
    /// the true Decimal32 domain), replacing the pre-fd-r0l lossy
    /// `f64` / `libm::exp` detour. The `exp_special_cases`
    /// short-circuit is kept ahead of the kernel call so Decimal32's
    /// special-value semantics (and the ADR-0016 Kani shim, which
    /// shares `exp_special_cases`) are byte-identical to before; only
    /// the finite-non-zero result path changes.
    #[must_use]
    pub fn exp(self, rm: RoundingMode) -> (Self, Status) {
        if let Some(special) = exp_special_cases(classify_bits(self.0)) {
            return special;
        }
        // Finite non-zero: faithful shared kernel.
        ferrodec_transcend::exp::exp_kernel::<Decimal32>(self, rm)
    }

    /// IEEE 754-2019 §9.2 `ln(self)` rounded by `rm`.
    ///
    /// Finite positive inputs route through the shared faithful
    /// `ferrodec-transcend` Extended-precision kernel (≤ 1 ULP across
    /// the true Decimal32 domain), replacing the pre-fd-r0l lossy
    /// `f64` / `libm::log` detour. The `ln_special_cases`
    /// short-circuit is kept ahead of the kernel call so Decimal32's
    /// special-value semantics (and the ADR-0016 Kani shim, which
    /// shares `ln_special_cases`) are byte-identical to before; only
    /// the finite-positive result path changes.
    #[must_use]
    pub fn ln(self, rm: RoundingMode) -> (Self, Status) {
        if let Some(special) = ln_special_cases(classify_bits(self.0)) {
            return special;
        }
        // Positive finite non-zero: faithful shared kernel.
        ferrodec_transcend::ln::ln_kernel::<Decimal32>(self, rm)
    }

    /// Base-2 exponential `2^self`. Computed as
    /// `exp(self · ln(2))` at extended precision.
    #[must_use]
    pub fn exp2(self, rm: RoundingMode) -> (Self, Status) {
        ferrodec_transcend::exp::exp2_kernel::<Decimal32>(self, rm)
    }

    /// Base-10 logarithm `log10(self)`. Computed as
    /// `ln_extended(self) · (1/ln(10))_extended`, then rounded once.
    #[must_use]
    pub fn log10(self, rm: RoundingMode) -> (Self, Status) {
        ferrodec_transcend::ln::log10_kernel::<Decimal32>(self, rm)
    }

    /// Base-2 logarithm `log2(self)`. Computed as
    /// `ln_extended(self) · (1/ln(2))_extended`, then rounded once.
    #[must_use]
    pub fn log2(self, rm: RoundingMode) -> (Self, Status) {
        ferrodec_transcend::ln::log2_kernel::<Decimal32>(self, rm)
    }

    /// IEEE 754-2019 §9.2 `logp1(self)`: `ln(1 + self)`, evaluated so
    /// an argument near zero keeps its full relative accuracy. Pure
    /// delegation onto the shared kernel, which resolves every §9.2.1
    /// special value internally (this module's header lists them) and
    /// runs the ADR-0059 escalation ladder from this operation's first
    /// release; the derivation of its exactness classification and its
    /// error budget live on `ferrodec_transcend::ln::logp1_kernel` and
    /// `ladder::LOGP1`.
    #[must_use]
    #[doc(alias = "logp1")]
    pub fn ln_1p(self, rm: RoundingMode) -> (Self, Status) {
        ferrodec_transcend::ln::logp1_kernel::<Decimal32>(self, rm)
    }

    pub fn log10_1p(self, rm: RoundingMode) -> (Self, Status) {
        ferrodec_transcend::ln::log10p1_kernel::<Decimal32>(self, rm)
    }

    /// Kani-only entry returning the `exp` special-case branch without
    /// invoking the `ferrodec-transcend` Extended-precision kernel. CBMC
    /// cannot tractably encode the bignum kernel path. ADR-0016.
    #[cfg(kani)]
    #[doc(hidden)]
    #[must_use]
    pub fn exp_special_only_for_kani(self) -> Option<(Self, Status)> {
        exp_special_cases(classify_bits(self.0))
    }

    /// Kani-only entry returning the `ln` special-case branch without
    /// invoking the `ferrodec-transcend` Extended-precision kernel.
    /// ADR-0016.
    #[cfg(kani)]
    #[doc(hidden)]
    #[must_use]
    pub fn ln_special_only_for_kani(self) -> Option<(Self, Status)> {
        ln_special_cases(classify_bits(self.0))
    }
}

/// Resolve every `exp` input class the `ferrodec-transcend`
/// Extended-precision kernel does not need to see. Returns `None` only
/// for finite non-zero, the single class the kernel evaluates. Shared
/// by production `exp` and the Kani shim so the two cannot drift.
fn exp_special_cases(class: Class) -> Option<(Decimal32, Status)> {
    match class {
        Class::SignalingNaN { sign, payload } => Some((
            Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::INVALID,
        )),
        Class::QuietNaN { sign, payload } => Some((
            Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::OK,
        )),
        Class::Infinity { sign: false } => Some((Decimal32::INFINITY, Status::OK)),
        Class::Infinity { sign: true } => Some((Decimal32::ZERO, Status::OK)),
        Class::Zero { .. } => Some((Decimal32::ONE, Status::OK)),
        Class::Finite { .. } => None,
    }
}

/// Resolve every `ln` input class the `ferrodec-transcend`
/// Extended-precision kernel does not need to see. Returns `None` only
/// for positive finite non-zero. Shared by production `ln` and the Kani
/// shim so the two cannot drift.
fn ln_special_cases(class: Class) -> Option<(Decimal32, Status)> {
    match class {
        Class::SignalingNaN { sign, payload } => Some((
            Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::INVALID,
        )),
        Class::QuietNaN { sign, payload } => Some((
            Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::OK,
        )),
        Class::Infinity { sign: false } => Some((Decimal32::INFINITY, Status::OK)),
        Class::Infinity { sign: true } => Some((Decimal32::NAN, Status::INVALID)),
        Class::Zero { .. } => Some((Decimal32::NEG_INFINITY, Status::DIV_BY_ZERO)),
        Class::Finite { sign: true, .. } => Some((Decimal32::NAN, Status::INVALID)),
        Class::Finite { sign: false, .. } => None,
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
        let af = a.to_f64(RoundingMode::NearestEven).0;
        let bf = b.to_f64(RoundingMode::NearestEven).0;
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
    fn exp_underflow_ladder() {
        // The shared Extended-precision kernel produces representable
        // Decimal32 subnormals for inputs in roughly `(−233, −219]`
        // (the removed f64 path saturated to zero before reaching that
        // window). Pin the whole underflow ladder: a normal result must
        // NOT raise a spurious UNDERFLOW, a subnormal result MUST raise
        // UNDERFLOW + INEXACT (IEEE 754-2019 §7.5), and a result rounded
        // to zero MUST raise UNDERFLOW + INEXACT. Verified fd-aqs.15 (the
        // Decimal64 sibling carries the same guard as
        // `exp_underflow_contract_m7`).
        //
        // exp(-180) ≈ 6E-79: a normal Decimal32, not a spurious underflow.
        let (r, s) = from_int(-180, 0).exp(RoundingMode::NearestEven);
        assert!(
            r.is_finite() && !r.is_zero() && !r.is_subnormal(),
            "exp(-180) normal"
        );
        assert!(s.inexact());
        assert!(
            !s.underflow(),
            "exp(-180) must not raise a spurious UNDERFLOW"
        );

        // exp(-225) ≈ 2E-98: a representable Decimal32 subnormal → must
        // carry UNDERFLOW + INEXACT.
        let (r, s) = from_int(-225, 0).exp(RoundingMode::NearestEven);
        assert!(r.is_subnormal(), "exp(-225) is a Decimal32 subnormal");
        assert!(
            s.underflow() && s.inexact(),
            "exp(-225) subnormal must raise UNDERFLOW + INEXACT"
        );

        // exp(-500): far below the subnormal floor, rounds to zero with
        // UNDERFLOW + INEXACT.
        let (r, s) = from_int(-500, 0).exp(RoundingMode::NearestEven);
        assert!(r.is_zero());
        assert!(s.underflow() && s.inexact());
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
