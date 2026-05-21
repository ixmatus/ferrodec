//! General Decimal Arithmetic `divideInteger` — truncated integer
//! quotient with exponent zero.
//!
//! `divide_integer(x, y) = trunc(x / y)` as an integer; the
//! remainder of the division is what [`Decimal128::rem_trunc`]
//! returns. The two operations share the same multiword arithmetic
//! kernel shape: the operands are decoded, scaled to a common
//! quantum, and divided as integers via [`U256::div_rem_u128`]. This
//! module returns the quotient, `rem.rs` returns the remainder.
//!
//! Specifics that distinguish `divide_integer` from `rem_trunc`:
//!
//! * The result's exponent is always 0 (per the GDA spec). The
//!   integer quotient `q` is packed at `biased_exp = BIAS`.
//! * The result's sign is the exclusive-or of the operand signs;
//!   `divide_integer(-1, 4) = -0` (negative zero is preserved on
//!   toward-zero division).
//! * Division by zero raises `Division_by_zero` and returns
//!   ±Infinity with the appropriate sign, mirroring the IEEE 754
//!   `divide` rule (whereas `rem_trunc` raises `Invalid_operation`
//!   on the same case).
//! * `Division_impossible` (mapped to `INVALID`) is raised when the
//!   integer quotient would exceed `PRECISION` digits, exactly the
//!   same dynamic check `rem_finite` uses for the same kernel.
//!
//! See ADR-0031 for the lens relitigation admitting this and the
//! seven other GDA extension operations into the 1.x line.

use crate::bid::{classify_bits, pack_finite, Class, BIAS, PRECISION};
use crate::decimal::Decimal128;
use crate::multiword::U256;
use crate::ops::propagate_nan2;
use crate::status::Status;

impl Decimal128 {
    /// General Decimal Arithmetic `divideInteger(x, y)`.
    ///
    /// Returns the truncated integer quotient `trunc(x / y)`, packed
    /// at exponent 0. The sign of the result is the exclusive-or of
    /// the operand signs, so `divide_integer(-1, 4)` is `-0`.
    ///
    /// Special cases:
    /// * `divide_integer(±0, ±0)` → quiet NaN + `INVALID`.
    /// * `divide_integer(±x, ±0)` for finite non-zero `x` → ±Infinity
    ///   + `DIV_BY_ZERO`.
    /// * `divide_integer(±∞, ±∞)` → quiet NaN + `INVALID`.
    /// * `divide_integer(±∞, finite)` → ±Infinity.
    /// * `divide_integer(finite, ±∞)` → ±0 (sign by xor).
    /// * NaN propagation: a signaling NaN raises `INVALID` and
    ///   yields a quiet NaN with the same payload; quiet NaN passes
    ///   through.
    /// * `Division_impossible`: when the exact integer quotient
    ///   would require more than `PRECISION = 34` decimal digits,
    ///   the operation returns quiet NaN + `INVALID` (the
    ///   `Division_impossible` condition collapses to `INVALID`
    ///   per the ferrodec status surface).
    ///
    /// The operation never raises `INEXACT`; the discarded
    /// fractional part of `x / y` is what `rem_trunc` returns.
    ///
    /// # Examples
    ///
    /// ```
    /// use ferrodec::Decimal128;
    ///
    /// let x = Decimal128::try_new(7, 0).unwrap();
    /// let y = Decimal128::try_new(2, 0).unwrap();
    /// let (q, st) = x.divide_integer(y);
    /// assert!(st.is_ok());
    /// // trunc(7/2) = 3.
    /// assert_eq!(q.to_bits(), Decimal128::try_new(3, 0).unwrap().to_bits());
    /// ```
    #[must_use]
    pub fn divide_integer(self, rhs: Self) -> (Self, Status) {
        if let Some(out) = divide_integer_special_cases(self, rhs) {
            return out;
        }
        divide_integer_finite(self, rhs)
    }
}

/// Special-case dispatch for `divide_integer`. Returns `None` for the
/// finite-non-zero / finite-non-zero pair that the integer kernel
/// handles below.
fn divide_integer_special_cases(a: Decimal128, b: Decimal128) -> Option<(Decimal128, Status)> {
    let cls_a = classify_bits(a.to_bits());
    let cls_b = classify_bits(b.to_bits());

    let snan =
        matches!(cls_a, Class::SignalingNaN { .. }) || matches!(cls_b, Class::SignalingNaN { .. });
    let snan_status = if snan { Status::INVALID } else { Status::OK };

    if matches!(cls_a, Class::QuietNaN { .. } | Class::SignalingNaN { .. })
        || matches!(cls_b, Class::QuietNaN { .. } | Class::SignalingNaN { .. })
    {
        return Some((propagate_nan2(a, b), snan_status));
    }

    // ±∞ on the left handles inf/inf, inf/finite, AND inf/0 — the
    // GDA spec treats `Inf / 0` as the infinity-arithmetic rule
    // (signed infinity, no flag), not the division-by-zero rule.
    // Order matters: this precedes the b == 0 branch below.
    if let Class::Infinity { sign: sign_a } = cls_a {
        if matches!(cls_b, Class::Infinity { .. }) {
            return Some((Decimal128::NAN, Status::INVALID));
        }
        let sign_b = sign_of(cls_b);
        let result_sign = sign_a ^ sign_b;
        return Some((
            if result_sign {
                Decimal128::NEG_INFINITY
            } else {
                Decimal128::INFINITY
            },
            Status::OK,
        ));
    }

    // After the Inf-on-the-left case, the remaining b == 0 cases are
    // 0/0 (INVALID with NaN) and finite_nonzero/0 (DIV_BY_ZERO with
    // signed Infinity).
    if matches!(cls_b, Class::Zero { .. }) {
        if matches!(cls_a, Class::Zero { .. }) {
            return Some((Decimal128::NAN, Status::INVALID));
        }
        let sign_a = sign_of(cls_a);
        let sign_b = sign_of(cls_b);
        let result_sign = sign_a ^ sign_b;
        return Some((
            if result_sign {
                Decimal128::NEG_INFINITY
            } else {
                Decimal128::INFINITY
            },
            Status::DIV_BY_ZERO,
        ));
    }

    // finite / ±∞ → ±0 (signed by xor, exponent 0).
    if let Class::Infinity { sign: sign_b } = cls_b {
        let sign_a = sign_of(cls_a);
        let result_sign = sign_a ^ sign_b;
        return Some((
            Decimal128::from_bits(pack_finite(result_sign, BIAS, 0)),
            Status::OK,
        ));
    }

    // ±0 / finite_non-zero → ±0 (signed by xor, exponent 0).
    if let Class::Zero { sign: sign_a, .. } = cls_a {
        let sign_b = sign_of(cls_b);
        let result_sign = sign_a ^ sign_b;
        return Some((
            Decimal128::from_bits(pack_finite(result_sign, BIAS, 0)),
            Status::OK,
        ));
    }

    None
}

/// Extract the sign of a non-NaN classification.
fn sign_of(c: Class) -> bool {
    match c {
        Class::Zero { sign, .. }
        | Class::Finite { sign, .. }
        | Class::Infinity { sign }
        | Class::QuietNaN { sign, .. }
        | Class::SignalingNaN { sign, .. } => sign,
    }
}

/// Compute `trunc(a / b)` as an integer for two finite, non-zero
/// operands. Mirrors `rem_finite`'s decode + multiword arithmetic,
/// but returns the integer quotient packed at exponent 0 instead of
/// the remainder.
fn divide_integer_finite(a: Decimal128, b: Decimal128) -> (Decimal128, Status) {
    let cls_a = classify_bits(a.to_bits());
    let cls_b = classify_bits(b.to_bits());
    let (sx, qxb, cx) = match cls_a {
        Class::Finite {
            sign,
            biased_exp,
            coefficient,
        } => (sign, biased_exp, coefficient),
        _ => unreachable!("special cases handled by divide_integer_special_cases"),
    };
    let (sy, qyb, cy) = match cls_b {
        Class::Finite {
            sign,
            biased_exp,
            coefficient,
        } => (sign, biased_exp, coefficient),
        _ => unreachable!("special cases handled by divide_integer_special_cases"),
    };
    debug_assert!(cx != 0 && cy != 0);

    let qx = qxb as i32 - BIAS as i32;
    let qy = qyb as i32 - BIAS as i32;
    let result_sign = sx ^ sy;

    // Align both coefficients to the same quantum, the minimum of qx
    // and qy. The integer quotient `q = floor(|a| / |b|)` of the
    // scaled integers equals `trunc(a/b)` since both are positive.
    let q_min = qx.min(qy);
    let dq_x = (qx - q_min) as u32;
    let dq_y = (qy - q_min) as u32;

    let cx_digits = crate::bid::decimal_digit_count(cx) + dq_x;
    let cy_digits = crate::bid::decimal_digit_count(cy) + dq_y;

    // Case 1: |y| ≫ |x|. The integer quotient is 0; return ±0 at
    // exponent 0 with the xor-of-signs sign. `cy_digits > 38` means
    // `y_scaled` overflows u128, but more importantly the quotient
    // is zero in every such case.
    if cy_digits > cx_digits {
        return (
            Decimal128::from_bits(pack_finite(result_sign, BIAS, 0)),
            Status::OK,
        );
    }
    // Case 2: aligned numerator overflows U256. The integer quotient
    // is guaranteed to exceed `PRECISION` digits because
    // `n_digits ≥ cx_digits − cy_digits ≥ 75 − 38 = 37 > 34`. Map
    // straight to Division_impossible.
    if cx_digits > 75 {
        return (Decimal128::NAN, Status::INVALID);
    }

    let y_scaled: u128 = cy * 10u128.pow(dq_y);
    let x_scaled = U256::from_u128(cx).mul_pow10(dq_x);
    let (q, _r) = x_scaled.div_rem_u128(y_scaled);

    // Division_impossible per the GDA spec: integer quotient exceeds
    // PRECISION digits.
    if q.decimal_digit_count() > PRECISION {
        return (Decimal128::NAN, Status::INVALID);
    }

    // q fits in u128 because its digit count is at most PRECISION
    // (= 34), well below 10^38.
    let q_u128 = q.lo;
    debug_assert!(q.hi == 0);
    (
        Decimal128::from_bits(pack_finite(result_sign, BIAS, q_u128)),
        Status::OK,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(c: i128, e: i32) -> Decimal128 {
        Decimal128::try_new(c, e).unwrap()
    }

    #[test]
    fn seven_div_two_is_three() {
        let (q, st) = d(7, 0).divide_integer(d(2, 0));
        assert!(st.is_ok());
        assert_eq!(q.to_bits(), d(3, 0).to_bits());
    }

    #[test]
    fn negative_one_div_four_is_negative_zero() {
        // toward-zero division: -1 / 4 = -0.25, trunc = -0 (sign
        // preserved on negative zero).
        let (q, st) = d(-1, 0).divide_integer(d(4, 0));
        assert!(st.is_ok());
        assert!(q.is_zero());
        assert!(q.is_sign_negative());
    }

    #[test]
    fn five_div_two_tenths_is_twenty_five() {
        // 5 / 0.2 = 25 exactly.
        let (q, st) = d(5, 0).divide_integer(d(2, -1));
        assert!(st.is_ok());
        assert_eq!(q.to_bits(), d(25, 0).to_bits());
    }

    #[test]
    fn nonzero_div_zero_raises_div_by_zero_with_signed_infinity() {
        let (q, st) = d(1, 0).divide_integer(d(0, 0));
        assert_eq!(st, Status::DIV_BY_ZERO);
        assert_eq!(q.to_bits(), Decimal128::INFINITY.to_bits());
        let (q_neg, st_neg) = d(-1, 0).divide_integer(d(0, 0));
        assert_eq!(st_neg, Status::DIV_BY_ZERO);
        assert_eq!(q_neg.to_bits(), Decimal128::NEG_INFINITY.to_bits());
    }

    #[test]
    fn zero_div_zero_raises_invalid() {
        let (q, st) = d(0, 0).divide_integer(d(0, 0));
        assert_eq!(st, Status::INVALID);
        assert!(q.is_nan());
    }

    #[test]
    fn inf_div_inf_raises_invalid() {
        let (q, st) = Decimal128::INFINITY.divide_integer(Decimal128::INFINITY);
        assert_eq!(st, Status::INVALID);
        assert!(q.is_nan());
    }

    #[test]
    fn inf_div_finite_is_signed_inf() {
        let (q, st) = Decimal128::INFINITY.divide_integer(d(2, 0));
        assert!(st.is_ok());
        assert_eq!(q.to_bits(), Decimal128::INFINITY.to_bits());
        let (q, _) = Decimal128::NEG_INFINITY.divide_integer(d(2, 0));
        assert_eq!(q.to_bits(), Decimal128::NEG_INFINITY.to_bits());
    }

    #[test]
    fn finite_div_inf_is_signed_zero() {
        let (q, st) = d(2, 0).divide_integer(Decimal128::INFINITY);
        assert!(st.is_ok());
        assert!(q.is_zero());
        assert!(!q.is_sign_negative());
        let (q, _) = d(2, 0).divide_integer(Decimal128::NEG_INFINITY);
        assert!(q.is_zero());
        assert!(q.is_sign_negative());
    }

    #[test]
    fn zero_div_finite_is_signed_zero() {
        let (q, st) = d(0, 0).divide_integer(d(2, 0));
        assert!(st.is_ok());
        assert!(q.is_zero());
        assert!(!q.is_sign_negative());
        let (q, _) = d(0, 0).divide_integer(d(-2, 0));
        assert!(q.is_zero());
        assert!(q.is_sign_negative());
    }

    #[test]
    fn division_impossible_when_quotient_exceeds_precision() {
        // 10^34 / 1 = 10^34, which has 35 digits > 34. Division_impossible.
        let big = d(1, 34);
        let (q, st) = big.divide_integer(d(1, 0));
        assert_eq!(st, Status::INVALID);
        assert!(q.is_nan());
    }

    #[test]
    fn signaling_nan_raises_invalid_and_quiets() {
        let (q, st) = Decimal128::SIGNALING_NAN.divide_integer(d(2, 0));
        assert_eq!(st, Status::INVALID);
        assert!(q.is_nan());
        assert!(!q.is_signaling_nan());
    }

    #[test]
    fn quiet_nan_propagates() {
        let (q, st) = Decimal128::NAN.divide_integer(d(2, 0));
        assert!(st.is_ok());
        assert!(q.is_nan());
    }
}
