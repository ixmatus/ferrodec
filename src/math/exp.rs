//! Re-export + delegating shim: the exp kernel moved to
//! ferrodec-transcend (P0a.2 c6). The public `Decimal128::exp` /
//! `exp2` wrappers and their behaviour tests stay here as the
//! byte-identical regression gate.

use crate::decimal::Decimal128;
use ferrodec_ieee::{RoundingMode, Status};

// The still-in-core pow.rs uses `crate::math::exp::exp_from_extended`
// and hyperbolic.rs uses `crate::math::exp::exp_extended`. These
// re-exports keep those imports resolving unchanged: `exp_from_extended`
// is generic with F inferred or turbofished by the caller, `exp_extended`
// is non-generic (pure Extended).
#[allow(unused_imports)]
pub(crate) use ferrodec_transcend::exp::{exp_extended, exp_from_extended};

impl Decimal128 {
    /// Natural exponential `e^self`, rounded according to `rm`.
    ///
    /// Domain: every finite input maps to a defined IEEE result —
    /// finite, `+0` (underflow), or `+∞` (overflow).
    #[must_use]
    pub fn exp(self, rm: RoundingMode) -> (Self, Status) {
        ferrodec_transcend::exp::exp_kernel::<Decimal128>(self, rm)
    }

    /// Base-2 exponential `2^self`. Computed as
    /// `exp(self · ln(2))` at extended precision.
    #[must_use]
    pub fn exp2(self, rm: RoundingMode) -> (Self, Status) {
        ferrodec_transcend::exp::exp2_kernel::<Decimal128>(self, rm)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Decimal128 {
        Decimal128::parse_str(s, RoundingMode::NearestEven)
            .unwrap()
            .0
    }

    fn within_ulps(got: Decimal128, want: Decimal128, ulps: u32) -> bool {
        let (diff, _) = got.sub(want, RoundingMode::NearestEven);
        let diff = diff.abs();
        let abs_want = want.abs();
        if abs_want.is_zero() {
            let bound = parse(&alloc::format!("{ulps}e-30"));
            let (cmp, _) = diff.partial_cmp(bound);
            return matches!(
                cmp,
                Some(core::cmp::Ordering::Less | core::cmp::Ordering::Equal)
            );
        }
        let (rel, _) = diff.div(abs_want, RoundingMode::NearestEven);
        let bound = parse(&alloc::format!("{ulps}e-33"));
        let (cmp, _) = rel.partial_cmp(bound);
        matches!(
            cmp,
            Some(core::cmp::Ordering::Less | core::cmp::Ordering::Equal)
        )
    }

    #[test]
    fn exp_zero_is_one() {
        let (r, _) = Decimal128::ZERO.exp(RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), Decimal128::ONE.to_bits());
    }

    #[test]
    fn exp_one_is_e() {
        let (r, _) = Decimal128::ONE.exp(RoundingMode::NearestEven);
        let target = parse("2.718281828459045235360287471352662");
        assert!(
            within_ulps(r, target, 1),
            "exp(1) = {r:?}, want ≈ {target:?}"
        );
    }

    #[test]
    fn exp_neg_one() {
        let (r, _) = Decimal128::NEG_ONE.exp(RoundingMode::NearestEven);
        let target = parse("0.3678794411714423215955237701614608");
        assert!(within_ulps(r, target, 1));
    }

    #[test]
    fn exp_two() {
        let two = parse("2");
        let (r, _) = two.exp(RoundingMode::NearestEven);
        let target = parse("7.389056098930650227230427460575008");
        assert!(within_ulps(r, target, 1));
    }

    #[test]
    fn exp_nan_propagates() {
        let (r, _) = Decimal128::NAN.exp(RoundingMode::NearestEven);
        assert!(r.is_nan());

        let (r, s) = Decimal128::SIGNALING_NAN.exp(RoundingMode::NearestEven);
        assert!(r.is_nan());
        assert!(s.invalid());
    }

    #[test]
    fn exp_pos_inf_is_pos_inf() {
        let (r, _) = Decimal128::INFINITY.exp(RoundingMode::NearestEven);
        assert!(r.is_infinite());
        assert!(!r.is_sign_negative());
    }

    #[test]
    fn exp_neg_inf_is_zero() {
        let (r, _) = Decimal128::NEG_INFINITY.exp(RoundingMode::NearestEven);
        assert!(r.is_zero());
    }

    #[test]
    fn exp_overflow() {
        let big = parse("15000");
        let (r, s) = big.exp(RoundingMode::NearestEven);
        assert!(r.is_infinite());
        assert!(s.overflow());
    }

    #[test]
    fn exp_underflow() {
        let big_neg = parse("-15000");
        let (r, s) = big_neg.exp(RoundingMode::NearestEven);
        assert!(r.is_zero());
        assert!(s.underflow());
    }

    #[test]
    fn exp_subnormal_window_does_not_saturate_to_zero() {
        // Pre-1.13 the underflow gate was symmetric at ±14150, but
        // the real underflow boundary is wider on the negative side.
        // The smallest representable Decimal128 subnormal is
        // `1 × 10⁻⁶¹⁷⁶`, and round-to-nearest-even maps any
        // `exp(x) < ½ × MIN_SUBNORMAL` to +0; that cutoff sits at
        // x ≈ −14220.85. Inputs strictly between −14221 and −14150
        // produce subnormal-but-non-zero results (e.g. exp(−14200)
        // ≈ 10⁻⁶¹⁶⁷) and the kernel must NOT saturate them.
        for s in ["-14151", "-14200", "-14219"] {
            let x = parse(s);
            let (r, st) = x.exp(RoundingMode::NearestEven);
            assert!(
                !r.is_zero(),
                "exp({s}) should produce a representable subnormal, \
                 got 0 (status {st:?})",
            );
            assert!(r.is_finite() && !r.is_sign_negative());
            assert!(st.inexact());
        }
        // Past the round-to-zero boundary, saturate is correct.
        let too_far = parse("-14225");
        let (r, st) = too_far.exp(RoundingMode::NearestEven);
        assert!(r.is_zero(), "exp(-14225) is past MIN_SUBNORMAL/2");
        assert!(st.underflow());
    }

    extern crate alloc;
}
