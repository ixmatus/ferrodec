//! Re-export + delegating shim: the ln kernel moved to
//! ferrodec-transcend (P0a.2 c5). The public `Decimal128::ln` /
//! `log10` / `log2` wrappers and their behaviour tests stay here as
//! the byte-identical regression gate.

use crate::decimal::Decimal128;
use ferrodec_ieee::{RoundingMode, Status};

// The still-in-core pow.rs / cbrt.rs use `crate::math::ln::ln_extended`
// and hyperbolic.rs uses `crate::math::ln::{ln_from_extended,
// log1p_extended}`. These re-exports keep those imports resolving
// unchanged: `ln_extended` is generic with F inferred from its argument,
// the other two are non-generic.
#[allow(unused_imports)]
pub(crate) use ferrodec_transcend::ln::{ln_extended, ln_from_extended, log1p_extended};

impl Decimal128 {
    /// Natural logarithm `ln(self)`.
    #[must_use]
    pub fn ln(self, rm: RoundingMode) -> (Self, Status) {
        ferrodec_transcend::ln::ln_kernel::<Decimal128>(self, rm)
    }

    /// Base-10 logarithm `log10(self)`. Computed as
    /// `ln_extended(self) · (1/ln(10))_extended`, then rounded once.
    #[must_use]
    pub fn log10(self, rm: RoundingMode) -> (Self, Status) {
        ferrodec_transcend::ln::log10_kernel::<Decimal128>(self, rm)
    }

    /// Base-2 logarithm `log2(self)`. Computed as
    /// `ln_extended(self) · (1/ln(2))_extended`, then rounded once.
    #[must_use]
    pub fn log2(self, rm: RoundingMode) -> (Self, Status) {
        ferrodec_transcend::ln::log2_kernel::<Decimal128>(self, rm)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::consts::ln10;
    extern crate alloc;

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
            let bound = parse(&alloc::format!("{ulps}e-33"));
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
    fn ln_one_is_zero() {
        let (r, _) = Decimal128::ONE.ln(RoundingMode::NearestEven);
        assert!(r.is_zero());
    }

    #[test]
    fn ln_e_is_one() {
        let e_val = crate::math::e();
        let (r, _) = e_val.ln(RoundingMode::NearestEven);
        assert!(within_ulps(r, Decimal128::ONE, 1));
    }

    #[test]
    fn ln_ten_is_ln10_const() {
        let ten = Decimal128::TEN;
        let (r, _) = ten.ln(RoundingMode::NearestEven);
        let target = ln10();
        assert!(within_ulps(r, target, 1));
    }

    #[test]
    fn ln_two_is_ln2_const() {
        let two = Decimal128::from_i32(2);
        let (r, _) = two.ln(RoundingMode::NearestEven);
        let target = parse("0.693147180559945309417232121458176568");
        assert!(within_ulps(r, target, 1));
    }

    #[test]
    fn ln_zero_is_neg_inf_div_by_zero() {
        let (r, s) = Decimal128::ZERO.ln(RoundingMode::NearestEven);
        assert!(r.is_infinite());
        assert!(r.is_sign_negative());
        assert!(s.div_by_zero());
    }

    #[test]
    fn ln_negative_is_invalid_nan() {
        let (r, s) = Decimal128::NEG_ONE.ln(RoundingMode::NearestEven);
        assert!(r.is_nan());
        assert!(s.invalid());
    }

    #[test]
    fn ln_inf_is_inf() {
        let (r, _) = Decimal128::INFINITY.ln(RoundingMode::NearestEven);
        assert!(r.is_infinite());
        assert!(!r.is_sign_negative());

        let (r, s) = Decimal128::NEG_INFINITY.ln(RoundingMode::NearestEven);
        assert!(r.is_nan());
        assert!(s.invalid());
    }

    #[test]
    fn ln_nan_propagates() {
        let (r, _) = Decimal128::NAN.ln(RoundingMode::NearestEven);
        assert!(r.is_nan());
        let (r, s) = Decimal128::SIGNALING_NAN.ln(RoundingMode::NearestEven);
        assert!(r.is_nan());
        assert!(s.invalid());
    }

    #[test]
    fn log10_powers_of_ten() {
        for &p in &[1i32, 2, 3, 4, 5, 10, -1, -3, 100, -100] {
            let x = parse(&alloc::format!("1e{p}"));
            let (r, _) = x.log10(RoundingMode::NearestEven);
            let target = Decimal128::from_i32(p);
            assert!(
                within_ulps(r, target, 1),
                "log10(1e{p}) = {r:?}, want {target:?}"
            );
        }
    }

    #[test]
    fn ln_exp_roundtrip() {
        for &v in &["0.5", "1.5", "2", "5", "10", "100"] {
            let x = parse(v);
            let (lx, _) = x.ln(RoundingMode::NearestEven);
            let (back, _) = lx.exp(RoundingMode::NearestEven);
            assert!(
                within_ulps(back, x, 5),
                "exp(ln({v})) = {back:?}, want {x:?}"
            );
        }
    }
}
