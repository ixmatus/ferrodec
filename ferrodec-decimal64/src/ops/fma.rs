//! IEEE 754-2019 fused multiply-add for [`Decimal64`].
//!
//! u128 working width. The exact product `coef_a × coef_b` fits in
//! u128 (max (10¹⁶ − 1)² ≈ 10³²); aligning with `c` over u128 fits
//! whenever `digit_count(operand) + shift ≤ 38`. The shift bound is
//! dynamic, not static: a small operand (e.g. `1 × 1 = 1`) leaves
//! plenty of headroom for alignment even when the static bound
//! `MAX_SHIFT = 6` would not.

use crate::bid::{classify_bits, BIAS, Class};
use crate::decimal::Decimal64;
use ferrodec_ieee::{decimal_digit_count_u128, RoundingMode, Status};

use super::addsub::round_and_pack_into_u64;

const POW10_U128: [u128; 39] = {
    let mut t = [0u128; 39];
    let mut i = 0;
    let mut v: u128 = 1;
    while i < 39 {
        t[i] = v;
        if i < 38 {
            v *= 10;
        }
        i += 1;
    }
    t
};

// Compile-time invariant: the largest reachable index is
// `U128_DIGIT_CAP = 38`. The table needs ≥ 39 entries.
const _: () = assert!(POW10_U128.len() > 38);

/// Upper bound on `digit_count(coef) + shift` that keeps the product
/// within `u128::MAX`. `10^38 < 2^128 ≈ 3.4 × 10³⁸`, so any product
/// with at most 38 decimal digits fits.
///
/// A *static* `MAX_SHIFT` bound (the previous design, `MAX_SHIFT = 6`)
/// is wrong: it assumes the product `ab_coef` is near its maximum
/// (~10³² digits), so only 6 digits of alignment headroom remain.
/// But `ab_coef` can be much smaller — `1 × 1 = 1` has 1 digit, so
/// 37 digits of alignment headroom remain. Using the static bound,
/// `fma(1, 1, 0.999999999999999)` mis-classified `ab` as dominant
/// and dropped `c`. The dynamic bound below uses
/// `digit_count(ab_coef)` instead, restoring correctness whenever
/// the actual operand fits.
const U128_DIGIT_CAP: u32 = 38;

impl Decimal64 {
    /// IEEE 754-2019 `fusedMultiplyAdd(self, b, c)` rounded by `rm`.
    #[must_use]
    pub fn fma(self, b: Self, c: Self, rm: RoundingMode) -> (Self, Status) {
        let ca = classify_bits(self.0);
        let cb = classify_bits(b.0);
        let cc = classify_bits(c.0);

        if let Some(out) = handle_specials(ca, cb, cc) {
            return out;
        }

        let (sign_a, biased_a, coef_a) = match ca {
            Class::Finite { sign, biased_exp, coefficient } => (sign, biased_exp, coefficient),
            Class::Zero { sign, biased_exp } => (sign, biased_exp, 0u64),
            _ => unreachable!(),
        };
        let (sign_b, biased_b, coef_b) = match cb {
            Class::Finite { sign, biased_exp, coefficient } => (sign, biased_exp, coefficient),
            Class::Zero { sign, biased_exp } => (sign, biased_exp, 0u64),
            _ => unreachable!(),
        };
        let (sign_c, biased_c, coef_c) = match cc {
            Class::Finite { sign, biased_exp, coefficient } => (sign, biased_exp, coefficient),
            Class::Zero { sign, biased_exp } => (sign, biased_exp, 0u64),
            _ => unreachable!(),
        };

        let ab_sign = sign_a ^ sign_b;
        let ab_coef = u128::from(coef_a) * u128::from(coef_b);
        let ab_exp = (biased_a as i32 - BIAS as i32) + (biased_b as i32 - BIAS as i32);
        let c_exp = biased_c as i32 - BIAS as i32;

        let target_q = ab_exp.min(c_exp);

        // Both zero: §6.3 sign rule for the cancellation `±0 + ±0`.
        if ab_coef == 0 && coef_c == 0 {
            let result_sign = zero_sum_sign(ab_sign, sign_c, rm);
            return (
                Decimal64::from_bits(crate::bid::pack_finite(
                    result_sign,
                    (target_q + BIAS as i32) as u32,
                    0,
                )),
                Status::OK,
            );
        }

        // Zero product with non-zero c: result is c rebased to the
        // preferred quantum. The non-zero summand's sign wins;
        // ab_sign / zero_sum_sign do not apply.
        if ab_coef == 0 {
            return round_and_pack_into_u64(
                u128::from(coef_c),
                c_exp,
                target_q,
                sign_c,
                false,
                rm,
            );
        }

        // Zero c with non-zero product: result is ab rebased.
        if coef_c == 0 {
            return round_and_pack_into_u64(
                ab_coef,
                ab_exp,
                target_q,
                ab_sign,
                false,
                rm,
            );
        }

        let shift_ab = (ab_exp - target_q) as u32;
        let shift_c = (c_exp - target_q) as u32;

        let ab_digits = decimal_digit_count_u128(ab_coef);
        let c_digits = decimal_digit_count_u128(u128::from(coef_c));
        let ab_safe_shift = U128_DIGIT_CAP - ab_digits;
        let c_safe_shift = U128_DIGIT_CAP - c_digits;

        let mut pre_sticky = false;

        // If aligning either operand into u128 would overflow, that
        // side's value at `target_q` exceeds `10³⁸`, which is far
        // beyond the other side's representable range (at most ~16
        // digits at `target_q`). It therefore *actually* dominates,
        // and the early-return is correct.
        let ab_u128: u128 = if shift_ab <= ab_safe_shift {
            ab_coef * POW10_U128[shift_ab as usize]
        } else {
            pre_sticky |= coef_c != 0;
            return round_and_pack_into_u64(ab_coef, ab_exp, ab_exp, ab_sign, pre_sticky, rm);
        };

        let c_u128: u128 = if shift_c <= c_safe_shift {
            u128::from(coef_c) * POW10_U128[shift_c as usize]
        } else {
            pre_sticky |= ab_coef != 0;
            return round_and_pack_into_u64(
                u128::from(coef_c),
                c_exp,
                c_exp,
                sign_c,
                pre_sticky,
                rm,
            );
        };

        let (combined_coef, combined_sign) = if ab_sign == sign_c {
            (ab_u128 + c_u128, ab_sign)
        } else if ab_u128 > c_u128 {
            (ab_u128 - c_u128, ab_sign)
        } else if c_u128 > ab_u128 {
            (c_u128 - ab_u128, sign_c)
        } else {
            let q_preferred = target_q;
            let result_sign = zero_sum_sign(ab_sign, sign_c, rm);
            return (
                Decimal64::from_bits(crate::bid::pack_finite(
                    result_sign,
                    (q_preferred + BIAS as i32) as u32,
                    0,
                )),
                Status::OK,
            );
        };

        round_and_pack_into_u64(combined_coef, target_q, target_q, combined_sign, pre_sticky, rm)
    }
}

#[inline]
fn zero_sum_sign(sign_a: bool, sign_b: bool, rm: RoundingMode) -> bool {
    if sign_a == sign_b {
        return sign_a;
    }
    matches!(rm, RoundingMode::TowardNegative)
}

fn handle_specials(a: Class, b: Class, c: Class) -> Option<(Decimal64, Status)> {
    use Class::{Finite, Infinity, QuietNaN, SignalingNaN, Zero};

    for cls in [a, b, c] {
        if let SignalingNaN { sign, payload } = cls {
            return Some((
                Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::INVALID,
            ));
        }
    }

    let zero_inf =
        matches!((a, b), (Zero { .. }, Infinity { .. }) | (Infinity { .. }, Zero { .. }));
    if zero_inf {
        return Some((Decimal64::NAN, Status::INVALID));
    }

    for cls in [a, b, c] {
        if let QuietNaN { sign, payload } = cls {
            return Some((
                Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::OK,
            ));
        }
    }

    let multiply_yields_infinity = matches!(a, Infinity { .. }) || matches!(b, Infinity { .. });

    if multiply_yields_infinity {
        let sa = match a {
            Infinity { sign } | Finite { sign, .. } | Zero { sign, .. } => sign,
            _ => unreachable!(),
        };
        let sb = match b {
            Infinity { sign } | Finite { sign, .. } | Zero { sign, .. } => sign,
            _ => unreachable!(),
        };
        let inf_sign = sa ^ sb;

        match c {
            Infinity { sign: sc } => {
                if sc == inf_sign {
                    return Some((
                        Decimal64::from_bits(crate::bid::pack_infinity(inf_sign)),
                        Status::OK,
                    ));
                }
                return Some((Decimal64::NAN, Status::INVALID));
            }
            Finite { .. } | Zero { .. } => {
                return Some((
                    Decimal64::from_bits(crate::bid::pack_infinity(inf_sign)),
                    Status::OK,
                ));
            }
            _ => unreachable!(),
        }
    }

    if let Infinity { sign } = c {
        return Some((
            Decimal64::from_bits(crate::bid::pack_infinity(sign)),
            Status::OK,
        ));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn from_int(n: i64, exp: i32) -> Decimal64 {
        Decimal64::try_new(n, exp).unwrap()
    }

    #[test]
    fn fma_basic() {
        let (r, s) = from_int(2, 0).fma(from_int(3, 0), from_int(4, 0), RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), from_int(10, 0).to_bits());
        assert!(s.is_ok());
    }

    #[test]
    fn fma_zero_addend() {
        let (r, _) = from_int(2, 0).fma(from_int(3, 0), Decimal64::ZERO, RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), from_int(6, 0).to_bits());
    }

    #[test]
    fn fma_zero_multiplicand() {
        let (r, _) =
            Decimal64::ZERO.fma(from_int(5, 0), from_int(7, 0), RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), from_int(7, 0).to_bits());
    }

    #[test]
    fn fma_zero_product_at_far_exponent_does_not_drop_c() {
        // Regression: when one product factor is zero AND the other
        // has a far exponent, the alignment-shift early-return used
        // to discard `c`. `1e50 × 0 + 1 = 1`, not `0`.
        let a = from_int(1, 50);
        let b = Decimal64::ZERO;
        let c = from_int(1, 0);
        let (r, _) = a.fma(b, c, RoundingMode::NearestEven);
        let one = from_int(1, 0);
        let (cmp, _) = r.partial_cmp(one);
        assert_eq!(
            cmp,
            Some(core::cmp::Ordering::Equal),
            "fma(1e50, 0, 1) = {r:?}, expected value 1"
        );
    }

    #[test]
    fn fma_far_exponent_with_small_product_does_not_drop_c() {
        // Regression: the previous *static* MAX_SHIFT = 6 made
        // `shift_ab > 6` route through the early-return, which
        // assumes ab dominates. But ab can be small (here ab =
        // 1 × 1 = 1, 1 digit) and c at the lower quantum can be
        // comparable, so neither dominates. The dynamic bound
        // `digit_count(ab_coef) + shift_ab ≤ 38` admits this case
        // through the normal align-and-sum path.
        //
        // fma(1, 1, 0.999999999999999) = 1.999999999999999
        let a = from_int(1, 0);
        let b = from_int(1, 0);
        let c = Decimal64::try_new(999_999_999_999_999, -15).unwrap();
        let (r, _) = a.fma(b, c, RoundingMode::NearestEven);
        let expected = Decimal64::try_new(1_999_999_999_999_999, -15).unwrap();
        let (cmp, _) = r.partial_cmp(expected);
        assert_eq!(
            cmp,
            Some(core::cmp::Ordering::Equal),
            "fma(1, 1, 0.999_999_999_999_999) = {r:?}, expected {expected:?}",
        );
    }

    #[test]
    fn fma_zero_c_at_far_exponent_does_not_drop_product() {
        // Regression: when c is a zero at a far quantum, the
        // alignment-shift early-return used to discard the product.
        // `1 × 1 + 0E+50 = 1`, not `0`.
        let a = from_int(1, 0);
        let b = from_int(1, 0);
        let c = Decimal64::try_new(0, 50).unwrap();
        let (r, _) = a.fma(b, c, RoundingMode::NearestEven);
        let one = from_int(1, 0);
        let (cmp, _) = r.partial_cmp(one);
        assert_eq!(
            cmp,
            Some(core::cmp::Ordering::Equal),
            "fma(1, 1, 0E+50) = {r:?}, expected value 1"
        );
    }

    #[test]
    fn fma_zero_times_infinity_invalid() {
        let (r, s) =
            Decimal64::ZERO.fma(Decimal64::INFINITY, Decimal64::ONE, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());
    }

    #[test]
    fn fma_infinity_minus_infinity_invalid() {
        let (r, s) = Decimal64::INFINITY.fma(
            Decimal64::ONE,
            Decimal64::NEG_INFINITY,
            RoundingMode::NearestEven,
        );
        assert!(r.is_quiet_nan());
        assert!(s.invalid());
    }

    #[test]
    fn fma_infinity_passes_through() {
        let (r, _) =
            Decimal64::INFINITY.fma(from_int(2, 0), from_int(3, 0), RoundingMode::NearestEven);
        assert!(r.is_infinite() && !r.is_sign_negative());
    }

    #[test]
    fn fma_nan_propagation() {
        let (r, s) =
            Decimal64::NAN.fma(Decimal64::ONE, Decimal64::ONE, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.is_ok());

        let (r, s) = Decimal64::SIGNALING_NAN.fma(
            Decimal64::ONE,
            Decimal64::ONE,
            RoundingMode::NearestEven,
        );
        assert!(r.is_quiet_nan());
        assert!(s.invalid());
    }

    #[test]
    fn fma_cancellation_zero_sign() {
        let (r, _) =
            from_int(1, 0).fma(from_int(1, 0), from_int(-1, 0), RoundingMode::NearestEven);
        assert!(r.is_zero() && !r.is_sign_negative());

        let (r, _) =
            from_int(1, 0).fma(from_int(1, 0), from_int(-1, 0), RoundingMode::TowardNegative);
        assert!(r.is_zero() && r.is_sign_negative());
    }
}
