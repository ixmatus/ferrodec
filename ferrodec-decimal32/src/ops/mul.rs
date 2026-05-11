//! IEEE 754-2019 multiply for [`Decimal32`].
//!
//! Returns `(Decimal32, Status)`. The finite path is straightforward
//! for Decimal32: both coefficients fit in `u32` (max `10⁷ − 1`), so
//! the product fits in `u64` (max ≈ `10¹⁴`) without multiword
//! machinery.
//!
//! # Special cases (IEEE 754-2019 §7)
//!
//! * sNaN in either operand → quiet NaN + `INVALID`.
//! * qNaN propagation (a preferred per §6.2.3).
//! * `0 × ±∞` and `±∞ × 0` → NaN + `INVALID`.
//! * `±∞ × finite` → `±∞` (sign by XOR of the two operand signs).
//! * `±∞ × ±∞` → `±∞` (sign by XOR).
//! * `0 × finite` and `finite × 0` → `±0` (sign by XOR), with the
//!   preferred quantum `exp_a + exp_b` clamped to the representable
//!   range (delegated to `round_and_pack_finite`'s zero branch).

use crate::bid::{classify_bits, Class, BIAS};
use crate::decimal::Decimal32;
use ferrodec_ieee::{RoundingMode, Status};

use super::round::round_and_pack_finite;

impl Decimal32 {
    /// IEEE 754-2019 `multiplication(self, other)` rounded by `rm`.
    #[must_use]
    pub fn mul(self, other: Self, rm: RoundingMode) -> (Self, Status) {
        let ca = classify_bits(self.0);
        let cb = classify_bits(other.0);

        if let Some(out) = handle_specials(ca, cb) {
            return out;
        }

        // Finite × finite (Zero × Finite handled via Class::Zero).
        let (sign_a, biased_a, coef_a) = match ca {
            Class::Finite {
                sign,
                biased_exp,
                coefficient,
            } => (sign, biased_exp, u64::from(coefficient)),
            Class::Zero { sign, biased_exp } => (sign, biased_exp, 0u64),
            _ => unreachable!("dispatcher handles non-finite"),
        };
        let (sign_b, biased_b, coef_b) = match cb {
            Class::Finite {
                sign,
                biased_exp,
                coefficient,
            } => (sign, biased_exp, u64::from(coefficient)),
            Class::Zero { sign, biased_exp } => (sign, biased_exp, 0u64),
            _ => unreachable!("dispatcher handles non-finite"),
        };

        let result_sign = sign_a ^ sign_b;
        let exp_a = biased_a as i32 - BIAS as i32;
        let exp_b = biased_b as i32 - BIAS as i32;
        let q_preferred = exp_a + exp_b;

        // u32 × u32 → u64; max product 9_999_999² < 10¹⁴ < 2⁴⁷ — well
        // within u64.
        let product = coef_a * coef_b;

        round_and_pack_finite(
            product,
            q_preferred,
            q_preferred,
            result_sign,
            false,
            rm,
            Status::OK,
        )
    }

    /// Kani-only entry point that returns the special-case branch only,
    /// without invoking the finite-finite product / rounding pipeline.
    /// Mirrors decimal128's `mul_special_only_for_kani` (ADR-0016).
    #[cfg(kani)]
    #[doc(hidden)]
    #[must_use]
    pub fn mul_special_only_for_kani(self, rhs: Self) -> Option<(Self, Status)> {
        handle_specials(classify_bits(self.0), classify_bits(rhs.0))
    }
}

fn handle_specials(a: Class, b: Class) -> Option<(Decimal32, Status)> {
    use Class::{Finite, Infinity, QuietNaN, SignalingNaN, Zero};

    // Signaling NaN propagation.
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

    // Quiet NaN propagation (a preferred).
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

    // 0 × ±∞ or ±∞ × 0 → NaN + INVALID.
    match (a, b) {
        (Zero { .. }, Infinity { .. }) | (Infinity { .. }, Zero { .. }) => {
            return Some((Decimal32::NAN, Status::INVALID));
        }
        _ => {}
    }

    // Any infinity remaining: result is ±∞ with XOR of signs.
    let (sa, sb) = match (a, b) {
        (Infinity { sign: sa }, Infinity { sign: sb }) => (Some(sa), Some(sb)),
        (Infinity { sign: sa }, Finite { sign: sb, .. }) => (Some(sa), Some(sb)),
        (Finite { sign: sa, .. }, Infinity { sign: sb }) => (Some(sa), Some(sb)),
        _ => (None, None),
    };
    if let (Some(sa), Some(sb)) = (sa, sb) {
        return Some((
            Decimal32::from_bits(crate::bid::pack_infinity(sa ^ sb)),
            Status::OK,
        ));
    }

    // No NaNs, no infinities, no infinity-zero collision: fall through
    // to the finite path.
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
    fn mul_basic() {
        let (r, s) = from_int(2, 0).mul(from_int(3, 0), RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), from_int(6, 0).to_bits());
        assert!(s.is_ok());

        let (r, _) = from_int(123, 0).mul(from_int(2, 0), RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), from_int(246, 0).to_bits());
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
        // 1.5 × 2.0 = 3.00 (q_preferred = -1 + -1 = -2). Coefficient
        // 15 × 20 = 300; padded to PRECISION on the inexact-pad path
        // would not apply here since the result is exact, so the
        // strip-up path may shift to remove trailing zeros... actually
        // strip-up only fires when exp_after < q_preferred, but here
        // exp_after = q_preferred, so the value is preserved at q=-2:
        // "300 × 10^-2" = "3.00".
        let (r, _) = from_int(15, -1).mul(from_int(20, -1), RoundingMode::NearestEven);
        let expected = Decimal32::from_bits(pack_finite(false, BIAS - 2, 300));
        assert_eq!(r.to_bits(), expected.to_bits());
    }

    #[test]
    fn mul_seven_digits_full_precision() {
        // 9_999_999 × 1 = 9_999_999 (exact, fits in 7 digits).
        let (r, s) = from_int(9_999_999, 0).mul(from_int(1, 0), RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), from_int(9_999_999, 0).to_bits());
        assert!(s.is_ok());
    }

    #[test]
    fn mul_inexact_rounds() {
        // 1234567 × 1234567 = 1_524_155_677_489 (13 digits). Round to
        // 7 → 1524156 × 10^6.
        let (r, s) = from_int(1_234_567, 0).mul(from_int(1_234_567, 0), RoundingMode::NearestEven);
        let expected = Decimal32::from_bits(pack_finite(false, BIAS + 6, 1_524_156));
        assert_eq!(r.to_bits(), expected.to_bits());
        assert!(s.inexact());
    }

    #[test]
    fn mul_zero() {
        let (r, _) = from_int(5, 0).mul(Decimal32::ZERO, RoundingMode::NearestEven);
        assert!(r.is_zero() && !r.is_sign_negative());

        let (r, _) = from_int(-5, 0).mul(Decimal32::ZERO, RoundingMode::NearestEven);
        assert!(r.is_zero() && r.is_sign_negative());
    }

    #[test]
    fn mul_overflow() {
        // MAX × 10 → overflow.
        let (r, s) = Decimal32::MAX.mul(from_int(10, 0), RoundingMode::NearestEven);
        assert!(r.is_infinite() && !r.is_sign_negative());
        assert!(s.overflow() && s.inexact());
    }

    #[test]
    fn mul_underflow() {
        // MIN_POSITIVE × 0.1 → underflows to 0.
        let (r, s) = Decimal32::MIN_POSITIVE.mul(from_int(1, -1), RoundingMode::NearestEven);
        assert!(r.is_zero());
        assert!(s.inexact() && s.underflow());
    }

    #[test]
    fn mul_nan_propagation() {
        let (r, s) = Decimal32::NAN.mul(Decimal32::ONE, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.is_ok());

        let (r, s) = Decimal32::SIGNALING_NAN.mul(Decimal32::ONE, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());
    }

    #[test]
    fn mul_infinity() {
        // ∞ × 2 = ∞
        let (r, s) = Decimal32::INFINITY.mul(from_int(2, 0), RoundingMode::NearestEven);
        assert!(r.is_infinite() && !r.is_sign_negative());
        assert!(s.is_ok());

        // ∞ × −2 = −∞
        let (r, _) = Decimal32::INFINITY.mul(from_int(-2, 0), RoundingMode::NearestEven);
        assert!(r.is_infinite() && r.is_sign_negative());

        // ∞ × ∞ = ∞
        let (r, _) = Decimal32::INFINITY.mul(Decimal32::INFINITY, RoundingMode::NearestEven);
        assert!(r.is_infinite() && !r.is_sign_negative());

        // ∞ × −∞ = −∞
        let (r, _) = Decimal32::INFINITY.mul(Decimal32::NEG_INFINITY, RoundingMode::NearestEven);
        assert!(r.is_infinite() && r.is_sign_negative());
    }

    #[test]
    fn mul_infinity_zero_invalid() {
        // 0 × ∞ = NaN + INVALID
        let (r, s) = Decimal32::ZERO.mul(Decimal32::INFINITY, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());

        let (r, s) = Decimal32::INFINITY.mul(Decimal32::NEG_ZERO, RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());
    }
}
