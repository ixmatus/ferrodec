//! IEEE 754-2019 multiply for [`Decimal64`].
//!
//! `u64 × u64 → u128` (max product `(10¹⁶ − 1)² ≈ 10³²`), compressed
//! back to `u64` via sticky tracking before routing through
//! `round_and_pack_finite`.

use crate::bid::{classify_bits, BIAS, Class};
use crate::decimal::Decimal64;
use crate::status::{RoundingMode, Status};

use super::addsub::round_and_pack_into_u64;

impl Decimal64 {
    /// IEEE 754-2019 `multiplication(self, other)` rounded by `rm`.
    #[must_use]
    pub fn mul(self, other: Self, rm: RoundingMode) -> (Self, Status) {
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
        let q_preferred = exp_a + exp_b;

        // u64 × u64 → u128. Max product (10¹⁶ − 1)² < 10³² < 2¹⁰⁶.
        let product = u128::from(coef_a) * u128::from(coef_b);

        round_and_pack_into_u64(product, q_preferred, q_preferred, result_sign, false, rm)
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

    if matches!((a, b), (Zero { .. }, Infinity { .. }) | (Infinity { .. }, Zero { .. })) {
        return Some((Decimal64::NAN, Status::INVALID));
    }

    let (sa, sb) = match (a, b) {
        (Infinity { sign: sa }, Infinity { sign: sb }) => (Some(sa), Some(sb)),
        (Infinity { sign: sa }, Finite { sign: sb, .. }) => (Some(sa), Some(sb)),
        (Finite { sign: sa, .. }, Infinity { sign: sb }) => (Some(sa), Some(sb)),
        _ => (None, None),
    };
    if let (Some(sa), Some(sb)) = (sa, sb) {
        return Some((
            Decimal64::from_bits(crate::bid::pack_infinity(sa ^ sb)),
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
    fn mul_basic() {
        let (r, s) = from_int(2, 0).mul(from_int(3, 0), RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), from_int(6, 0).to_bits());
        assert!(s.is_ok());
    }

    #[test]
    fn mul_with_signs() {
        let (r, _) = from_int(-2, 0).mul(from_int(3, 0), RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), from_int(-6, 0).to_bits());

        let (r, _) = from_int(-2, 0).mul(from_int(-3, 0), RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), from_int(6, 0).to_bits());
    }

    #[test]
    fn mul_quantum_addition() {
        // 1.5 × 2.0 = 3.00 (q_preferred = -1 + -1 = -2).
        let (r, _) = from_int(15, -1).mul(from_int(20, -1), RoundingMode::NearestEven);
        let expected = Decimal64::from_bits(pack_finite(false, BIAS - 2, 300));
        assert_eq!(r.to_bits(), expected.to_bits());
    }

    #[test]
    fn mul_sixteen_digits_full_precision() {
        let (r, s) = from_int(9_999_999_999_999_999, 0)
            .mul(from_int(1, 0), RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), from_int(9_999_999_999_999_999, 0).to_bits());
        assert!(s.is_ok());
    }

    #[test]
    fn mul_inexact_rounds() {
        // 1234567890123456 × 1234567890123456 = 32-digit product;
        // round to 16 digits.
        let (r, s) = from_int(1_234_567_890_123_456, 0)
            .mul(from_int(1_234_567_890_123_456, 0), RoundingMode::NearestEven);
        assert!(r.is_finite() && !r.is_sign_negative());
        assert!(s.inexact());
    }

    #[test]
    fn mul_zero() {
        let (r, _) = from_int(5, 0).mul(Decimal64::ZERO, RoundingMode::NearestEven);
        assert!(r.is_zero() && !r.is_sign_negative());

        let (r, _) = from_int(-5, 0).mul(Decimal64::ZERO, RoundingMode::NearestEven);
        assert!(r.is_zero() && r.is_sign_negative());
    }

    #[test]
    fn mul_overflow() {
        let (r, s) = Decimal64::MAX.mul(from_int(10, 0), RoundingMode::NearestEven);
        assert!(r.is_infinite() && !r.is_sign_negative());
        assert!(s.overflow() && s.inexact());
    }

    #[test]
    fn mul_underflow() {
        let (r, s) = Decimal64::MIN_POSITIVE.mul(from_int(1, -1), RoundingMode::NearestEven);
        assert!(r.is_zero());
        assert!(s.inexact() && s.underflow());
    }

    #[test]
    fn mul_nan_propagation() {
        let (r, s) = Decimal64::NAN.mul(Decimal64::ONE, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.is_ok());

        let (r, s) = Decimal64::SIGNALING_NAN.mul(Decimal64::ONE, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());
    }

    #[test]
    fn mul_infinity() {
        let (r, _) = Decimal64::INFINITY.mul(from_int(2, 0), RoundingMode::NearestEven);
        assert!(r.is_infinite() && !r.is_sign_negative());

        let (r, _) = Decimal64::INFINITY.mul(from_int(-2, 0), RoundingMode::NearestEven);
        assert!(r.is_infinite() && r.is_sign_negative());

        let (r, _) = Decimal64::INFINITY.mul(Decimal64::INFINITY, RoundingMode::NearestEven);
        assert!(r.is_infinite() && !r.is_sign_negative());
    }

    #[test]
    fn mul_infinity_zero_invalid() {
        let (r, s) = Decimal64::ZERO.mul(Decimal64::INFINITY, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());
    }
}
