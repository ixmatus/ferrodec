//! General Decimal Arithmetic `divideInteger` — truncated integer
//! quotient with exponent zero.
//!
//! `Decimal32` counterpart to the parent
//! [`crate::Decimal32::divide_integer`]. The working multiword type is
//! `u128`, matching `ferrodec-decimal32/src/ops/rem.rs`. Decimal32's
//! `PRECISION = 7` keeps every representable integer quotient well
//! inside `u32`. See ADR-0031 for the GDA lens relitigation.

use crate::bid::{
    classify_bits, decimal_digit_count, pack_finite, pack_quiet_nan, BiasedExp, Class, Coefficient,
    BIAS, COEFFICIENT_LIMIT,
};
use crate::decimal::Decimal32;
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

impl Decimal32 {
    /// General Decimal Arithmetic `divideInteger(x, y)`.
    ///
    /// Returns `trunc(x / y)` as an integer at exponent 0. See the
    /// [`Decimal64`] / [`Decimal128`] counterparts for the full
    /// special-case table; this Decimal32 implementation is
    /// structurally identical, with PRECISION = 7.
    ///
    /// Decimal32 has no upstream `dsDivideInt.decTest`; verification
    /// rests on the hand-derived unit tests below (mirroring the
    /// Decimal64 conformance coverage at precision 7).
    ///
    /// # Examples
    ///
    /// ```
    /// use ferrodec_decimal32::Decimal32;
    ///
    /// let x = Decimal32::try_new(7, 0).unwrap();
    /// let y = Decimal32::try_new(2, 0).unwrap();
    /// let (q, st) = x.divide_integer(y);
    /// assert!(st.is_ok());
    /// assert_eq!(q.to_bits(), Decimal32::try_new(3, 0).unwrap().to_bits());
    /// ```
    ///
    /// [`Decimal64`]: ferrodec_decimal64::Decimal64
    /// [`Decimal128`]: ferrodec::Decimal128
    #[must_use]
    pub fn divide_integer(self, rhs: Self) -> (Self, Status) {
        let ca = classify_bits(self.0);
        let cb = classify_bits(rhs.0);

        if let Some(out) = divide_integer_special_cases(ca, cb) {
            return out;
        }

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

        if shift_a > ab_safe_shift {
            return (Decimal32::NAN, Status::INVALID);
        }

        if shift_b > bb_safe_shift {
            return (
                Decimal32::from_bits(pack_finite(
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
            return (Decimal32::NAN, Status::INVALID);
        }

        let q_u32 = quotient as u32;
        let coefficient = Coefficient::try_new(q_u32).expect("quotient < COEFFICIENT_LIMIT");
        (
            Decimal32::from_bits(pack_finite(
                result_sign,
                BiasedExp::ZERO_QUANTUM,
                coefficient,
            )),
            Status::OK,
        )
    }
}

fn divide_integer_special_cases(a: Class, b: Class) -> Option<(Decimal32, Status)> {
    use Class::{Infinity, QuietNaN, SignalingNaN, Zero};

    if let SignalingNaN { sign, payload } = a {
        return Some((
            Decimal32::from_bits(pack_quiet_nan(sign, payload)),
            Status::INVALID,
        ));
    }
    if let SignalingNaN { sign, payload } = b {
        return Some((
            Decimal32::from_bits(pack_quiet_nan(sign, payload)),
            Status::INVALID,
        ));
    }
    if let QuietNaN { sign, payload } = a {
        return Some((
            Decimal32::from_bits(pack_quiet_nan(sign, payload)),
            Status::OK,
        ));
    }
    if let QuietNaN { sign, payload } = b {
        return Some((
            Decimal32::from_bits(pack_quiet_nan(sign, payload)),
            Status::OK,
        ));
    }

    if let Infinity { sign: sign_a } = a {
        if matches!(b, Infinity { .. }) {
            return Some((Decimal32::NAN, Status::INVALID));
        }
        let sign_b = sign_of(b);
        let result_sign = sign_a ^ sign_b;
        return Some((
            if result_sign {
                Decimal32::NEG_INFINITY
            } else {
                Decimal32::INFINITY
            },
            Status::OK,
        ));
    }

    if matches!(b, Zero { .. }) {
        if matches!(a, Zero { .. }) {
            return Some((Decimal32::NAN, Status::INVALID));
        }
        let sign_a = sign_of(a);
        let sign_b = sign_of(b);
        let result_sign = sign_a ^ sign_b;
        return Some((
            if result_sign {
                Decimal32::NEG_INFINITY
            } else {
                Decimal32::INFINITY
            },
            Status::DIV_BY_ZERO,
        ));
    }

    if let Infinity { sign: sign_b } = b {
        let sign_a = sign_of(a);
        let result_sign = sign_a ^ sign_b;
        return Some((
            Decimal32::from_bits(pack_finite(
                result_sign,
                BiasedExp::ZERO_QUANTUM,
                Coefficient::ZERO,
            )),
            Status::OK,
        ));
    }

    if let Zero { sign: sign_a, .. } = a {
        let sign_b = sign_of(b);
        let result_sign = sign_a ^ sign_b;
        return Some((
            Decimal32::from_bits(pack_finite(
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

    fn d(c: i32, e: i32) -> Decimal32 {
        Decimal32::try_new(c, e).unwrap()
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
        assert_eq!(q.to_bits(), Decimal32::INFINITY.to_bits());
    }

    #[test]
    fn zero_div_zero_raises_invalid() {
        let (q, st) = d(0, 0).divide_integer(d(0, 0));
        assert_eq!(st, Status::INVALID);
        assert!(q.is_nan());
    }

    #[test]
    fn inf_div_inf_raises_invalid() {
        let (q, st) = Decimal32::INFINITY.divide_integer(Decimal32::INFINITY);
        assert_eq!(st, Status::INVALID);
        assert!(q.is_nan());
    }

    #[test]
    fn inf_div_zero_is_signed_inf_no_flag() {
        let (q, st) = Decimal32::INFINITY.divide_integer(d(0, 0));
        assert!(st.is_ok());
        assert_eq!(q.to_bits(), Decimal32::INFINITY.to_bits());
    }

    #[test]
    fn division_impossible_when_quotient_exceeds_precision() {
        // 10^7 / 1 needs 8 digits, > PRECISION = 7.
        let (q, st) = d(1, 7).divide_integer(d(1, 0));
        assert_eq!(st, Status::INVALID);
        assert!(q.is_nan());
    }

    #[test]
    fn signaling_nan_quiets_and_raises_invalid() {
        let (q, st) = Decimal32::SIGNALING_NAN.divide_integer(d(2, 0));
        assert_eq!(st, Status::INVALID);
        assert!(q.is_nan());
        assert!(!q.is_signaling_nan());
    }
}
