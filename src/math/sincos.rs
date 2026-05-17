//! Re-export + delegating shim: the sincos kernel moved to
//! ferrodec-transcend (P0a.2 c8). The public `Decimal128::sin` /
//! `cos` / `tan` wrappers and their behaviour tests stay here as the
//! byte-identical regression gate.

use crate::decimal::Decimal128;
use ferrodec_ieee::{RoundingMode, Status};

impl Decimal128 {
    /// Sine, in radians.
    #[must_use]
    pub fn sin(self, rm: RoundingMode) -> (Self, Status) {
        ferrodec_transcend::sincos::sin_kernel::<Decimal128>(self, rm)
    }

    /// Cosine, in radians.
    #[must_use]
    pub fn cos(self, rm: RoundingMode) -> (Self, Status) {
        ferrodec_transcend::sincos::cos_kernel::<Decimal128>(self, rm)
    }

    /// Tangent, in radians.
    ///
    /// `tan(x) = sin(x) / cos(x)`, computed by dividing the two
    /// extended-precision sin/cos values before rounding to
    /// `Decimal128`. At `cos(x) = 0` (odd multiples of π/2) the
    /// result diverges; we return `±∞` without raising
    /// `DIV_BY_ZERO` (since `tan` of a finite input doesn't fit the
    /// IEEE 754 §7.3 division-by-zero condition — it's just an
    /// asymptote).
    #[must_use]
    pub fn tan(self, rm: RoundingMode) -> (Self, Status) {
        ferrodec_transcend::sincos::tan_kernel::<Decimal128>(self, rm)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::consts::pi;
    extern crate alloc;
    use alloc::format;

    fn parse(s: &str) -> Decimal128 {
        Decimal128::parse_str(s, RoundingMode::NearestEven)
            .unwrap()
            .0
    }

    fn approx_equal_ulps(a: Decimal128, b: Decimal128, ulps: u32) -> bool {
        let (diff, _) = a.sub(b, RoundingMode::NearestEven);
        let diff = diff.abs();
        let abs_b = b.abs();
        if abs_b.is_zero() {
            // Absolute tolerance for values near zero.
            let bound = parse(&format!("{ulps}e-30"));
            let (cmp, _) = diff.partial_cmp(bound);
            return matches!(
                cmp,
                Some(core::cmp::Ordering::Less | core::cmp::Ordering::Equal)
            );
        }
        let (rel, _) = diff.div(abs_b, RoundingMode::NearestEven);
        let bound = parse(&format!("{ulps}e-33"));
        let (cmp, _) = rel.partial_cmp(bound);
        matches!(
            cmp,
            Some(core::cmp::Ordering::Less | core::cmp::Ordering::Equal)
        )
    }

    #[test]
    fn sin_zero_is_zero() {
        let (r, _) = Decimal128::ZERO.sin(RoundingMode::NearestEven);
        assert!(r.is_zero());
        assert!(!r.is_sign_negative());

        let (r, _) = Decimal128::NEG_ZERO.sin(RoundingMode::NearestEven);
        assert!(r.is_zero());
        assert!(r.is_sign_negative());
    }

    #[test]
    fn cos_zero_is_one() {
        let (r, _) = Decimal128::ZERO.cos(RoundingMode::NearestEven);
        let (cmp, _) = r.partial_cmp(Decimal128::ONE);
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));
    }

    #[test]
    fn sin_pi_is_approximately_zero() {
        let (r, _) = pi().sin(RoundingMode::NearestEven);
        assert!(approx_equal_ulps(r, Decimal128::ZERO, 100));
    }

    #[test]
    fn cos_pi_is_neg_one() {
        let (r, _) = pi().cos(RoundingMode::NearestEven);
        assert!(approx_equal_ulps(r, Decimal128::NEG_ONE, 50));
    }

    #[test]
    fn sin_half_pi_is_one() {
        let (half_pi, _) = pi().div(Decimal128::from_i32(2), RoundingMode::NearestEven);
        let (s, _) = half_pi.sin(RoundingMode::NearestEven);
        assert!(approx_equal_ulps(s, Decimal128::ONE, 50));
    }

    #[test]
    fn cos_half_pi_is_zero() {
        let (half_pi, _) = pi().div(Decimal128::from_i32(2), RoundingMode::NearestEven);
        let (c, _) = half_pi.cos(RoundingMode::NearestEven);
        assert!(approx_equal_ulps(c, Decimal128::ZERO, 100));
    }

    #[test]
    fn pythagorean_identity() {
        // sin²(x) + cos²(x) = 1, for various x.
        for s in &["0.5", "1", "-1", "1.5", "3", "-2.7"] {
            let x = parse(s);
            let (sin_x, _) = x.sin(RoundingMode::NearestEven);
            let (cos_x, _) = x.cos(RoundingMode::NearestEven);
            let (sin_sq, _) = sin_x.mul(sin_x, RoundingMode::NearestEven);
            let (cos_sq, _) = cos_x.mul(cos_x, RoundingMode::NearestEven);
            let (sum, _) = sin_sq.add(cos_sq, RoundingMode::NearestEven);
            assert!(
                approx_equal_ulps(sum, Decimal128::ONE, 200),
                "sin²({s}) + cos²({s}) = {sum:?}, want ≈ 1"
            );
        }
    }

    #[test]
    fn sin_nan_propagates() {
        let (r, _) = Decimal128::NAN.sin(RoundingMode::NearestEven);
        assert!(r.is_nan());
        let (r, s) = Decimal128::SIGNALING_NAN.sin(RoundingMode::NearestEven);
        assert!(r.is_nan());
        assert!(s.invalid());
    }

    #[test]
    fn trig_qnan_preserves_payload_bit_for_bit() {
        // The qNaN arms of `sin`/`cos`/`tan` return `self` directly,
        // so the bit pattern (sign, signaling-flag, full 110-bit
        // payload) must come through unchanged. Pin this with a
        // distinctive payload so a future refactor that funnels qNaN
        // through `nan_from` (which canonicalises) would fail loud.
        let payload: u128 = 0x0000_DEAD_BEEF_CAFE_BA5E;
        let qnan = Decimal128::from_bits(crate::bid::pack_quiet_nan(true, payload));
        for &op in &[
            Decimal128::sin as fn(Decimal128, RoundingMode) -> (Decimal128, Status),
            Decimal128::cos,
            Decimal128::tan,
        ] {
            let (r, s) = op(qnan, RoundingMode::NearestEven);
            assert_eq!(r.to_bits(), qnan.to_bits(), "qNaN bits must pass through");
            assert!(!s.invalid(), "qNaN must not raise INVALID");
        }

        // sNaN gets quieted (signaling bit cleared) but the payload
        // bits should still survive — propagate_nan / nan_from
        // routes through `pack_quiet_nan(sign, payload)`. Verifies
        // the payload-preservation invariant on the sNaN side too.
        let snan = Decimal128::from_bits(crate::bid::pack_signaling_nan(false, payload));
        for &op in &[
            Decimal128::sin as fn(Decimal128, RoundingMode) -> (Decimal128, Status),
            Decimal128::cos,
            Decimal128::tan,
        ] {
            let (r, s) = op(snan, RoundingMode::NearestEven);
            assert!(r.is_nan() && r.is_quiet_nan(), "sNaN gets quieted");
            assert!(s.invalid(), "sNaN must raise INVALID");
            let r_payload = r.to_bits() & ((1u128 << 110) - 1);
            assert_eq!(r_payload, payload, "sNaN payload must survive");
        }
    }

    #[test]
    fn sin_inf_is_invalid_nan() {
        let (r, s) = Decimal128::INFINITY.sin(RoundingMode::NearestEven);
        assert!(r.is_nan());
        assert!(s.invalid());
        let (r, s) = Decimal128::NEG_INFINITY.cos(RoundingMode::NearestEven);
        assert!(r.is_nan());
        assert!(s.invalid());
    }

    #[test]
    fn sin_neg_x_is_neg_sin_x() {
        // sin is odd. Compare bit-by-bit modulo small ULP for non-special x.
        for s in &["0.7", "1.1", "2.5"] {
            let x = parse(s);
            let (sin_x, _) = x.sin(RoundingMode::NearestEven);
            let (sin_neg, _) = x.neg().sin(RoundingMode::NearestEven);
            // sin(-x) ≈ -sin(x); allow ~10 ULP of drift.
            assert!(
                approx_equal_ulps(sin_neg, sin_x.neg(), 50),
                "sin(-{s}) symmetry"
            );
        }
    }

    #[test]
    fn cos_neg_x_is_cos_x() {
        for s in &["0.7", "1.1", "2.5"] {
            let x = parse(s);
            let (cos_x, _) = x.cos(RoundingMode::NearestEven);
            let (cos_neg, _) = x.neg().cos(RoundingMode::NearestEven);
            assert!(approx_equal_ulps(cos_neg, cos_x, 50), "cos(-{s}) symmetry");
        }
    }
}
