//! IEEE 754 multiplication for [`Decimal128`].
//!
//! Multiplication is structurally simpler than add/sub: there's no
//! alignment to do, just one widening product and a rounding step.
//! The exception ladder is different though:
//!
//! * `0 × ∞` and `∞ × 0` are *invalid* operations and yield NaN with
//!   the `INVALID` flag set (IEEE 754-2019 §7.2 — these are the only
//!   "invalid finite × infinite" mixes).
//! * `∞ × ∞` and `∞ × finite_non_zero` give `±∞` with the `XORed` sign.
//! * `0 × 0`, `0 × finite`, and `finite × 0` all give `±0` with the
//!   `XORed` sign — IEEE 754 sign rule applies even to zero results.
//!
//! Result quantum: `q_a + q_b`. The coefficient product is up to
//! 226 bits, so the alignment intermediate buys nothing — we just
//! pass the 256-bit product to [`round_and_pack_finite`] with the
//! sum of the unbiased exponents.

use crate::bid::{classify_bits, pack_finite, pack_infinity, Class, BIAS};
use crate::decimal::Decimal128;
use crate::multiword::{u256::widening_mul_u128, U256};
use crate::ops::{propagate_nan2, round_and_pack_finite};
use crate::status::{RoundingMode, Status};

impl Decimal128 {
    /// IEEE 754 `multiplication(self, rhs)`.
    ///
    /// Returns `(self × rhs, status)` rounded according to `rm`.
    #[must_use]
    pub fn mul(self, rhs: Self, rm: RoundingMode) -> (Self, Status) {
        if let Some(early) = mul_special_cases(self, rhs) {
            return early;
        }
        mul_finite_finite(self, rhs, rm)
    }

    /// Kani-only entry point for the special-case path.
    #[cfg(kani)]
    #[doc(hidden)]
    #[must_use]
    pub fn mul_special_only_for_kani(self, rhs: Self) -> Option<(Self, Status)> {
        mul_special_cases(self, rhs)
    }
}

/// Resolve every multiplication branch that doesn't need the
/// 226-bit-product pipeline. Returns `None` only when both operands are
/// finite and non-zero.
#[inline]
fn mul_special_cases(a: Decimal128, b: Decimal128) -> Option<(Decimal128, Status)> {
    let cls_a = classify_bits(a.to_bits());
    let cls_b = classify_bits(b.to_bits());

    let snan =
        matches!(cls_a, Class::SignalingNaN { .. }) || matches!(cls_b, Class::SignalingNaN { .. });
    let mut status = if snan { Status::INVALID } else { Status::OK };

    if matches!(cls_a, Class::QuietNaN { .. } | Class::SignalingNaN { .. })
        || matches!(cls_b, Class::QuietNaN { .. } | Class::SignalingNaN { .. })
    {
        return Some((propagate_nan2(a, b), status));
    }

    let (sign_a, _, _) = decompose_finite_or_inf(cls_a);
    let (sign_b, _, _) = decompose_finite_or_inf(cls_b);
    let result_sign = sign_a ^ sign_b;

    let zero_a = matches!(cls_a, Class::Zero { .. });
    let zero_b = matches!(cls_b, Class::Zero { .. });
    let inf_a = matches!(cls_a, Class::Infinity { .. });
    let inf_b = matches!(cls_b, Class::Infinity { .. });

    // 0 × Inf in either order → NaN + INVALID.
    if (zero_a && inf_b) || (inf_a && zero_b) {
        status |= Status::INVALID;
        return Some((Decimal128::NAN, status));
    }

    if inf_a || inf_b {
        return Some((Decimal128::from_bits(pack_infinity(result_sign)), status));
    }

    if zero_a || zero_b {
        // Quantum of the result is the sum of operand quanta. We pick
        // `q_a + q_b` and clamp to the storable range; non-canonical
        // quantum after clamp is fine for ±0. If the clamp moved the
        // quantum, raise §7.4 Clamped (informational); the zero is exact
        // at every exponent (fd-61r / ADR-0048; matches dqmul504 / dqmul505).
        let (_, ea, _) = decompose_finite_or_zero(cls_a);
        let (_, eb, _) = decompose_finite_or_zero(cls_b);
        let q_biased = ea as i32 + eb as i32 - BIAS as i32;
        let exp = q_biased.clamp(0, crate::bid::BIASED_EXP_MAX as i32);
        if exp != q_biased {
            status |= Status::CLAMPED;
        }
        return Some((
            Decimal128::from_bits(pack_finite(result_sign, exp as u32, 0)),
            status,
        ));
    }

    None
}

fn mul_finite_finite(a: Decimal128, b: Decimal128, rm: RoundingMode) -> (Decimal128, Status) {
    let cls_a = classify_bits(a.to_bits());
    let cls_b = classify_bits(b.to_bits());
    let (sa, ea, ca) = decompose_finite_or_zero(cls_a);
    let (sb, eb, cb) = decompose_finite_or_zero(cls_b);
    debug_assert!(ca != 0 && cb != 0);

    let sign = sa ^ sb;

    // 226-bit product. ca, cb < 10^34 < 2^113, so the product fits.
    let (hi, lo) = widening_mul_u128(ca, cb);
    let coef = U256 { lo, hi };

    // Quantum of the product is the sum of operand quanta.
    let unbiased_exp = (ea as i32 - BIAS as i32) + (eb as i32 - BIAS as i32);

    // IEEE 754 §6.3 preferred quantum for mul is `qa + qb`, which is
    // exactly `unbiased_exp` here.
    round_and_pack_finite(
        coef,
        unbiased_exp,
        unbiased_exp,
        sign,
        false,
        rm,
        Status::OK,
    )
}

/// Decompose a Zero / Finite [`Class`] into `(sign, biased_exp, coefficient)`.
fn decompose_finite_or_zero(c: Class) -> (bool, u32, u128) {
    match c {
        Class::Zero { sign, biased_exp } => (sign, biased_exp, 0),
        Class::Finite {
            sign,
            biased_exp,
            coefficient,
        } => (sign, biased_exp, coefficient),
        _ => {
            debug_assert!(false, "decompose_finite_or_zero on non-finite Class");
            (false, BIAS, 0)
        }
    }
}

/// Decompose any [`Class`] except NaN to extract its sign. Used for the
/// XOR sign rule on the special-case path, including infinities.
fn decompose_finite_or_inf(c: Class) -> (bool, u32, u128) {
    match c {
        Class::Zero { sign, biased_exp } => (sign, biased_exp, 0),
        Class::Finite {
            sign,
            biased_exp,
            coefficient,
        } => (sign, biased_exp, coefficient),
        Class::Infinity { sign } => (sign, BIAS, 0),
        _ => {
            debug_assert!(false, "decompose_finite_or_inf on NaN Class");
            (false, BIAS, 0)
        }
    }
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
        let (r, s) = Decimal128::ONE.mul(Decimal128::NAN, RoundingMode::default());
        assert!(r.is_nan());
        assert!(s.is_ok());
        let (r, s) = Decimal128::NAN.mul(Decimal128::TEN, RoundingMode::default());
        assert!(r.is_nan());
        assert!(s.is_ok());
    }

    #[test]
    fn snan_raises_invalid() {
        let (r, s) = Decimal128::ONE.mul(Decimal128::SIGNALING_NAN, RoundingMode::default());
        assert!(r.is_nan());
        assert!(s.invalid());
    }

    #[test]
    fn zero_times_inf_is_nan_invalid() {
        let (r, s) = Decimal128::ZERO.mul(Decimal128::INFINITY, RoundingMode::default());
        assert!(r.is_nan());
        assert!(s.invalid());

        let (r, s) = Decimal128::INFINITY.mul(Decimal128::ZERO, RoundingMode::default());
        assert!(r.is_nan());
        assert!(s.invalid());

        // Negative variants too.
        let (r, s) = Decimal128::NEG_ZERO.mul(Decimal128::INFINITY, RoundingMode::default());
        assert!(r.is_nan());
        assert!(s.invalid());
        let (r, s) = Decimal128::NEG_INFINITY.mul(Decimal128::NEG_ZERO, RoundingMode::default());
        assert!(r.is_nan());
        assert!(s.invalid());
    }

    #[test]
    fn inf_times_inf_sign_xor() {
        let (r, s) = Decimal128::INFINITY.mul(Decimal128::INFINITY, RoundingMode::default());
        assert!(r.is_infinite());
        assert!(!r.is_sign_negative());
        assert!(s.is_ok());

        let (r, _) = Decimal128::INFINITY.mul(Decimal128::NEG_INFINITY, RoundingMode::default());
        assert!(r.is_infinite());
        assert!(r.is_sign_negative());

        let (r, _) =
            Decimal128::NEG_INFINITY.mul(Decimal128::NEG_INFINITY, RoundingMode::default());
        assert!(r.is_infinite());
        assert!(!r.is_sign_negative());
    }

    #[test]
    fn inf_times_finite_nonzero_is_inf() {
        let (r, _) = Decimal128::INFINITY.mul(Decimal128::ONE, RoundingMode::default());
        assert!(r.is_infinite());
        assert!(!r.is_sign_negative());

        let (r, _) = Decimal128::INFINITY.mul(Decimal128::NEG_ONE, RoundingMode::default());
        assert!(r.is_infinite());
        assert!(r.is_sign_negative());

        let (r, _) = Decimal128::NEG_ONE.mul(Decimal128::NEG_INFINITY, RoundingMode::default());
        assert!(r.is_infinite());
        assert!(!r.is_sign_negative());
    }

    #[test]
    fn zero_times_finite_is_signed_zero() {
        let (r, _) = Decimal128::ZERO.mul(Decimal128::ONE, RoundingMode::default());
        assert!(r.is_zero());
        assert!(!r.is_sign_negative());

        let (r, _) = Decimal128::NEG_ZERO.mul(Decimal128::ONE, RoundingMode::default());
        assert!(r.is_zero());
        assert!(r.is_sign_negative());

        let (r, _) = Decimal128::ZERO.mul(Decimal128::NEG_ONE, RoundingMode::default());
        assert!(r.is_zero());
        assert!(r.is_sign_negative());

        let (r, _) = Decimal128::NEG_ZERO.mul(Decimal128::NEG_ONE, RoundingMode::default());
        assert!(r.is_zero());
        assert!(!r.is_sign_negative());
    }

    #[test]
    fn finite_times_finite_basics() {
        // 2 × 3 = 6
        let (r, _) = d_int(2).mul(d_int(3), RoundingMode::default());
        let (ord, _) = r.partial_cmp(d_int(6));
        assert_eq!(ord, Some(core::cmp::Ordering::Equal));

        // -4 × 5 = -20
        let (r, _) = d_int(-4).mul(d_int(5), RoundingMode::default());
        let (ord, _) = r.partial_cmp(d_int(-20));
        assert_eq!(ord, Some(core::cmp::Ordering::Equal));

        // 7 × 0.1: 7 × (1, BIAS-1) = 0.7 = (7, BIAS-1)
        let (r, _) = d_int(7).mul(d_finite(false, BIAS - 1, 1), RoundingMode::default());
        let expected = d_finite(false, BIAS - 1, 7);
        let (ord, _) = r.partial_cmp(expected);
        assert_eq!(ord, Some(core::cmp::Ordering::Equal));
    }

    #[test]
    fn one_times_x_is_x_numeric() {
        for &v in &[1i128, -1, 7, -42, 1_000, 1_000_000] {
            let x = d_int(v);
            let (r, _) = Decimal128::ONE.mul(x, RoundingMode::default());
            let (ord, _) = r.partial_cmp(x);
            assert_eq!(ord, Some(core::cmp::Ordering::Equal), "1 * {v}");

            let (r, _) = x.mul(Decimal128::ONE, RoundingMode::default());
            let (ord, _) = r.partial_cmp(x);
            assert_eq!(ord, Some(core::cmp::Ordering::Equal), "{v} * 1");
        }
    }

    #[test]
    fn mul_commutative_simple() {
        let pairs = [
            (3i128, 11),
            (-7, 13),
            (123, -456),
            (1_000, 999),
            (10_000_000, 100_000),
        ];
        for (a, b) in pairs {
            let da = d_int(a);
            let db = d_int(b);
            let (ab, _) = da.mul(db, RoundingMode::default());
            let (ba, _) = db.mul(da, RoundingMode::default());
            assert_eq!(ab.to_bits(), ba.to_bits(), "({a}) × ({b})");
        }
    }

    #[test]
    fn mul_at_precision_boundary() {
        // (10^17) × (10^17) = 10^34 — exactly at the precision boundary.
        let big = d_finite(false, BIAS, 10u128.pow(17));
        let (r, _) = big.mul(big, RoundingMode::default());
        // Numerically equals 10^34. The encoding renormalises to
        // (10^33, BIAS+1) = 10^33 × 10^1 = 10^34.
        let expected = d_finite(false, BIAS + 1, 10u128.pow(33));
        let (ord, _) = r.partial_cmp(expected);
        assert_eq!(ord, Some(core::cmp::Ordering::Equal), "got {r:?}");
    }

    #[test]
    fn mul_signs_all_combinations() {
        for &a_sign in &[false, true] {
            for &b_sign in &[false, true] {
                let a = d_finite(a_sign, BIAS, 7);
                let b = d_finite(b_sign, BIAS, 11);
                let (r, _) = a.mul(b, RoundingMode::default());
                assert_eq!(r.is_sign_negative(), a_sign ^ b_sign);
            }
        }
    }

    #[test]
    fn mul_overflow_to_infinity() {
        // 9.99...9 × 10^6144 (MAX) times 10 → overflow.
        let max = Decimal128::MAX;
        let ten = Decimal128::TEN;
        let (r, s) = max.mul(ten, RoundingMode::default());
        assert!(r.is_infinite(), "got {r:?}");
        assert!(s.overflow());
        assert!(s.inexact());
    }
}
