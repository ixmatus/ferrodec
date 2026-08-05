//! IEEE 754-2019 §9.2 exponential functions for [`Decimal64`].
//!
//! `exp` and `ln` route their finite-non-zero path through the shared
//! faithful `ferrodec-transcend` Extended-precision kernel. The base
//! variants `exp2` / `log2` / `log10`, the shifted logarithm `ln_1p`
//! (IEEE 754-2019 §9.2 `logp1`), and the shifted exponential `exp_m1`
//! (§9.2 `expm1`) are pure delegations onto the same faithful kernel
//! (it resolves every special case internally), at exact parity with
//! the `ferrodec` (Decimal128) parent.
//!
//! The shared kernel runs at 50-digit
//! `Extended` working precision, rounded once at the format boundary,
//! giving faithfully-rounded (≤ 1 ULP at 16 digits) results without
//! the pre-fd-r0l lossy `f64` / `libm` detour. The kernel is the same
//! verified implementation the `ferrodec` (Decimal128) parent uses,
//! instantiated at `F = Decimal64` via the `DecimalFormat` seam.
//!
//! The special-value short-circuits (`exp_special_cases` /
//! `ln_special_cases`) stay in this module ahead of the kernel call:
//! they are shared with the ADR-0016 Kani shims (which must never
//! reach the Extended kernel) and keep Decimal64's special-value
//! semantics byte-identical across the rewire.
//!
//! # Special cases (IEEE 754-2019 §9.2)
//!
//! * NaN propagates (sNaN raises INVALID).
//! * `exp(±∞)`: `+∞ → +∞`, `−∞ → +0`.
//! * `exp(±0) = 1`.
//! * Out of range: Decimal64's exponent range supports `exp(x)` up to
//!   `x ≈ +886.49` (since `e^886.49 ≈ 10^385 = MAX`) and underflow to
//!   subnormals down to `x ≈ −916.98`. The faithful kernel's
//!   magnitude gate short-circuits to `+∞ + OVERFLOW` for `x > +887`
//!   and to `+0 + UNDERFLOW + INEXACT` for `x < −918`; inputs in
//!   `(−918, −887]` produce representable subnormals (the Taylor
//!   pipeline handles them). The thresholds are derived in
//!   `DecimalFormat for Decimal64` (`transcend_impl.rs`).
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
//!
//! # Special cases for `log2_1p` (§9.2.1 `log2p1`)
//!
//! * `log2_1p(NaN)` propagates; a signaling NaN raises INVALID.
//! * `log2_1p(±0) = ±0`, sign preserved, no exception.
//! * `log2_1p(-1) = −∞ + DIV_BY_ZERO`.
//! * `log2_1p(x)` for `x < −1`, `−∞` included, is NaN + INVALID.
//! * `log2_1p(+∞) = +∞`.
//! * A tiny `x` can land the result in the subnormal range, raising
//!   UNDERFLOW + INEXACT.
//!
//! # Special cases for `exp_m1` (§9.2 `expm1`, §9.2.1)
//!
//! * `expm1(NaN)` propagates (sNaN raises INVALID).
//! * `expm1(±0) = ±0`, sign preserved, no exception.
//! * `expm1(−∞) = −1` exactly, with no exception.
//! * `expm1(+∞) = +∞`.
//! * An argument past the overflow threshold above delivers the §7.4
//!   disposition for the rounding direction with OVERFLOW + INEXACT;
//!   subtracting 1 cannot pull a value at that scale back into range.
//! * A subnormal result raises UNDERFLOW alongside INEXACT, which a
//!   tiny argument reaches because the result hugs the argument.
//!
//! # Special cases for `exp2_m1` (§9.2.1 `exp2m1`)
//!
//! * `exp2_m1(NaN)` propagates; a signaling NaN raises INVALID.
//! * `exp2_m1(±0) = ±0`, sign preserved, no exception.
//! * `exp2_m1(−∞) = −1` exactly, no exception.
//! * `exp2_m1(+∞) = +∞`.
//! * A large positive argument overflows per §7.4 (`+∞` at the
//!   nearest modes and toward `+∞`, the largest finite magnitude
//!   toward zero and `−∞`) with OVERFLOW + INEXACT.
//! * A tiny `x` can land the result in the subnormal range, raising
//!   UNDERFLOW + INEXACT.
//!
//! # Special cases for `exp10_m1` (§9.2 `exp10m1`, §9.2.1)
//!
//! * `exp10m1(NaN)` propagates (sNaN raises INVALID).
//! * `exp10m1(±0) = ±0`, sign preserved, no exception.
//! * `exp10m1(−∞) = −1` exactly, no exception.
//! * `exp10m1(+∞) = +∞`.
//! * A large positive argument overflows to the §7.4 disposition for
//!   the rounding direction with OVERFLOW + INEXACT; a tiny argument
//!   can land the result in the subnormal range, raising UNDERFLOW +
//!   INEXACT.
//!
//!
//! # Special cases for `exp10` (§9.2 `exp10`, §9.2.1)
//!
//! * `exp10(NaN)` propagates (sNaN raises INVALID).
//! * `exp10(±0) = 1` exactly, no exception.
//! * `exp10(−∞) = +0`; `exp10(+∞) = +∞`.
//! * `exp10(n) = 10^n` exactly for every integer `n` in
//!   `[−398, 384]`, with no INEXACT (§7.5); integers past that range
//!   deliver the §7.4 overflow or underflow disposition with
//!   OVERFLOW + INEXACT or UNDERFLOW + INEXACT.
//!

use crate::bid::{classify_bits, Class};
use crate::decimal::Decimal64;
use ferrodec_ieee::{RoundingMode, Status};

impl Decimal64 {
    /// IEEE 754-2019 §9.2 `exp(self)` rounded by `rm`.
    ///
    /// Finite non-zero inputs route through the shared faithful
    /// `ferrodec-transcend` Extended-precision kernel (≤ 1 ULP across
    /// the true Decimal64 domain), replacing the pre-fd-r0l lossy
    /// `f64` / `libm::exp` detour. The `exp_special_cases`
    /// short-circuit is kept ahead of the kernel call so Decimal64's
    /// special-value semantics (and the ADR-0016 Kani shim, which
    /// shares `exp_special_cases`) are byte-identical to before; only
    /// the finite-non-zero result path changes.
    #[must_use]
    pub fn exp(self, rm: RoundingMode) -> (Self, Status) {
        if let Some(special) = exp_special_cases(classify_bits(self.0)) {
            return special;
        }
        // Finite non-zero: faithful shared kernel.
        ferrodec_transcend::exp::exp_kernel::<Decimal64>(self, rm)
    }

    /// IEEE 754-2019 §9.2 `ln(self)` rounded by `rm`.
    ///
    /// Finite positive inputs route through the shared faithful
    /// `ferrodec-transcend` Extended-precision kernel (≤ 1 ULP across
    /// the true Decimal64 domain), replacing the pre-fd-r0l lossy
    /// `f64` / `libm::log` detour. The `ln_special_cases`
    /// short-circuit is kept ahead of the kernel call so Decimal64's
    /// special-value semantics (and the ADR-0016 Kani shim, which
    /// shares `ln_special_cases`) are byte-identical to before; only
    /// the finite-positive result path changes.
    #[must_use]
    pub fn ln(self, rm: RoundingMode) -> (Self, Status) {
        if let Some(special) = ln_special_cases(classify_bits(self.0)) {
            return special;
        }
        // Positive finite non-zero: faithful shared kernel.
        ferrodec_transcend::ln::ln_kernel::<Decimal64>(self, rm)
    }

    /// Base-2 exponential `2^self`. Computed as
    /// `exp(self · ln(2))` at extended precision.
    #[must_use]
    pub fn exp2(self, rm: RoundingMode) -> (Self, Status) {
        ferrodec_transcend::exp::exp2_kernel::<Decimal64>(self, rm)
    }

    /// Base-10 logarithm `log10(self)`. Computed as
    /// `ln_extended(self) · (1/ln(10))_extended`, then rounded once.
    #[must_use]
    pub fn log10(self, rm: RoundingMode) -> (Self, Status) {
        ferrodec_transcend::ln::log10_kernel::<Decimal64>(self, rm)
    }

    /// Base-2 logarithm `log2(self)`. Computed as
    /// `ln_extended(self) · (1/ln(2))_extended`, then rounded once.
    #[must_use]
    pub fn log2(self, rm: RoundingMode) -> (Self, Status) {
        ferrodec_transcend::ln::log2_kernel::<Decimal64>(self, rm)
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
        ferrodec_transcend::ln::logp1_kernel::<Decimal64>(self, rm)
    }

    /// IEEE 754-2019 §9.2 `expm1(self)`: `e^self − 1`, evaluated so an
    /// argument near zero keeps its full relative accuracy. Pure
    /// delegation onto the shared kernel, which resolves every §9.2.1
    /// special value internally (this module's header lists them) and
    /// runs the ADR-0059 escalation ladder from this operation's first
    /// release; the derivation of its exactness classification, its
    /// two ADR-0051 anchor seams, and its error budget live on
    /// `ferrodec_transcend::exp::expm1_kernel` and `ladder::EXPM1`.
    #[must_use]
    #[doc(alias = "expm1")]
    pub fn exp_m1(self, rm: RoundingMode) -> (Self, Status) {
        ferrodec_transcend::exp::expm1_kernel::<Decimal64>(self, rm)
    }

    pub fn exp2_m1(self, rm: RoundingMode) -> (Self, Status) {
        ferrodec_transcend::exp::exp2m1_kernel::<Decimal64>(self, rm)
    }

    pub fn exp10(self, rm: RoundingMode) -> (Self, Status) {
        ferrodec_transcend::exp::exp10_kernel::<Decimal64>(self, rm)
    }

    pub fn exp10_m1(self, rm: RoundingMode) -> (Self, Status) {
        ferrodec_transcend::exp::exp10m1_kernel::<Decimal64>(self, rm)
    }

    pub fn log2_1p(self, rm: RoundingMode) -> (Self, Status) {
        ferrodec_transcend::ln::log2p1_kernel::<Decimal64>(self, rm)
    }

    pub fn log10_1p(self, rm: RoundingMode) -> (Self, Status) {
        ferrodec_transcend::ln::log10p1_kernel::<Decimal64>(self, rm)
    }

    /// Kani-only entry returning the `exp` special-case branch
    /// without invoking the `ferrodec-transcend` Extended-precision
    /// kernel. CBMC cannot tractably encode the bignum kernel path.
    /// ADR-0016.
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
fn exp_special_cases(class: Class) -> Option<(Decimal64, Status)> {
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
        Class::Infinity { sign: true } => Some((Decimal64::ZERO, Status::OK)),
        Class::Zero { .. } => Some((Decimal64::ONE, Status::OK)),
        Class::Finite { .. } => None,
    }
}

/// Resolve every `ln` input class the `ferrodec-transcend`
/// Extended-precision kernel does not need to see. Returns `None` only
/// for positive finite non-zero. Shared by production `ln` and the Kani
/// shim so the two cannot drift.
fn ln_special_cases(class: Class) -> Option<(Decimal64, Status)> {
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
        Class::Zero { .. } => Some((Decimal64::NEG_INFINITY, Status::DIV_BY_ZERO)),
        Class::Finite { sign: true, .. } => Some((Decimal64::NAN, Status::INVALID)),
        Class::Finite { sign: false, .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bid::{pack_finite, BiasedExp, Coefficient, BIAS};

    fn from_int(n: i64, exp: i32) -> Decimal64 {
        Decimal64::try_new(n, exp).unwrap()
    }

    fn approx_equal(a: Decimal64, b: Decimal64, max_ulp: u32) -> bool {
        // Convert both to f64 and check relative tolerance proportional
        // to max_ulp. Decimal64 carries 16 digits but the f64 round-trip
        // in this comparison caps achievable precision at ~10⁻¹⁵ relative; we
        // pick 1e-14 to absorb the worst-case double-rounding noise.
        let af = a.to_f64(RoundingMode::NearestEven).0;
        let bf = b.to_f64(RoundingMode::NearestEven).0;
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
        let expected = Decimal64::from_bits(pack_finite(
            false,
            BiasedExp::try_from_biased(BIAS - 15).unwrap(),
            Coefficient::try_new(2_718_281_828_459_045).unwrap(),
        ));
        assert!(approx_equal(r, expected, 1));
    }

    #[test]
    fn exp_negative_one_is_reciprocal_e() {
        let (r, _) = Decimal64::NEG_ONE.exp(RoundingMode::NearestEven);
        // 1/e ≈ 0.3678794411714423
        let expected = Decimal64::from_bits(pack_finite(
            false,
            BiasedExp::try_from_biased(BIAS - 16).unwrap(),
            Coefficient::try_new(3_678_794_411_714_423).unwrap(),
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
    fn exp_underflow_contract_m7() {
        // Finding T1 (work-order alias M7) claimed exp produces a
        // Decimal64-subnormal result that misses UNDERFLOW. Under the
        // shared Extended-precision kernel the subnormal window is
        // genuine: inputs in roughly `(−918, −887]` produce
        // representable Decimal64 subnormals (see this module's header),
        // unlike the removed f64 path which saturated to zero before
        // reaching that window. So this test pins the whole underflow
        // ladder and guards both directions: a normal mid-range result
        // must NOT raise a spurious UNDERFLOW, and a subnormal result
        // MUST raise UNDERFLOW + INEXACT (IEEE 754-2019 §7.5) — the
        // exact defect T1 named. Verified against the kernel (fd-aqs.15).
        //
        // exp(-720) ≈ 2E-313: a normal Decimal64, inexact, NOT a
        // spurious underflow.
        let (r, s) = from_int(-720, 0).exp(RoundingMode::NearestEven);
        assert!(r.is_finite() && !r.is_zero());
        assert!(!r.is_subnormal(), "exp(-720) is normal, not subnormal");
        assert!(s.inexact());
        assert!(
            !s.underflow(),
            "exp(-720) must not raise a spurious UNDERFLOW"
        );

        // exp(-900) ≈ 1.4E-391: a representable Decimal64 *subnormal*.
        // A subnormal inexact result is a genuine underflow, so it must
        // carry UNDERFLOW + INEXACT. This is the case T1 flagged, now
        // reachable through the kernel (the removed f64 path never
        // produced it).
        let (r, s) = from_int(-900, 0).exp(RoundingMode::NearestEven);
        assert!(r.is_subnormal(), "exp(-900) is a Decimal64 subnormal");
        assert!(
            s.underflow() && s.inexact(),
            "exp(-900) subnormal must raise UNDERFLOW + INEXACT"
        );

        // exp(-2000): far below the subnormal floor, rounds to zero
        // with UNDERFLOW + INEXACT.
        let (r, s) = from_int(-2000, 0).exp(RoundingMode::NearestEven);
        assert!(r.is_zero());
        assert!(s.underflow() && s.inexact());
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
        let e_approx = Decimal64::from_bits(pack_finite(
            false,
            BiasedExp::try_from_biased(BIAS - 15).unwrap(),
            Coefficient::try_new(2_718_281_828_459_045).unwrap(),
        ));
        let (r, _) = e_approx.ln(RoundingMode::NearestEven);
        assert!(approx_equal(r, Decimal64::ONE, 10));
    }

    #[test]
    fn ln_ten_is_ln10() {
        let (r, _) = Decimal64::TEN.ln(RoundingMode::NearestEven);
        // ln(10) ≈ 2.302585092994046 at 16 digits.
        let expected = Decimal64::from_bits(pack_finite(
            false,
            BiasedExp::try_from_biased(BIAS - 15).unwrap(),
            Coefficient::try_new(2_302_585_092_994_046).unwrap(),
        ));
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
