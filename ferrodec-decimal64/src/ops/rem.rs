//! Truncated remainder for [`Decimal64`].
//!
//! `rem(a, b) = a − trunc(a / b) × b`. Result has sign of dividend
//! and magnitude < |b|. Result quantum = `min(Q(a), Q(b))` per
//! IEEE 754-2019 §5.3.1. Returns `(NaN, INVALID)` when the integer
//! quotient would exceed `PRECISION` (= 16) digits or when an operand
//! makes the operation undefined.

use crate::bid::{classify_bits, BIAS, Class, COEFFICIENT_LIMIT};
use crate::decimal::Decimal64;
use ferrodec_ieee::{RoundingMode, Status};

const POW10_U128: [u128; 24] = {
    let mut t = [0u128; 24];
    let mut i = 0;
    let mut v: u128 = 1;
    while i < 24 {
        t[i] = v;
        if i < 23 {
            v *= 10;
        }
        i += 1;
    }
    t
};

// Compile-time invariant: every `POW10_U128[k]` access satisfies
// `k <= MAX_SAFE_SHIFT`.
const _: () = {
    // Defined further down; reproduced here so the assert can sit
    // next to the table.
    const MAX_SAFE_SHIFT: u32 = 22;
    assert!(POW10_U128.len() > MAX_SAFE_SHIFT as usize);
};

const MAX_SAFE_SHIFT: u32 = 22;

impl Decimal64 {
    /// Truncated remainder.
    #[must_use]
    pub fn rem(self, other: Self, rm: RoundingMode) -> (Self, Status) {
        let _ = rm;
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
        let (_sign_b, biased_b, coef_b) = match cb {
            Class::Finite { sign, biased_exp, coefficient } => (sign, biased_exp, coefficient),
            Class::Zero { sign, biased_exp } => (sign, biased_exp, 0u64),
            _ => unreachable!(),
        };

        let exp_a = biased_a as i32 - BIAS as i32;
        let exp_b = biased_b as i32 - BIAS as i32;
        let target_q = exp_a.min(exp_b);

        if coef_a == 0 {
            return (
                Decimal64::from_bits(crate::bid::pack_finite(
                    sign_a,
                    (target_q + BIAS as i32) as u32,
                    0,
                )),
                Status::OK,
            );
        }

        let shift_a = (exp_a - target_q) as u32;
        let shift_b = (exp_b - target_q) as u32;

        if shift_a > MAX_SAFE_SHIFT || shift_b > MAX_SAFE_SHIFT {
            if shift_a > MAX_SAFE_SHIFT {
                return (Decimal64::NAN, Status::INVALID);
            }
            return (
                Decimal64::from_bits(crate::bid::pack_finite(
                    sign_a,
                    (exp_a + BIAS as i32) as u32,
                    coef_a,
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
        let residue = aligned_a - quotient * aligned_b;
        debug_assert!(residue < aligned_b);

        if residue >= u128::from(COEFFICIENT_LIMIT) {
            return (Decimal64::NAN, Status::INVALID);
        }

        (
            Decimal64::from_bits(crate::bid::pack_finite(
                sign_a,
                (target_q + BIAS as i32) as u32,
                residue as u64,
            )),
            Status::OK,
        )
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
    if matches!(a, Infinity { .. }) {
        return Some((Decimal64::NAN, Status::INVALID));
    }
    if matches!(b, Zero { .. }) {
        return Some((Decimal64::NAN, Status::INVALID));
    }
    if matches!(b, Infinity { .. }) {
        if let Finite { sign, biased_exp, coefficient } = a {
            return Some((
                Decimal64::from_bits(crate::bid::pack_finite(sign, biased_exp, coefficient)),
                Status::OK,
            ));
        }
        if let Zero { sign, biased_exp } = a {
            return Some((
                Decimal64::from_bits(crate::bid::pack_finite(sign, biased_exp, 0)),
                Status::OK,
            ));
        }
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
    fn rem_basic() {
        let (r, _) = from_int(10, 0).rem(from_int(3, 0), RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), from_int(1, 0).to_bits());

        let (r, _) = from_int(10, 0).rem(from_int(5, 0), RoundingMode::NearestEven);
        assert!(r.is_zero() && !r.is_sign_negative());

        let (r, _) = from_int(-10, 0).rem(from_int(3, 0), RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), from_int(-1, 0).to_bits());
    }

    #[test]
    fn rem_zero_dividend() {
        let (r, _) = Decimal64::ZERO.rem(from_int(5, 0), RoundingMode::NearestEven);
        assert!(r.is_zero() && !r.is_sign_negative());
    }

    #[test]
    fn rem_by_zero_invalid() {
        let (r, s) = from_int(5, 0).rem(Decimal64::ZERO, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());
    }

    #[test]
    fn rem_infinity() {
        let (r, s) = Decimal64::INFINITY.rem(from_int(3, 0), RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());

        let (r, _) = from_int(7, 0).rem(Decimal64::INFINITY, RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), from_int(7, 0).to_bits());
    }

    #[test]
    fn rem_too_large_quotient_invalid() {
        let (r, s) = Decimal64::MAX.rem(Decimal64::MIN_POSITIVE, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());
    }

    #[test]
    fn rem_dividend_smaller_than_divisor() {
        let (r, _) = from_int(3, 0).rem(from_int(10, 0), RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), from_int(3, 0).to_bits());
    }

    #[test]
    fn rem_nan_propagation() {
        let (r, s) = Decimal64::NAN.rem(Decimal64::ONE, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.is_ok());

        let (r, s) = Decimal64::SIGNALING_NAN.rem(Decimal64::ONE, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());
    }
}
