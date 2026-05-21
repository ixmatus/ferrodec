//! General Decimal Arithmetic `shift` on `Decimal64`. Same shape as
//! the parent `Decimal128::shift`; see ADR-0031.

use crate::bid::{
    classify_bits, pack_finite, pack_quiet_nan, BiasedExp, Class, Coefficient, BIAS, PRECISION,
};
use crate::decimal::Decimal64;
use crate::digits::{coefficient_to_digits, digits_to_coefficient};
use ferrodec_ieee::Status;

pub(crate) fn validate_shift_rhs(rhs: Decimal64) -> Option<i32> {
    let (sign, biased_exp, coef) = match classify_bits(rhs.0) {
        Class::Zero { sign, biased_exp } => (sign, biased_exp, 0u64),
        Class::Finite {
            sign,
            biased_exp,
            coefficient,
        } => (sign, biased_exp, coefficient),
        _ => return None,
    };
    if biased_exp != BIAS {
        return None;
    }
    if coef > u64::from(PRECISION) {
        return None;
    }
    let n = coef as i32;
    Some(if sign { -n } else { n })
}

impl Decimal64 {
    /// General Decimal Arithmetic `shift(x, n)`. See
    /// [`Decimal128::shift`] for the full contract.
    ///
    /// [`Decimal128::shift`]: ferrodec::Decimal128::shift
    #[must_use]
    pub fn shift(self, rhs: Self) -> (Self, Status) {
        digit_shift(self, rhs, /*wrap=*/ false)
    }
}

pub(crate) fn digit_shift(lhs: Decimal64, rhs: Decimal64, wrap: bool) -> (Decimal64, Status) {
    match classify_bits(lhs.0) {
        Class::SignalingNaN { sign, payload } => {
            return (
                Decimal64::from_bits(pack_quiet_nan(sign, payload)),
                Status::INVALID,
            );
        }
        _ => {}
    }
    match classify_bits(rhs.0) {
        Class::SignalingNaN { sign, payload } => {
            return (
                Decimal64::from_bits(pack_quiet_nan(sign, payload)),
                Status::INVALID,
            );
        }
        _ => {}
    }
    if let Class::QuietNaN { .. } = classify_bits(lhs.0) {
        return (lhs, Status::OK);
    }
    if let Class::QuietNaN { .. } = classify_bits(rhs.0) {
        return (rhs, Status::OK);
    }
    let n = match validate_shift_rhs(rhs) {
        Some(v) => v,
        None => return (Decimal64::NAN, Status::INVALID),
    };
    if matches!(classify_bits(lhs.0), Class::Infinity { .. }) {
        return (lhs, Status::OK);
    }
    let (sign, biased_exp, coef) = match classify_bits(lhs.0) {
        Class::Zero { sign, biased_exp } => (sign, biased_exp, 0u64),
        Class::Finite {
            sign,
            biased_exp,
            coefficient,
        } => (sign, biased_exp, coefficient),
        _ => unreachable!("NaN / Infinity handled above"),
    };
    let bexp_typed = BiasedExp::try_from_biased(biased_exp).expect("biased_exp from classify_bits");
    if coef == 0 {
        return (
            Decimal64::from_bits(pack_finite(sign, bexp_typed, Coefficient::ZERO)),
            Status::OK,
        );
    }
    let mut digits = [0u8; PRECISION as usize];
    coefficient_to_digits(coef, &mut digits);
    let p = PRECISION as i32;
    debug_assert!(n.abs() <= p);
    let mut out = [0u8; PRECISION as usize];
    if wrap {
        let n_mod = n.rem_euclid(p) as usize;
        for i in 0..PRECISION as usize {
            out[(i + n_mod) % PRECISION as usize] = digits[i];
        }
    } else if n > 0 {
        let n = n as usize;
        for i in 0..(PRECISION as usize - n) {
            out[i + n] = digits[i];
        }
    } else if n < 0 {
        let n_abs = (-n) as usize;
        for i in 0..(PRECISION as usize - n_abs) {
            out[i] = digits[i + n_abs];
        }
    } else {
        out = digits;
    }
    let new_coef = digits_to_coefficient(&out);
    let coef_typed = Coefficient::try_new(new_coef).expect("digits-derived coef fits Decimal64");
    (
        Decimal64::from_bits(pack_finite(sign, bexp_typed, coef_typed)),
        Status::OK,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(c: i64, e: i32) -> Decimal64 {
        Decimal64::try_new(c, e).unwrap()
    }

    #[test]
    fn shift_one_by_two_is_one_hundred() {
        let (r, st) = d(1, 0).shift(d(2, 0));
        assert!(st.is_ok());
        assert_eq!(r.to_bits(), d(100, 0).to_bits());
    }

    #[test]
    fn shift_full_precision_left_drops_all_digits() {
        let (r, st) = d(1, 0).shift(d(16, 0));
        assert!(st.is_ok());
        assert_eq!(r.to_bits(), d(0, 0).to_bits());
    }

    #[test]
    fn shift_right_drops_low_digits() {
        let (r, st) = d(1234, 0).shift(d(-2, 0));
        assert!(st.is_ok());
        assert_eq!(r.to_bits(), d(12, 0).to_bits());
    }

    #[test]
    fn rhs_non_integer_is_invalid() {
        let (r, st) = d(1, 0).shift(d(10, -1));
        assert_eq!(st, Status::INVALID);
        assert!(r.is_nan());
    }

    #[test]
    fn rhs_above_precision_is_invalid() {
        let (r, st) = d(1, 0).shift(d(17, 0));
        assert_eq!(st, Status::INVALID);
        assert!(r.is_nan());
    }

    #[test]
    fn signaling_nan_lhs_quiets_and_raises_invalid() {
        let (r, st) = Decimal64::SIGNALING_NAN.shift(d(3, 0));
        assert_eq!(st, Status::INVALID);
        assert!(r.is_nan());
        assert!(!r.is_signaling_nan());
    }
}
