//! IEEE 754-2019 divide for [`Decimal32`].
//!
//! Returns `(Decimal32, Status)`. The finite path scales the dividend
//! so the integer quotient holds at least `PRECISION + 1 = 8` digits,
//! then routes through `round_and_pack_finite` with the post-scale
//! remainder feeding the rounding sticky bit.
//!
//! # Special cases (IEEE 754-2019 §7)
//!
//! * sNaN in either operand → quiet NaN + `INVALID`.
//! * qNaN propagation (a preferred per §6.2.3).
//! * `0 / 0` → NaN + `INVALID`.
//! * `±∞ / ±∞` → NaN + `INVALID`.
//! * `finite / 0` (finite ≠ 0) → ±∞ + `DIV_BY_ZERO`, sign by XOR.
//! * `±∞ / finite` → ±∞, sign by XOR.
//! * `finite / ±∞` → ±0, sign by XOR (preferred quantum is the
//!   minimum representable value, delegated to `round_and_pack`'s
//!   zero branch).

use crate::bid::{classify_bits, decimal_digit_count, BIAS, Class, PRECISION};
use crate::decimal::Decimal32;
use ferrodec_ieee::{RoundingMode, Status};

use super::round::round_and_pack_finite;

const POW10_U64: [u64; 16] = [
    1,
    10,
    100,
    1_000,
    10_000,
    100_000,
    1_000_000,
    10_000_000,
    100_000_000,
    1_000_000_000,
    10_000_000_000,
    100_000_000_000,
    1_000_000_000_000,
    10_000_000_000_000,
    100_000_000_000_000,
    1_000_000_000_000_000,
];

impl Decimal32 {
    /// IEEE 754-2019 `division(self, other)` rounded by `rm`.
    #[must_use]
    pub fn div(self, other: Self, rm: RoundingMode) -> (Self, Status) {
        let ca = classify_bits(self.0);
        let cb = classify_bits(other.0);

        if let Some(out) = handle_specials(ca, cb) {
            return out;
        }

        // Finite / finite (Zero / Finite handled via Class::Zero).
        let (sign_a, biased_a, coef_a) = match ca {
            Class::Finite { sign, biased_exp, coefficient } => (sign, biased_exp, u64::from(coefficient)),
            Class::Zero { sign, biased_exp } => (sign, biased_exp, 0u64),
            _ => unreachable!("dispatcher handles non-finite"),
        };
        let (sign_b, biased_b, coef_b) = match cb {
            Class::Finite { sign, biased_exp, coefficient } => (sign, biased_exp, u64::from(coefficient)),
            Class::Zero { sign, biased_exp } => (sign, biased_exp, 0u64),
            _ => unreachable!("dispatcher handles non-finite"),
        };

        let result_sign = sign_a ^ sign_b;
        let exp_a = biased_a as i32 - BIAS as i32;
        let exp_b = biased_b as i32 - BIAS as i32;
        let q_preferred = exp_a - exp_b;

        // Both finite: divisor is non-zero (zero divisor handled by
        // dispatcher above). Numerator may be zero.
        if coef_a == 0 {
            // 0 / non-zero = ±0 at preferred quantum (clamped by
            // round_and_pack's zero branch if out of range).
            return round_and_pack_finite(0, q_preferred, q_preferred, result_sign, false, rm, Status::OK);
        }

        // Scale dividend so quotient has ≥ PRECISION + 1 digits.
        let da = decimal_digit_count(coef_a as u32);
        let db = decimal_digit_count(coef_b as u32);
        let scale: i32 = (db as i32 - da as i32) + (PRECISION as i32 + 1);
        debug_assert!(scale >= 0); // PRECISION = 7 ≥ |da - db|
        let scale_u = scale as u32;
        debug_assert!((scale_u as usize) < POW10_U64.len());

        let scaled_a = coef_a * POW10_U64[scale_u as usize];
        let quotient = scaled_a / coef_b;
        let remainder = scaled_a % coef_b;
        let sticky = remainder != 0;

        let result_exp = exp_a - exp_b - scale;

        round_and_pack_finite(
            quotient,
            result_exp,
            q_preferred,
            result_sign,
            sticky,
            rm,
            Status::OK,
        )
    }
}

fn handle_specials(a: Class, b: Class) -> Option<(Decimal32, Status)> {
    use Class::{Finite, Infinity, QuietNaN, SignalingNaN, Zero};

    if let SignalingNaN { sign, payload } = a {
        return Some((
            Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::INVALID,
        ));
    }
    if let SignalingNaN { sign, payload } = b {
        return Some((
            Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::INVALID,
        ));
    }
    if let QuietNaN { sign, payload } = a {
        return Some((
            Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::OK,
        ));
    }
    if let QuietNaN { sign, payload } = b {
        return Some((
            Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
            Status::OK,
        ));
    }

    // 0 / 0 → NaN + INVALID.
    if matches!(a, Zero { .. }) && matches!(b, Zero { .. }) {
        return Some((Decimal32::NAN, Status::INVALID));
    }

    // ∞ / ∞ → NaN + INVALID.
    if matches!(a, Infinity { .. }) && matches!(b, Infinity { .. }) {
        return Some((Decimal32::NAN, Status::INVALID));
    }

    // finite / 0 (finite ≠ 0) → ±∞ + DIV_BY_ZERO.
    if let (Finite { sign: sa, .. }, Zero { sign: sb, .. }) = (a, b) {
        let result_sign = sa ^ sb;
        return Some((
            Decimal32::from_bits(crate::bid::pack_infinity(result_sign)),
            Status::DIV_BY_ZERO,
        ));
    }

    // ±∞ / finite or ±∞ / 0 → ±∞ (no flag for 0 in denominator since
    // ∞ already absorbs it).
    if let Infinity { sign: sa } = a {
        let sb = match b {
            Finite { sign, .. } | Zero { sign, .. } => sign,
            _ => unreachable!(),
        };
        return Some((
            Decimal32::from_bits(crate::bid::pack_infinity(sa ^ sb)),
            Status::OK,
        ));
    }

    // finite / ±∞ → ±0 (sign by XOR). Quantum decision: IEEE 754-2019
    // §6.3 preferred is exp_a - exp_b, but with exp_b unbounded we
    // pin to the smallest representable quantum (handled by
    // round_and_pack_finite's zero clamp).
    if let (Finite { sign: sa, .. } | Zero { sign: sa, .. }, Infinity { sign: sb }) = (a, b) {
        let result_sign = sa ^ sb;
        return Some((
            Decimal32::from_bits(crate::bid::pack_finite(result_sign, 0, 0)),
            Status::OK,
        ));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bid::pack_finite;

    fn from_int(n: i32, exp: i32) -> Decimal32 {
        Decimal32::try_new(n, exp).unwrap()
    }

    #[test]
    fn div_exact() {
        // 6 / 2 = 3. q_preferred = 0 - 0 = 0. Strip trailing zeros to
        // match preferred quantum.
        let (r, s) = from_int(6, 0).div(from_int(2, 0), RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), from_int(3, 0).to_bits());
        assert!(s.is_ok());

        // 10 / 4 = 2.5
        let (r, s) = from_int(10, 0).div(from_int(4, 0), RoundingMode::NearestEven);
        let expected = Decimal32::from_bits(pack_finite(false, BIAS - 1, 25));
        assert_eq!(r.to_bits(), expected.to_bits());
        assert!(s.is_ok());
    }

    #[test]
    fn div_inexact() {
        // 1 / 3 = 0.3333333 (7 digits, INEXACT).
        let (r, s) = from_int(1, 0).div(from_int(3, 0), RoundingMode::NearestEven);
        let expected = Decimal32::from_bits(pack_finite(false, BIAS - 7, 3_333_333));
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
        let (r, s) = from_int(1, 0).div(Decimal32::ZERO, RoundingMode::NearestEven);
        assert!(r.is_infinite() && !r.is_sign_negative());
        assert!(s.div_by_zero());

        let (r, s) = from_int(-1, 0).div(Decimal32::ZERO, RoundingMode::NearestEven);
        assert!(r.is_infinite() && r.is_sign_negative());
        assert!(s.div_by_zero());

        let (r, s) = from_int(1, 0).div(Decimal32::NEG_ZERO, RoundingMode::NearestEven);
        assert!(r.is_infinite() && r.is_sign_negative());
        assert!(s.div_by_zero());
    }

    #[test]
    fn div_zero_by_zero_invalid() {
        let (r, s) = Decimal32::ZERO.div(Decimal32::ZERO, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());
    }

    #[test]
    fn div_zero_by_finite() {
        let (r, _) = Decimal32::ZERO.div(from_int(5, 0), RoundingMode::NearestEven);
        assert!(r.is_zero() && !r.is_sign_negative());

        let (r, _) = Decimal32::ZERO.div(from_int(-5, 0), RoundingMode::NearestEven);
        assert!(r.is_zero() && r.is_sign_negative());
    }

    #[test]
    fn div_infinity() {
        // ∞ / 2 = ∞
        let (r, _) = Decimal32::INFINITY.div(from_int(2, 0), RoundingMode::NearestEven);
        assert!(r.is_infinite() && !r.is_sign_negative());

        // 5 / ∞ = +0
        let (r, _) = from_int(5, 0).div(Decimal32::INFINITY, RoundingMode::NearestEven);
        assert!(r.is_zero() && !r.is_sign_negative());

        // 5 / -∞ = -0
        let (r, _) = from_int(5, 0).div(Decimal32::NEG_INFINITY, RoundingMode::NearestEven);
        assert!(r.is_zero() && r.is_sign_negative());

        // ∞ / ∞ = NaN + INVALID
        let (r, s) = Decimal32::INFINITY.div(Decimal32::INFINITY, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());
    }

    #[test]
    fn div_nan_propagation() {
        let (r, s) = Decimal32::NAN.div(Decimal32::ONE, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.is_ok());

        let (r, s) = Decimal32::SIGNALING_NAN.div(Decimal32::ONE, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());
    }

    #[test]
    fn div_overflow() {
        // MAX / MIN_POSITIVE → overflow.
        let (r, s) = Decimal32::MAX.div(Decimal32::MIN_POSITIVE, RoundingMode::NearestEven);
        assert!(r.is_infinite());
        assert!(s.overflow() && s.inexact());
    }

    #[test]
    fn div_underflow() {
        // MIN_POSITIVE / MAX → underflow.
        let (r, s) = Decimal32::MIN_POSITIVE.div(Decimal32::MAX, RoundingMode::NearestEven);
        assert!(r.is_zero());
        assert!(s.inexact() && s.underflow());
    }
}
