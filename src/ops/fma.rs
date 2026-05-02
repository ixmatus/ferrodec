//! IEEE 754 fused multiply-add for [`Decimal128`].
//!
//! ## v1 limitation: separated rounding
//!
//! True IEEE 754 §5.4.1 `fusedMultiplyAdd(a, b, c)` rounds **once** —
//! the intermediate `a × b` is held as an exact 2×PRECISION-digit
//! product and only the final sum is rounded.
//!
//! This v1 implementation computes `(a × b) + c` with two rounding
//! steps. For most calculator workloads this is within 1 ULP of the
//! correctly-rounded fma. It also has one extra observable
//! difference: if `a × b` overflows but `(a × b) + c` would not (e.g.
//! `MAX + MAX − MAX`), the v1 result is `±∞ + OVERFLOW` whereas a
//! true fma would have produced a finite result.
//!
//! Both gaps are tracked as a follow-up. The single-rounding pipeline
//! requires a U384 (≥ 3×PRECISION-digit alignment buffer) to avoid
//! losing precision in the worst-case operand mix; that's its own
//! piece of work.
//!
//! ### Specials handled by the separated formulation
//!
//! Every IEEE 754 fma special case still falls out correctly:
//!
//! * Any NaN operand → NaN; any sNaN operand raises `INVALID`.
//! * `0 × ∞` (or `∞ × 0`) regardless of `c` → NaN + `INVALID`
//!   (the `mul` step raises it; then `NaN + c = NaN`).
//! * `∞ × finite_nonzero + opposing_sign_∞` → NaN + `INVALID`
//!   (mul yields `±∞`, then `+∞ + −∞ = NaN + INVALID` in add).
//! * Sign of zero in `0 × x` and `(±0) + (±0)` follows the
//!   composition's rules.

use crate::decimal::Decimal128;
use crate::status::{RoundingMode, Status};

impl Decimal128 {
    /// IEEE 754 `fusedMultiplyAdd(self, b, c)` — see module docs for the
    /// v1 separated-rounding caveat.
    #[must_use]
    pub fn fma(self, b: Self, c: Self, rm: RoundingMode) -> (Self, Status) {
        let (product, st1) = self.mul(b, rm);
        let (sum, st2) = product.add(c, rm);
        (sum, st1 | st2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bid::{pack_finite, BIAS};

    fn d_int(c: i128) -> Decimal128 {
        if c == 0 {
            return Decimal128::ZERO;
        }
        let sign = c < 0;
        let coef = c.unsigned_abs();
        Decimal128::from_bits(pack_finite(sign, BIAS, coef))
    }

    #[test]
    fn nan_propagates() {
        let (r, _) = Decimal128::ONE.fma(Decimal128::NAN, Decimal128::ONE, RoundingMode::default());
        assert!(r.is_nan());

        let (r, s) = Decimal128::SIGNALING_NAN.fma(
            Decimal128::ONE,
            Decimal128::ONE,
            RoundingMode::default(),
        );
        assert!(r.is_nan());
        assert!(s.invalid());

        let (r, s) = Decimal128::ONE.fma(
            Decimal128::ONE,
            Decimal128::SIGNALING_NAN,
            RoundingMode::default(),
        );
        assert!(r.is_nan());
        assert!(s.invalid());
    }

    #[test]
    fn zero_times_inf_is_invalid_nan() {
        let (r, s) = Decimal128::ZERO.fma(
            Decimal128::INFINITY,
            Decimal128::ONE,
            RoundingMode::default(),
        );
        assert!(r.is_nan());
        assert!(s.invalid());
    }

    #[test]
    fn fma_basic() {
        // 2 * 3 + 4 = 10
        let (r, _) = d_int(2).fma(d_int(3), d_int(4), RoundingMode::default());
        let (cmp, _) = r.partial_cmp(d_int(10));
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));

        // 5 * (-7) + 100 = 65
        let (r, _) = d_int(5).fma(d_int(-7), d_int(100), RoundingMode::default());
        let (cmp, _) = r.partial_cmp(d_int(65));
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));
    }

    #[test]
    fn fma_with_zero_addend() {
        let (r, _) = d_int(7).fma(d_int(11), Decimal128::ZERO, RoundingMode::default());
        let (cmp, _) = r.partial_cmp(d_int(77));
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));
    }

    #[test]
    fn fma_with_one_multiplier() {
        // 1 * a + 0 = a (numerically)
        for &v in &[1i128, -1, 7, -42] {
            let (r, _) =
                Decimal128::ONE.fma(d_int(v), Decimal128::ZERO, RoundingMode::default());
            let (cmp, _) = r.partial_cmp(d_int(v));
            assert_eq!(cmp, Some(core::cmp::Ordering::Equal));
        }
    }
}
