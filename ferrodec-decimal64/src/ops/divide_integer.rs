//! General Decimal Arithmetic `divideInteger` — truncated integer
//! quotient with exponent zero.
//!
//! `Decimal64` counterpart to the parent
//! [`crate::Decimal64::divide_integer`]. Same algorithmic shape as
//! the truncating remainder kernel in this module's sibling
//! `rem.rs`, with the integer quotient (rather than the remainder)
//! returned. The working width is `u128`: aligning a 16-digit
//! coefficient by up to 22 decimal positions stays within 38 digits,
//! the largest count that fits in `u128`. See ADR-0031 for the GDA
//! lens relitigation.

use crate::bid::{
    classify_bits, decimal_digit_count, pack_finite, pack_quiet_nan, BiasedExp, Class, Coefficient,
    BIAS, COEFFICIENT_LIMIT,
};
use crate::decimal::Decimal64;
use ferrodec_ieee::Status;

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

const U128_DIGIT_CAP: u32 = 38;

impl Decimal64 {
    /// General Decimal Arithmetic `divideInteger(x, y)`.
    ///
    /// Returns `trunc(x / y)` as an integer at exponent 0. The sign
    /// of the result is the exclusive-or of the operand signs;
    /// `divide_integer(-1, 4)` is `-0`.
    ///
    /// Special cases mirror the GDA spec:
    /// * `divide_integer(±0, ±0)` → quiet NaN + `INVALID`.
    /// * `divide_integer(finite_nonzero, ±0)` → ±Infinity +
    ///   `DIV_BY_ZERO`.
    /// * `divide_integer(±∞, ±∞)` → quiet NaN + `INVALID`.
    /// * `divide_integer(±∞, finite_or_zero)` → ±Infinity (signed
    ///   by xor; no flag — the infinity-arithmetic rule, not the
    ///   division-by-zero rule).
    /// * `divide_integer(finite, ±∞)` → ±0 (signed by xor, exponent 0).
    /// * `divide_integer(±0, finite_nonzero)` → ±0 (signed by xor,
    ///   exponent 0).
    /// * NaN propagation: signaling NaN raises `INVALID` and yields
    ///   a quiet NaN with the same payload; quiet NaN passes
    ///   through.
    /// * `Division_impossible`: when the exact integer quotient
    ///   would require more than `PRECISION = 16` decimal digits,
    ///   returns quiet NaN + `INVALID`.
    ///
    /// Never raises `INEXACT`.
    ///
    /// # Examples
    ///
    /// ```
    /// use ferrodec_decimal64::Decimal64;
    ///
    /// let x = Decimal64::try_new(7, 0).unwrap();
    /// let y = Decimal64::try_new(2, 0).unwrap();
    /// let (q, st) = x.divide_integer(y);
    /// assert!(st.is_ok());
    /// assert_eq!(q.to_bits(), Decimal64::try_new(3, 0).unwrap().to_bits());
    /// ```
    #[must_use]
    pub fn divide_integer(self, rhs: Self) -> (Self, Status) {
        let ca = classify_bits(self.0);
        let cb = classify_bits(rhs.0);

        if let Some(out) = divide_integer_special_cases(ca, cb) {
            return out;
        }

        // Both are finite-or-zero; the special cases handler covers
        // every "denominator is zero" path so reaching here implies
        // `cb` is a non-zero finite. The numerator may still be zero,
        // but that was handled by the special cases too — only
        // finite-non-zero pairs reach here.
        let (sign_a, biased_a, coef_a) = match ca {
            Class::Finite {
                sign,
                biased_exp,
                coefficient,
            } => (sign, biased_exp, coefficient),
            _ => unreachable!("special cases handled by divide_integer_special_cases"),
        };
        let (sign_b, biased_b, coef_b) = match cb {
            Class::Finite {
                sign,
                biased_exp,
                coefficient,
            } => (sign, biased_exp, coefficient),
            _ => unreachable!("special cases handled by divide_integer_special_cases"),
        };
        debug_assert!(coef_a != 0 && coef_b != 0);

        let result_sign = sign_a ^ sign_b;
        let exp_a = biased_a as i32 - BIAS as i32;
        let exp_b = biased_b as i32 - BIAS as i32;
        let target_q = exp_a.min(exp_b);

        let shift_a = (exp_a - target_q) as u32;
        let shift_b = (exp_b - target_q) as u32;

        let d_a = decimal_digit_count(coef_a);
        let d_b = decimal_digit_count(coef_b);
        let ab_safe_shift = U128_DIGIT_CAP - d_a;
        let bb_safe_shift = U128_DIGIT_CAP - d_b;

        // Numerator overflows u128 after alignment. By the same bound
        // argument rem.rs uses (D_a + shift_a − D_b ≥ 22 > PRECISION =
        // 16), the integer quotient exceeds PRECISION digits, which
        // is Division_impossible.
        if shift_a > ab_safe_shift {
            return (Decimal64::NAN, Status::INVALID);
        }

        // Denominator overflows u128 after alignment ⇒ |b| ≫ |a| at
        // the aligned quantum ⇒ truncated quotient is 0.
        if shift_b > bb_safe_shift {
            return (
                Decimal64::from_bits(pack_finite(
                    result_sign,
                    BiasedExp::ZERO_QUANTUM,
                    Coefficient::ZERO,
                )),
                Status::OK,
            );
        }

        let aligned_a = u128::from(coef_a) * POW10_U128[shift_a as usize];
        let aligned_b = u128::from(coef_b) * POW10_U128[shift_b as usize];
        debug_assert!(aligned_b > 0);

        let quotient = aligned_a / aligned_b;
        if quotient >= u128::from(COEFFICIENT_LIMIT) {
            return (Decimal64::NAN, Status::INVALID);
        }

        let q_u64 = quotient as u64;
        let coefficient = Coefficient::try_new(q_u64).expect("quotient < COEFFICIENT_LIMIT");
        (
            Decimal64::from_bits(pack_finite(
                result_sign,
                BiasedExp::ZERO_QUANTUM,
                coefficient,
            )),
            Status::OK,
        )
    }
}

/// Special-case dispatch for `divide_integer`. Returns `None` only for
/// the finite-nonzero / finite-nonzero pair the integer kernel handles.
fn divide_integer_special_cases(a: Class, b: Class) -> Option<(Decimal64, Status)> {
    use Class::{Finite, Infinity, QuietNaN, SignalingNaN, Zero};

    if let SignalingNaN { sign, payload } = a {
        return Some((
            Decimal64::from_bits(pack_quiet_nan(sign, payload)),
            Status::INVALID,
        ));
    }
    if let SignalingNaN { sign, payload } = b {
        return Some((
            Decimal64::from_bits(pack_quiet_nan(sign, payload)),
            Status::INVALID,
        ));
    }
    if let QuietNaN { sign, payload } = a {
        return Some((
            Decimal64::from_bits(pack_quiet_nan(sign, payload)),
            Status::OK,
        ));
    }
    if let QuietNaN { sign, payload } = b {
        return Some((
            Decimal64::from_bits(pack_quiet_nan(sign, payload)),
            Status::OK,
        ));
    }

    // ±∞ on the left handles inf/inf, inf/finite, and inf/zero — the
    // GDA spec treats `Inf / 0` as the infinity-arithmetic rule
    // (signed infinity, no flag), not the division-by-zero rule.
    if let Infinity { sign: sign_a } = a {
        if matches!(b, Infinity { .. }) {
            return Some((Decimal64::NAN, Status::INVALID));
        }
        let sign_b = sign_of(b);
        let result_sign = sign_a ^ sign_b;
        return Some((
            if result_sign {
                Decimal64::NEG_INFINITY
            } else {
                Decimal64::INFINITY
            },
            Status::OK,
        ));
    }

    // After the inf-on-left case, the remaining b == 0 cases are 0/0
    // (INVALID with NaN) and finite_nonzero/0 (DIV_BY_ZERO with
    // signed Infinity).
    if matches!(b, Zero { .. }) {
        if matches!(a, Zero { .. }) {
            return Some((Decimal64::NAN, Status::INVALID));
        }
        let sign_a = sign_of(a);
        let sign_b = sign_of(b);
        let result_sign = sign_a ^ sign_b;
        return Some((
            if result_sign {
                Decimal64::NEG_INFINITY
            } else {
                Decimal64::INFINITY
            },
            Status::DIV_BY_ZERO,
        ));
    }

    // finite / ±∞ → ±0 (signed by xor, exponent 0).
    if let Infinity { sign: sign_b } = b {
        let sign_a = sign_of(a);
        let result_sign = sign_a ^ sign_b;
        return Some((
            Decimal64::from_bits(pack_finite(
                result_sign,
                BiasedExp::ZERO_QUANTUM,
                Coefficient::ZERO,
            )),
            Status::OK,
        ));
    }

    // ±0 / finite_nonzero → ±0 (signed by xor, exponent 0).
    if let Zero { sign: sign_a, .. } = a {
        let _ = matches!(b, Finite { .. }); // (b must be finite here.)
        let sign_b = sign_of(b);
        let result_sign = sign_a ^ sign_b;
        return Some((
            Decimal64::from_bits(pack_finite(
                result_sign,
                BiasedExp::ZERO_QUANTUM,
                Coefficient::ZERO,
            )),
            Status::OK,
        ));
    }

    None
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

#[cfg(test)]
mod tests {
    use super::*;

    fn d(c: i64, e: i32) -> Decimal64 {
        Decimal64::try_new(c, e).unwrap()
    }

    #[test]
    fn seven_div_two_is_three() {
        let (q, st) = d(7, 0).divide_integer(d(2, 0));
        assert!(st.is_ok());
        assert_eq!(q.to_bits(), d(3, 0).to_bits());
    }

    #[test]
    fn negative_one_div_four_is_negative_zero() {
        let (q, st) = d(-1, 0).divide_integer(d(4, 0));
        assert!(st.is_ok());
        assert!(q.is_zero());
        assert!(q.is_sign_negative());
    }

    #[test]
    fn five_div_two_tenths_is_twenty_five() {
        let (q, st) = d(5, 0).divide_integer(d(2, -1));
        assert!(st.is_ok());
        assert_eq!(q.to_bits(), d(25, 0).to_bits());
    }

    #[test]
    fn nonzero_div_zero_raises_div_by_zero() {
        let (q, st) = d(1, 0).divide_integer(d(0, 0));
        assert_eq!(st, Status::DIV_BY_ZERO);
        assert_eq!(q.to_bits(), Decimal64::INFINITY.to_bits());
    }

    #[test]
    fn zero_div_zero_raises_invalid() {
        let (q, st) = d(0, 0).divide_integer(d(0, 0));
        assert_eq!(st, Status::INVALID);
        assert!(q.is_nan());
    }

    #[test]
    fn inf_div_inf_raises_invalid() {
        let (q, st) = Decimal64::INFINITY.divide_integer(Decimal64::INFINITY);
        assert_eq!(st, Status::INVALID);
        assert!(q.is_nan());
    }

    #[test]
    fn inf_div_zero_is_signed_inf_no_flag() {
        let (q, st) = Decimal64::INFINITY.divide_integer(d(0, 0));
        assert!(st.is_ok());
        assert_eq!(q.to_bits(), Decimal64::INFINITY.to_bits());
    }

    #[test]
    fn division_impossible_when_quotient_exceeds_precision() {
        // 10^16 / 1 needs 17 digits, > PRECISION = 16.
        let (q, st) = d(1, 16).divide_integer(d(1, 0));
        assert_eq!(st, Status::INVALID);
        assert!(q.is_nan());
    }

    #[test]
    fn signaling_nan_quiets_and_raises_invalid() {
        let (q, st) = Decimal64::SIGNALING_NAN.divide_integer(d(2, 0));
        assert_eq!(st, Status::INVALID);
        assert!(q.is_nan());
        assert!(!q.is_signaling_nan());
    }
}
