//! IEEE 754 remainder for [`Decimal128`].
//!
//! `remainder(x, y) = x − n × y` where `n` is the nearest-even integer
//! to `x / y`. The IEEE definition is *exact* — there is no rounding,
//! and `INEXACT` is never raised.
//!
//! Special cases (IEEE 754-2019 §5.3.1):
//!
//! * NaN propagation; sNaN → `INVALID`.
//! * `x / 0` (finite `x`) → NaN + `INVALID`.
//! * `±∞ / y` → NaN + `INVALID`.
//! * `x / ±∞` (finite `x`) → `x` (the result equals `x` exactly).
//! * `x = ±0` → `±0` with the sign of `x`.
//!
//! ## v1 scope
//!
//! The finite-finite kernel aligns both operands to a common quantum
//! `q_min = min(q_x, q_y)`. If the aligned numerator and denominator
//! both fit in the working envelope (numerator in U256, denominator in
//! `u128`), we do an exact `div_rem_u128` and round the quotient
//! to nearest-even, returning `x − n·y` packed at `q_min`.
//!
//! Two cases are deferred to a follow-up:
//!
//! 1. `q_y − q_x` so large that the aligned divisor overflows `u128`
//!    (≥ 39 decimal digits). In this case `|y| ≫ |x|`, so `n = 0` and
//!    the remainder is exactly `x` — we return `self` directly.
//!    Mathematically correct.
//! 2. `q_x − q_y` so large that the aligned numerator overflows U256
//!    (≥ 76 decimal digits). This is the truly unsupported case;
//!    proper handling requires modular exponentiation
//!    (`10^Δ mod y` via repeated squaring) to fold the alignment
//!    into the divisor's residue. Tracked as a TODO. The current
//!    fallback returns NaN with `INVALID` raised so the limitation
//!    surfaces loudly.

use crate::bid::{
    classify_bits, decimal_digit_count, pack_finite, Class, BIAS, BIASED_EXP_MAX,
};
use crate::decimal::Decimal128;
use crate::multiword::U256;
use crate::status::Status;

impl Decimal128 {
    /// IEEE 754 `remainder(self, rhs)`.
    ///
    /// Always exact when defined. See module docs for the v1 envelope
    /// and the deferred case.
    #[must_use]
    pub fn rem(self, rhs: Self) -> (Self, Status) {
        if let Some(early) = rem_special_cases(self, rhs) {
            return early;
        }
        rem_finite(self, rhs)
    }

    /// Kani-only entry point for the special-case path.
    #[cfg(kani)]
    #[doc(hidden)]
    #[must_use]
    pub fn rem_special_only_for_kani(self, rhs: Self) -> Option<(Self, Status)> {
        rem_special_cases(self, rhs)
    }
}

#[inline]
fn rem_special_cases(a: Decimal128, b: Decimal128) -> Option<(Decimal128, Status)> {
    let cls_a = classify_bits(a.to_bits());
    let cls_b = classify_bits(b.to_bits());

    let snan = matches!(cls_a, Class::SignalingNaN { .. })
        || matches!(cls_b, Class::SignalingNaN { .. });
    let status = if snan { Status::INVALID } else { Status::OK };

    if matches!(
        cls_a,
        Class::QuietNaN { .. } | Class::SignalingNaN { .. }
    ) || matches!(
        cls_b,
        Class::QuietNaN { .. } | Class::SignalingNaN { .. }
    ) {
        return Some((Decimal128::NAN, status));
    }

    // x / 0 — invalid.
    if matches!(cls_b, Class::Zero { .. }) {
        return Some((Decimal128::NAN, status | Status::INVALID));
    }
    // ±∞ / y — invalid.
    if matches!(cls_a, Class::Infinity { .. }) {
        return Some((Decimal128::NAN, status | Status::INVALID));
    }
    // x / ±∞ = x (preserve cohort).
    if matches!(cls_b, Class::Infinity { .. }) {
        return Some((a, status));
    }
    // ±0 / y_finite_nonzero = ±0 with sign of x. Preferred quantum
    // per dec spec is `min(qx, qy)`.
    if let Class::Zero { sign, biased_exp } = cls_a {
        let qy = match cls_b {
            Class::Zero { biased_exp, .. } | Class::Finite { biased_exp, .. } => biased_exp,
            _ => biased_exp,
        };
        let q = biased_exp.min(qy);
        return Some((
            Decimal128::from_bits(pack_finite(sign, q, 0)),
            status,
        ));
    }

    None
}

fn rem_finite(a: Decimal128, b: Decimal128) -> (Decimal128, Status) {
    let cls_a = classify_bits(a.to_bits());
    let cls_b = classify_bits(b.to_bits());
    let (sx, qxb, cx) = decompose_finite(cls_a);
    let (_sy, qyb, cy) = decompose_finite(cls_b);
    debug_assert!(cx != 0 && cy != 0);

    let qx = qxb as i32 - BIAS as i32;
    let qy = qyb as i32 - BIAS as i32;

    let q_min = qx.min(qy);
    let dq_x = (qx - q_min) as u32;
    let dq_y = (qy - q_min) as u32;

    let cx_digits = decimal_digit_count(cx) + dq_x;
    let cy_digits = decimal_digit_count(cy) + dq_y;

    // Case 1: |y| ≫ |x| — divisor too wide for u128. Exact answer is `x`.
    if cy_digits > 38 {
        return (a, Status::OK);
    }
    // Case 2: |x| so much wider than |y| that aligned numerator overflows
    // U256. Tracked as a v1 follow-up — see module docs.
    if cx_digits > 75 {
        return (Decimal128::NAN, Status::INVALID);
    }

    let y_scaled: u128 = cy * 10u128.pow(dq_y);
    let x_scaled = U256::from_u128(cx).mul_pow10(dq_x);

    let (q, r) = x_scaled.div_rem_u128(y_scaled);

    // dec-spec "Division_impossible": if the integer quotient would
    // exceed PRECISION digits, the remainder operation is undefined
    // and we return NaN+INVALID. This matters for cases like
    // `remaindernear (10^33) 0.1` where the integer quotient is 10^34.
    if q.decimal_digit_count() > crate::bid::PRECISION {
        return (Decimal128::NAN, Status::INVALID);
    }

    // Round-to-nearest-even adjustment of the integer quotient.
    let n_lsb = (q.lo & 1) as u32;
    let round_up = compare_remainder_to_half(r, y_scaled, n_lsb);

    let (result_mag, sign_flip) = if round_up {
        (y_scaled - r, true)
    } else {
        (r, false)
    };

    if result_mag == 0 {
        // Exact zero remainder; sign is sign(x).
        let biased = clamp_biased(q_min);
        return (
            Decimal128::from_bits(pack_finite(sx, biased, 0)),
            Status::OK,
        );
    }

    let result_sign = sx ^ sign_flip;

    // Re-encode at quantum q_min, normalising trailing zeros if the
    // magnitude has more than `PRECISION` digits. The IEEE remainder is
    // always exactly representable when the operands are, so any
    // overflow above 34 digits must be made up of trailing zeros — we
    // shift right (dividing by 10) and increment the quantum until the
    // coefficient fits.
    let (mut coef, mut q_unbiased) = (result_mag, q_min);
    while decimal_digit_count(coef) > 34 {
        debug_assert!(coef % 10 == 0, "rem result not exactly representable");
        coef /= 10;
        q_unbiased += 1;
    }
    let biased = clamp_biased(q_unbiased);
    (
        Decimal128::from_bits(pack_finite(result_sign, biased, coef)),
        Status::OK,
    )
}

/// `r` vs `y_scaled / 2`, with tie-breaking by `n_lsb` (round-to-even).
/// Returns `true` if we should round the integer quotient up.
fn compare_remainder_to_half(r: u128, y_scaled: u128, n_lsb: u32) -> bool {
    if r == 0 {
        return false;
    }
    let (two_r, overflow) = r.overflowing_mul(2);
    if overflow {
        // 2r > u128::MAX ≥ y_scaled, so r > y_scaled / 2.
        return true;
    }
    match two_r.cmp(&y_scaled) {
        core::cmp::Ordering::Less => false,
        core::cmp::Ordering::Greater => true,
        core::cmp::Ordering::Equal => n_lsb == 1,
    }
}

fn clamp_biased(unbiased: i32) -> u32 {
    let biased = unbiased + BIAS as i32;
    biased.clamp(0, BIASED_EXP_MAX as i32) as u32
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
        let (r, _) = Decimal128::ONE.rem(Decimal128::NAN);
        assert!(r.is_nan());
        let (r, s) = Decimal128::SIGNALING_NAN.rem(Decimal128::ONE);
        assert!(r.is_nan());
        assert!(s.invalid());
    }

    #[test]
    fn x_over_zero_is_invalid_nan() {
        let (r, s) = Decimal128::ONE.rem(Decimal128::ZERO);
        assert!(r.is_nan());
        assert!(s.invalid());
    }

    #[test]
    fn inf_over_y_is_invalid_nan() {
        let (r, s) = Decimal128::INFINITY.rem(Decimal128::ONE);
        assert!(r.is_nan());
        assert!(s.invalid());
    }

    #[test]
    fn x_over_inf_is_x() {
        let (r, _) = d_int(7).rem(Decimal128::INFINITY);
        assert_eq!(r.to_bits(), d_int(7).to_bits());
        let (r, _) = Decimal128::NEG_ZERO.rem(Decimal128::INFINITY);
        assert!(r.is_zero());
        assert!(r.is_sign_negative());
    }

    #[test]
    fn rem_basic_in_range() {
        // 7 mod 3: 7 = 2*3 + 1, |1| < 1.5, n=2, result = 1
        let (r, s) = d_int(7).rem(d_int(3));
        let (cmp, _) = r.partial_cmp(d_int(1));
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));
        assert!(s.is_ok());

        // 8 mod 3: 8 = 2*3 + 2, |2| > 1.5, n=3, result = -1
        let (r, _) = d_int(8).rem(d_int(3));
        let (cmp, _) = r.partial_cmp(d_int(-1));
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));

        // -7 mod 3: result = -1
        let (r, _) = d_int(-7).rem(d_int(3));
        let (cmp, _) = r.partial_cmp(d_int(-1));
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));

        // -8 mod 3: result = +1
        let (r, _) = d_int(-8).rem(d_int(3));
        let (cmp, _) = r.partial_cmp(d_int(1));
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));
    }

    #[test]
    fn rem_round_to_even_tie() {
        // 5 mod 2: q=2 r=1, 2r=2=y_scaled tie. q parity = even, round down.
        // result = 1.
        let (r, _) = d_int(5).rem(d_int(2));
        let (cmp, _) = r.partial_cmp(d_int(1));
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));

        // 7 mod 2: q=3 r=1, tie. q parity = odd, round up.
        // result = -1.
        let (r, _) = d_int(7).rem(d_int(2));
        let (cmp, _) = r.partial_cmp(d_int(-1));
        assert_eq!(cmp, Some(core::cmp::Ordering::Equal));
    }

    #[test]
    fn rem_zero_when_divisible() {
        let (r, _) = d_int(12).rem(d_int(4));
        assert!(r.is_zero());
        let (r, _) = d_int(-12).rem(d_int(4));
        assert!(r.is_zero());
    }

    #[test]
    fn rem_y_much_larger_returns_x() {
        // x = 1, y = 1 × 10^100 (way larger). |x|/|y| → 0. n = 0.
        // Result = x.
        let huge_y = d_finite(false, BIAS + 100, 1);
        let (r, _) = Decimal128::ONE.rem(huge_y);
        assert_eq!(r.to_bits(), Decimal128::ONE.to_bits());
    }

    #[test]
    fn rem_zero_dividend_preserves_sign() {
        let (r, _) = Decimal128::ZERO.rem(d_int(7));
        assert!(r.is_zero());
        assert!(!r.is_sign_negative());

        let (r, _) = Decimal128::NEG_ZERO.rem(d_int(7));
        assert!(r.is_zero());
        assert!(r.is_sign_negative());
    }
}
