//! IEEE 754 division for [`Decimal128`].
//!
//! Special cases follow IEEE 754-2019 §7:
//!
//! * `NaN / x` and `x / NaN` propagate NaN; signaling-NaN inputs raise `INVALID`.
//! * `0 / 0` and `∞ / ∞` are *invalid*: NaN + `INVALID`.
//! * `finite_non_zero / 0` is `±∞` with `DIV_BY_ZERO` raised.
//! * `∞ / 0` is `±∞` (no `DIV_BY_ZERO` — the infinity already encodes it).
//! * `0 / finite_non_zero` is `±0`.
//! * `∞ / finite` is `±∞`.
//! * `finite / ∞` is `±0`.
//! * Sign in every case follows `sign(a) ⊕ sign(b)`.
//!
//! Finite-finite path: scale the numerator by `10^k` so the integer
//! quotient has `PRECISION + 1` decimal digits (35 for Decimal128),
//! then divide via [`U256::div_rem_u128`]. The remainder threads through
//! to [`round_and_pack_finite`] as the pre-sticky bit, so the rounding
//! direction is correct for every IEEE rounding mode.
//!
//! `k` lies in `[2, 68]` for any operands with `1 ≤ digits ≤ 34`, so
//! the scaled numerator stays under `10^69 ≈ 2^229` (within U256) and
//! the quotient stays under `10^37 ≈ 2^123` (within u128).

use crate::bid::{
    classify_bits, decimal_digit_count, pack_finite, pack_infinity, Class, BIAS, PRECISION,
};
use crate::decimal::Decimal128;
use crate::multiword::U256;
use crate::ops::{propagate_nan2, round_and_pack_finite};
use crate::status::{RoundingMode, Status};

impl Decimal128 {
    /// IEEE 754 `division(self, rhs)`.
    ///
    /// Returns `(self / rhs, status)` rounded according to `rm`.
    #[must_use]
    pub fn div(self, rhs: Self, rm: RoundingMode) -> (Self, Status) {
        if let Some(early) = div_special_cases(self, rhs) {
            return early;
        }
        div_finite_finite(self, rhs, rm)
    }

    /// Kani-only entry point for the special-case path.
    #[cfg(kani)]
    #[doc(hidden)]
    #[must_use]
    pub fn div_special_only_for_kani(self, rhs: Self) -> Option<(Self, Status)> {
        div_special_cases(self, rhs)
    }
}

#[inline]
fn div_special_cases(a: Decimal128, b: Decimal128) -> Option<(Decimal128, Status)> {
    let cls_a = classify_bits(a.to_bits());
    let cls_b = classify_bits(b.to_bits());

    let snan =
        matches!(cls_a, Class::SignalingNaN { .. }) || matches!(cls_b, Class::SignalingNaN { .. });
    let status = if snan { Status::INVALID } else { Status::OK };

    if matches!(cls_a, Class::QuietNaN { .. } | Class::SignalingNaN { .. })
        || matches!(cls_b, Class::QuietNaN { .. } | Class::SignalingNaN { .. })
    {
        return Some((propagate_nan2(a, b), status));
    }

    let sign_a = sign_of(cls_a);
    let sign_b = sign_of(cls_b);
    let result_sign = sign_a ^ sign_b;

    let zero_a = matches!(cls_a, Class::Zero { .. });
    let zero_b = matches!(cls_b, Class::Zero { .. });
    let inf_a = matches!(cls_a, Class::Infinity { .. });
    let inf_b = matches!(cls_b, Class::Infinity { .. });

    // 0 / 0 — invalid.
    if zero_a && zero_b {
        return Some((Decimal128::NAN, status | Status::INVALID));
    }
    // ∞ / ∞ — invalid.
    if inf_a && inf_b {
        return Some((Decimal128::NAN, status | Status::INVALID));
    }
    // ±∞ / 0 = ±∞ (no DIV_BY_ZERO — the infinity is genuine, not produced by the division).
    if inf_a && zero_b {
        return Some((Decimal128::from_bits(pack_infinity(result_sign)), status));
    }
    // finite_non_zero / 0 = ±∞ + DIV_BY_ZERO.
    if zero_b {
        return Some((
            Decimal128::from_bits(pack_infinity(result_sign)),
            status | Status::DIV_BY_ZERO,
        ));
    }
    // ∞ / finite_non_zero = ±∞.
    if inf_a {
        return Some((Decimal128::from_bits(pack_infinity(result_sign)), status));
    }
    // x / ∞ → ±0. Per IEEE 754 / dec-spec the preferred quantum is
    // `qa − q_inf`, but ∞ has no quantum so the spec clamps to the
    // smallest representable quantum (`Q_MIN`, biased 0). We emit
    // that directly.
    if inf_b {
        return Some((
            Decimal128::from_bits(pack_finite(result_sign, 0, 0)),
            status,
        ));
    }
    // 0 / finite_non_zero: result is ±0 with preferred quantum `qa − qb`.
    if zero_a {
        let (_, ea, _) = decompose_finite(cls_a);
        let (_, eb, _) = decompose_finite(cls_b);
        let q = (ea as i32 - eb as i32) + BIAS as i32;
        let biased = q.clamp(0, crate::bid::BIASED_EXP_MAX as i32) as u32;
        return Some((
            Decimal128::from_bits(pack_finite(result_sign, biased, 0)),
            status,
        ));
    }

    None
}

fn div_finite_finite(a: Decimal128, b: Decimal128, rm: RoundingMode) -> (Decimal128, Status) {
    let cls_a = classify_bits(a.to_bits());
    let cls_b = classify_bits(b.to_bits());
    let (sa, ea, ca) = decompose_finite(cls_a);
    let (sb, eb, cb) = decompose_finite(cls_b);
    debug_assert!(ca != 0 && cb != 0);

    let sign = sa ^ sb;

    let digits_a = decimal_digit_count(ca);
    let digits_b = decimal_digit_count(cb);

    // Scale the numerator so the integer quotient has at least PRECISION+1
    // decimal digits — round_and_pack_finite then drops the excess and
    // rounds correctly.
    //
    // For valid Decimal128 coefficients (1 ≤ digits ≤ 34) this gives
    // k ∈ [2, 68], keeping `scaled_num < 10^69` (fits in U256) and the
    // quotient under `10^37` (fits in u128).
    let k = (PRECISION as i32 + 1) + (digits_b as i32) - (digits_a as i32);
    let k_u32 = k.max(0) as u32;

    let scaled_num = U256::from_u128(ca).mul_pow10(k_u32);
    let (quotient, remainder) = scaled_num.div_rem_u128(cb);

    // The unbiased quantum exponent of `quotient × 10^(...)` is the difference
    // of input quanta minus the artificial scale we applied to the numerator.
    let unbiased_exp = (ea as i32 - BIAS as i32) - (eb as i32 - BIAS as i32) - k_u32 as i32;

    let pre_sticky = remainder != 0;

    // IEEE 754 §6.3 preferred quantum for div is `qa - qb` (when exact).
    let q_preferred = (ea as i32 - BIAS as i32) - (eb as i32 - BIAS as i32);
    round_and_pack_finite(
        quotient,
        unbiased_exp,
        q_preferred,
        sign,
        pre_sticky,
        rm,
        Status::OK,
    )
}

fn sign_of(c: Class) -> bool {
    match c {
        Class::Zero { sign, .. }
        | Class::Finite { sign, .. }
        | Class::Infinity { sign }
        | Class::QuietNaN { sign, .. }
        | Class::SignalingNaN { sign, .. } => sign,
    }
}

fn decompose_finite(c: Class) -> (bool, u32, u128) {
    match c {
        Class::Zero { sign, biased_exp } => (sign, biased_exp, 0),
        Class::Finite {
            sign,
            biased_exp,
            coefficient,
        } => (sign, biased_exp, coefficient),
        _ => {
            debug_assert!(false, "decompose_finite on non-finite Class");
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
        let (r, s) = Decimal128::ONE.div(Decimal128::NAN, RoundingMode::default());
        assert!(r.is_nan());
        assert!(s.is_ok());

        let (r, s) = Decimal128::SIGNALING_NAN.div(Decimal128::ONE, RoundingMode::default());
        assert!(r.is_nan());
        assert!(s.invalid());
    }

    #[test]
    fn zero_over_zero_is_invalid_nan() {
        let (r, s) = Decimal128::ZERO.div(Decimal128::ZERO, RoundingMode::default());
        assert!(r.is_nan());
        assert!(s.invalid());
        let (r, s) = Decimal128::NEG_ZERO.div(Decimal128::ZERO, RoundingMode::default());
        assert!(r.is_nan());
        assert!(s.invalid());
    }

    #[test]
    fn inf_over_inf_is_invalid_nan() {
        let (r, s) = Decimal128::INFINITY.div(Decimal128::NEG_INFINITY, RoundingMode::default());
        assert!(r.is_nan());
        assert!(s.invalid());
    }

    #[test]
    fn finite_over_zero_is_div_by_zero() {
        let (r, s) = Decimal128::ONE.div(Decimal128::ZERO, RoundingMode::default());
        assert!(r.is_infinite());
        assert!(!r.is_sign_negative());
        assert!(s.div_by_zero());

        let (r, s) = Decimal128::NEG_ONE.div(Decimal128::ZERO, RoundingMode::default());
        assert!(r.is_infinite());
        assert!(r.is_sign_negative());
        assert!(s.div_by_zero());

        let (r, s) = Decimal128::ONE.div(Decimal128::NEG_ZERO, RoundingMode::default());
        assert!(r.is_infinite());
        assert!(r.is_sign_negative());
        assert!(s.div_by_zero());
    }

    #[test]
    fn inf_over_zero_no_div_by_zero() {
        let (r, s) = Decimal128::INFINITY.div(Decimal128::ZERO, RoundingMode::default());
        assert!(r.is_infinite());
        assert!(!s.div_by_zero());
    }

    #[test]
    fn zero_over_finite_is_signed_zero() {
        let (r, s) = Decimal128::ZERO.div(Decimal128::ONE, RoundingMode::default());
        assert!(r.is_zero());
        assert!(!r.is_sign_negative());
        assert!(s.is_ok());

        let (r, _) = Decimal128::ZERO.div(Decimal128::NEG_ONE, RoundingMode::default());
        assert!(r.is_zero());
        assert!(r.is_sign_negative());

        let (r, _) = Decimal128::NEG_ZERO.div(Decimal128::ONE, RoundingMode::default());
        assert!(r.is_zero());
        assert!(r.is_sign_negative());
    }

    #[test]
    fn inf_over_finite_is_inf() {
        let (r, _) = Decimal128::INFINITY.div(Decimal128::TEN, RoundingMode::default());
        assert!(r.is_infinite());
        assert!(!r.is_sign_negative());

        let (r, _) = Decimal128::NEG_INFINITY.div(Decimal128::TEN, RoundingMode::default());
        assert!(r.is_infinite());
        assert!(r.is_sign_negative());
    }

    #[test]
    fn finite_over_inf_is_zero() {
        let (r, _) = Decimal128::ONE.div(Decimal128::INFINITY, RoundingMode::default());
        assert!(r.is_zero());
        assert!(!r.is_sign_negative());

        let (r, _) = Decimal128::NEG_ONE.div(Decimal128::INFINITY, RoundingMode::default());
        assert!(r.is_zero());
        assert!(r.is_sign_negative());
    }

    #[test]
    fn finite_basic_quotients() {
        // 6 / 2 = 3
        let (r, _) = d_int(6).div(d_int(2), RoundingMode::default());
        let (cmp, _) = r.partial_cmp(d_int(3));
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));

        // 1 / 4 = 0.25
        let (r, _) = Decimal128::ONE.div(d_int(4), RoundingMode::default());
        let expected = d_finite(false, BIAS - 2, 25); // 0.25 = 25 × 10^-2
        let (cmp, _) = r.partial_cmp(expected);
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));

        // 100 / 4 = 25
        let (r, _) = d_int(100).div(d_int(4), RoundingMode::default());
        let (cmp, _) = r.partial_cmp(d_int(25));
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));
    }

    #[test]
    fn one_over_three_repeats_inexact() {
        let (r, s) = Decimal128::ONE.div(d_int(3), RoundingMode::NearestEven);
        // 1/3 in 34-digit decimal = 0.333...3 (with last digit rounded).
        // Just sanity-check it's positive, finite, INEXACT.
        assert!(r.is_finite());
        assert!(!r.is_zero());
        assert!(s.inexact());
    }

    #[test]
    fn div_signs_all_combinations() {
        for &a_sign in &[false, true] {
            for &b_sign in &[false, true] {
                let a = d_finite(a_sign, BIAS, 12);
                let b = d_finite(b_sign, BIAS, 4);
                let (r, _) = a.div(b, RoundingMode::default());
                assert_eq!(
                    r.is_sign_negative(),
                    a_sign ^ b_sign,
                    "sign of {a:?} / {b:?}"
                );
            }
        }
    }

    #[test]
    fn div_by_one_is_identity_numerically() {
        for &v in &[1i128, -1, 7, -42, 1_000_000, -10_000] {
            let x = d_int(v);
            let (r, _) = x.div(Decimal128::ONE, RoundingMode::default());
            let (cmp, _) = r.partial_cmp(x);
            assert_eq!(cmp, Some(core::cmp::Ordering::Equal), "{v} / 1");
        }
    }

    #[test]
    fn div_self_is_one_for_finite_nonzero() {
        for &v in &[1i128, 7, -42, 1_000_000] {
            let x = d_int(v);
            let (r, _) = x.div(x, RoundingMode::default());
            let (cmp, _) = r.partial_cmp(Decimal128::ONE);
            assert_eq!(cmp, Some(core::cmp::Ordering::Equal), "{v} / {v}");
        }
    }

    #[test]
    fn one_over_ten_is_tenth() {
        let (r, _) = Decimal128::ONE.div(Decimal128::TEN, RoundingMode::default());
        let expected = d_finite(false, BIAS - 1, 1); // 0.1 = 1 × 10^-1
        let (cmp, _) = r.partial_cmp(expected);
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));
    }
}
