//! IEEE 754-2019 divide for [`Decimal64`].
//!
//! Long division at u128 working precision. Scale dividend by
//! `10^(db - da + PRECISION + 1) = 10^(db - da + 17)` to produce a
//! 17-digit (or wider) integer quotient, then route through
//! `round_and_pack_into_u64`.

use crate::bid::{classify_bits, decimal_digit_count, BIAS, Class, PRECISION};
use crate::decimal::Decimal64;
use ferrodec_ieee::{RoundingMode, Status};

use super::addsub::round_and_pack_into_u64;

const POW10_U128: [u128; 34] = {
    let mut t = [0u128; 34];
    let mut i = 0;
    let mut v: u128 = 1;
    while i < 34 {
        t[i] = v;
        if i < 33 {
            v *= 10;
        }
        i += 1;
    }
    t
};

// Compile-time invariant: the largest reachable index is
// `(db − da) + PRECISION + 1` with `db ≤ PRECISION = 16` and `da
// ≥ 1`, so max = `15 + 17 = 32`. POW10_U128 needs ≥ 33 entries.
const _: () = assert!(POW10_U128.len() > (crate::bid::PRECISION as usize - 1) + crate::bid::PRECISION as usize + 1);

impl Decimal64 {
    /// IEEE 754-2019 `division(self, other)` rounded by `rm`.
    #[must_use]
    pub fn div(self, other: Self, rm: RoundingMode) -> (Self, Status) {
        let ca = classify_bits(self.0);
        let cb = classify_bits(other.0);

        if let Some(out) = handle_specials(ca, cb) {
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

        let result_sign = sign_a ^ sign_b;
        let exp_a = biased_a as i32 - BIAS as i32;
        let exp_b = biased_b as i32 - BIAS as i32;
        let q_preferred = exp_a - exp_b;

        if coef_a == 0 {
            return round_and_pack_into_u64(0, q_preferred, q_preferred, result_sign, false, rm);
        }

        let da = decimal_digit_count(coef_a);
        let db = decimal_digit_count(coef_b);
        let scale: i32 = (db as i32 - da as i32) + (PRECISION as i32 + 1);
        debug_assert!(scale >= 0);
        let scale_u = scale as u32;
        debug_assert!((scale_u as usize) < POW10_U128.len());

        let scaled_a = u128::from(coef_a) * POW10_U128[scale_u as usize];
        let divisor = u128::from(coef_b);
        let quotient = scaled_a / divisor;
        let remainder = scaled_a % divisor;
        let sticky = remainder != 0;

        let result_exp = exp_a - exp_b - scale;

        round_and_pack_into_u64(quotient, result_exp, q_preferred, result_sign, sticky, rm)
    }
}

fn handle_specials(a: Class, b: Class) -> Option<(Decimal64, Status)> {
    use Class::{Finite, Infinity, QuietNaN, SignalingNaN, Zero};

    if let SignalingNaN { sign, payload } = a {
        return Some((
            Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::INVALID,
        ));
    }
    if let SignalingNaN { sign, payload } = b {
        return Some((
            Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::INVALID,
        ));
    }
    if let QuietNaN { sign, payload } = a {
        return Some((
            Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::OK,
        ));
    }
    if let QuietNaN { sign, payload } = b {
        return Some((
            Decimal64::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::OK,
        ));
    }
    if matches!(a, Zero { .. }) && matches!(b, Zero { .. }) {
        return Some((Decimal64::NAN, Status::INVALID));
    }
    if matches!(a, Infinity { .. }) && matches!(b, Infinity { .. }) {
        return Some((Decimal64::NAN, Status::INVALID));
    }
    if let (Finite { sign: sa, .. }, Zero { sign: sb, .. }) = (a, b) {
        let result_sign = sa ^ sb;
        return Some((
            Decimal64::from_bits(crate::bid::pack_infinity(result_sign)),
            Status::DIV_BY_ZERO,
        ));
    }
    if let Infinity { sign: sa } = a {
        let sb = match b {
            Finite { sign, .. } | Zero { sign, .. } => sign,
            _ => unreachable!(),
        };
        return Some((
            Decimal64::from_bits(crate::bid::pack_infinity(sa ^ sb)),
            Status::OK,
        ));
    }
    if let (Finite { sign: sa, .. } | Zero { sign: sa, .. }, Infinity { sign: sb }) = (a, b) {
        let result_sign = sa ^ sb;
        return Some((
            Decimal64::from_bits(crate::bid::pack_finite(result_sign, 0, 0)),
            Status::OK,
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bid::pack_finite;

    fn from_int(n: i64, exp: i32) -> Decimal64 {
        Decimal64::try_new(n, exp).unwrap()
    }

    #[test]
    fn div_exact() {
        let (r, s) = from_int(6, 0).div(from_int(2, 0), RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), from_int(3, 0).to_bits());
        assert!(s.is_ok());

        let (r, _) = from_int(10, 0).div(from_int(4, 0), RoundingMode::NearestEven);
        let expected = Decimal64::from_bits(pack_finite(false, BIAS - 1, 25));
        assert_eq!(r.to_bits(), expected.to_bits());
    }

    #[test]
    fn div_inexact() {
        // 1 / 3 = 0.3333... at 16 digits.
        let (r, s) = from_int(1, 0).div(from_int(3, 0), RoundingMode::NearestEven);
        let expected = Decimal64::from_bits(pack_finite(false, BIAS - 16, 3_333_333_333_333_333));
        assert_eq!(r.to_bits(), expected.to_bits());
        assert!(s.inexact());
    }

    #[test]
    fn div_signs() {
        let (r, _) = from_int(-6, 0).div(from_int(2, 0), RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), from_int(-3, 0).to_bits());

        let (r, _) = from_int(-6, 0).div(from_int(-2, 0), RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), from_int(3, 0).to_bits());
    }

    #[test]
    fn div_by_zero() {
        let (r, s) = from_int(1, 0).div(Decimal64::ZERO, RoundingMode::NearestEven);
        assert!(r.is_infinite() && !r.is_sign_negative());
        assert!(s.div_by_zero());

        let (r, _) = from_int(-1, 0).div(Decimal64::ZERO, RoundingMode::NearestEven);
        assert!(r.is_infinite() && r.is_sign_negative());
    }

    #[test]
    fn div_zero_by_zero_invalid() {
        let (r, s) = Decimal64::ZERO.div(Decimal64::ZERO, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());
    }

    #[test]
    fn div_zero_by_finite() {
        let (r, _) = Decimal64::ZERO.div(from_int(5, 0), RoundingMode::NearestEven);
        assert!(r.is_zero() && !r.is_sign_negative());

        let (r, _) = Decimal64::ZERO.div(from_int(-5, 0), RoundingMode::NearestEven);
        assert!(r.is_zero() && r.is_sign_negative());
    }

    #[test]
    fn div_infinity() {
        let (r, _) = Decimal64::INFINITY.div(from_int(2, 0), RoundingMode::NearestEven);
        assert!(r.is_infinite() && !r.is_sign_negative());

        let (r, _) = from_int(5, 0).div(Decimal64::INFINITY, RoundingMode::NearestEven);
        assert!(r.is_zero() && !r.is_sign_negative());

        let (r, s) = Decimal64::INFINITY.div(Decimal64::INFINITY, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());
    }

    #[test]
    fn div_overflow() {
        let (r, s) = Decimal64::MAX.div(Decimal64::MIN_POSITIVE, RoundingMode::NearestEven);
        assert!(r.is_infinite());
        assert!(s.overflow() && s.inexact());
    }

    #[test]
    fn div_underflow() {
        let (r, s) = Decimal64::MIN_POSITIVE.div(Decimal64::MAX, RoundingMode::NearestEven);
        assert!(r.is_zero());
        assert!(s.inexact() && s.underflow());
    }

    #[test]
    fn div_nan_propagation() {
        let (r, s) = Decimal64::NAN.div(Decimal64::ONE, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.is_ok());

        let (r, s) = Decimal64::SIGNALING_NAN.div(Decimal64::ONE, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());
    }
}
