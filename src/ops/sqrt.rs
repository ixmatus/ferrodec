//! IEEE 754 square root for [`Decimal128`].
//!
//! Special cases (IEEE 754-2019 §5.4.1):
//!
//! * `√NaN` propagates NaN; signaling NaN raises `INVALID`.
//! * `√(-0)` is `−0`. `√(+0)` is `+0`.
//! * `√(+∞)` is `+∞`. `√(−∞)` is NaN with `INVALID`.
//! * `√(negative_finite)` is NaN with `INVALID`.
//!
//! Finite-positive path: scale the coefficient by `10^k` so that
//! `c × 10^k` has roughly `2 × (PRECISION + 1)` decimal digits, with `k`
//! adjusted by ±1 so that `(q − k)` is even (the result quantum is
//! `(q − k) / 2`). Take the integer square root via [`U256::isqrt`];
//! the integer remainder threads through to
//! [`round_and_pack_finite`] as the pre-sticky bit, ensuring correct
//! rounding for every IEEE rounding mode.
//!
//! For `Decimal128` (`PRECISION = 34`) the chosen `k` lies in `[36, 70]`,
//! keeping `c × 10^k < 10^70 < 2^234` — well within U256.

use crate::bid::{
    classify_bits, decimal_digit_count, pack_finite, pack_infinity, Class, BIAS, PRECISION,
};
use crate::decimal::Decimal128;
use crate::multiword::U256;
use crate::ops::round_and_pack_finite;
use crate::status::{RoundingMode, Status};

impl Decimal128 {
    /// IEEE 754 `squareRoot(self)`.
    #[must_use]
    pub fn sqrt(self, rm: RoundingMode) -> (Self, Status) {
        if let Some(early) = sqrt_special_cases(self) {
            return early;
        }
        sqrt_finite(self, rm)
    }

    /// Kani-only entry point for the special-case path.
    #[cfg(kani)]
    #[doc(hidden)]
    #[must_use]
    pub fn sqrt_special_only_for_kani(self) -> Option<(Self, Status)> {
        sqrt_special_cases(self)
    }
}

#[inline]
fn sqrt_special_cases(a: Decimal128) -> Option<(Decimal128, Status)> {
    let cls = classify_bits(a.to_bits());

    let snan = matches!(cls, Class::SignalingNaN { .. });
    let status = if snan { Status::INVALID } else { Status::OK };

    if matches!(cls, Class::QuietNaN { .. } | Class::SignalingNaN { .. }) {
        return Some((Decimal128::NAN, status));
    }

    match cls {
        Class::Zero { sign, biased_exp } => {
            // sqrt(±0) = ±0. Quantum convention: the sqrt of a zero with
            // quantum q has quantum ⌊q/2⌋. We just keep the input quantum;
            // a stricter implementation would re-emit at ⌊q/2⌋.
            let _ = biased_exp;
            Some((
                Decimal128::from_bits(pack_finite(sign, biased_exp, 0)),
                status,
            ))
        }
        Class::Infinity { sign: false } => {
            Some((Decimal128::from_bits(pack_infinity(false)), status))
        }
        Class::Infinity { sign: true } => Some((Decimal128::NAN, status | Status::INVALID)),
        Class::Finite { sign: true, .. } => Some((Decimal128::NAN, status | Status::INVALID)),
        Class::Finite { sign: false, .. } => None,
        // NaN cases handled above; unreachable here.
        Class::QuietNaN { .. } | Class::SignalingNaN { .. } => unreachable!(),
    }
}

fn sqrt_finite(a: Decimal128, rm: RoundingMode) -> (Decimal128, Status) {
    let cls = classify_bits(a.to_bits());
    let (sign, ea, ca) = match cls {
        Class::Finite {
            sign,
            biased_exp,
            coefficient,
        } => (sign, biased_exp, coefficient),
        _ => {
            debug_assert!(false, "sqrt_finite called on non-finite-positive");
            return (Decimal128::NAN, Status::INVALID);
        }
    };
    debug_assert!(!sign);
    debug_assert!(ca > 0);

    let q_a = ea as i32 - BIAS as i32;
    let digits_c = decimal_digit_count(ca);

    // Scale so the integer sqrt result has at least PRECISION+1 digits.
    let mut k: i32 = 2 * (PRECISION as i32 + 1) - digits_c as i32;
    // Result quantum is (q_a − k) / 2 — must be an integer, so adjust
    // `k` by 1 to match the parity of `q_a`.
    if (q_a - k) & 1 != 0 {
        k += 1;
    }
    debug_assert!(k > 0, "sqrt scaling factor went non-positive");
    let k_u32 = k as u32;

    let scaled = U256::from_u128(ca).mul_pow10(k_u32);
    let (root, rem) = scaled.isqrt();

    let result_unbiased_exp = (q_a - k) / 2;
    let pre_sticky = !rem.is_zero();

    // IEEE 754 §6.3 preferred quantum for sqrt is `floor(qa / 2)`.
    let q_preferred = q_a.div_euclid(2);
    round_and_pack_finite(
        U256::from_u128(root),
        result_unbiased_exp,
        q_preferred,
        false,
        pre_sticky,
        rm,
        Status::OK,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bid::pack_finite;

    fn d_finite(s: bool, exp: u32, coef: u128) -> Decimal128 {
        Decimal128::from_bits(pack_finite(s, exp, coef))
    }

    fn d_int(c: i128) -> Decimal128 {
        if c == 0 {
            return Decimal128::ZERO;
        }
        let sign = c < 0;
        let coef = c.unsigned_abs();
        d_finite(sign, BIAS, coef)
    }

    #[test]
    fn nan_propagates() {
        let (r, s) = Decimal128::NAN.sqrt(RoundingMode::default());
        assert!(r.is_nan());
        assert!(s.is_ok());

        let (r, s) = Decimal128::SIGNALING_NAN.sqrt(RoundingMode::default());
        assert!(r.is_nan());
        assert!(s.invalid());
    }

    #[test]
    fn zero_sqrt() {
        let (r, _) = Decimal128::ZERO.sqrt(RoundingMode::default());
        assert!(r.is_zero());
        assert!(!r.is_sign_negative());

        let (r, _) = Decimal128::NEG_ZERO.sqrt(RoundingMode::default());
        assert!(r.is_zero());
        assert!(r.is_sign_negative());
    }

    #[test]
    fn pos_inf_sqrt_is_pos_inf() {
        let (r, s) = Decimal128::INFINITY.sqrt(RoundingMode::default());
        assert!(r.is_infinite());
        assert!(!r.is_sign_negative());
        assert!(s.is_ok());
    }

    #[test]
    fn neg_inf_sqrt_is_invalid_nan() {
        let (r, s) = Decimal128::NEG_INFINITY.sqrt(RoundingMode::default());
        assert!(r.is_nan());
        assert!(s.invalid());
    }

    #[test]
    fn negative_finite_sqrt_is_invalid_nan() {
        let (r, s) = Decimal128::NEG_ONE.sqrt(RoundingMode::default());
        assert!(r.is_nan());
        assert!(s.invalid());

        let (r, s) = Decimal128::MIN.sqrt(RoundingMode::default());
        assert!(r.is_nan());
        assert!(s.invalid());
    }

    #[test]
    fn perfect_squares() {
        // sqrt(4) = 2, sqrt(9) = 3, sqrt(16) = 4, …
        for &(n, root) in &[
            (4i128, 2i128),
            (9, 3),
            (16, 4),
            (25, 5),
            (100, 10),
            (10_000, 100),
        ] {
            let (r, _) = d_int(n).sqrt(RoundingMode::default());
            let (cmp, _) = r.partial_cmp(d_int(root));
            assert_eq!(cmp, Some(core::cmp::Ordering::Equal), "sqrt({n})");
        }
    }

    #[test]
    fn sqrt_two_inexact() {
        // sqrt(2) ≈ 1.4142135... Inexact at 34 digits.
        let (r, s) = d_int(2).sqrt(RoundingMode::NearestEven);
        assert!(r.is_finite());
        assert!(!r.is_zero());
        assert!(s.inexact());
        // Coarse sanity: r should be between 1 and 2.
        let (cmp_lo, _) = r.partial_cmp(Decimal128::ONE);
        assert_eq!(cmp_lo, Some(core::cmp::Ordering::Greater));
        let (cmp_hi, _) = r.partial_cmp(d_int(2));
        assert_eq!(cmp_hi, Some(core::cmp::Ordering::Less));
    }

    #[test]
    fn sqrt_one_is_one() {
        let (r, _) = Decimal128::ONE.sqrt(RoundingMode::default());
        let (cmp, _) = r.partial_cmp(Decimal128::ONE);
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));
    }

    #[test]
    fn sqrt_squared_is_input_for_perfect_squares() {
        // For perfect squares the result is exact; squaring should give back
        // the input numerically.
        for &n in &[4i128, 9, 16, 25, 100, 10_000, 1_000_000_000_000] {
            let x = d_int(n);
            let (root, _) = x.sqrt(RoundingMode::default());
            let (squared, _) = root.mul(root, RoundingMode::default());
            let (cmp, _) = squared.partial_cmp(x);
            assert_eq!(cmp, Some(core::cmp::Ordering::Equal), "sqrt({n})^2");
        }
    }
}
