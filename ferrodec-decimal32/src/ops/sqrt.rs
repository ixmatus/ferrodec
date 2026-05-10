//! IEEE 754-2019 square root for [`Decimal32`].
//!
//! Returns `(Decimal32, Status)`. The finite path scales the coefficient
//! to 15 or 16 decimal digits so the integer square root falls in
//! `[10⁷, 10⁸)` (= 8 digits, one above PRECISION for correct rounding),
//! then routes through `round_and_pack_finite` with the
//! squared-back-residue feeding the rounding sticky bit.
//!
//! # Special cases (IEEE 754-2019 §5.4.1)
//!
//! * sNaN → quiet NaN + `INVALID`.
//! * qNaN → propagated quietly.
//! * `sqrt(−finite)` (finite ≠ −0) → NaN + `INVALID`.
//! * `sqrt(±0)` → ±0 (sign preserved per IEEE 754, even for −0).
//! * `sqrt(+∞)` → +∞.
//! * `sqrt(−∞)` → NaN + `INVALID`.
//!
//! The preferred quantum per §6.3 is `floor(Q(x) / 2)`. The pack
//! routine pads or strips toward this quantum.

use crate::bid::{classify_bits, decimal_digit_count, BIAS, Class};
use crate::decimal::Decimal32;
use crate::status::{RoundingMode, Status};

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
    /// IEEE 754-2019 `squareRoot(self)` rounded by `rm`.
    #[must_use]
    pub fn sqrt(self, rm: RoundingMode) -> (Self, Status) {
        let class = classify_bits(self.0);

        match class {
            Class::SignalingNaN { sign, payload } => (
                Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::INVALID,
            ),
            Class::QuietNaN { sign, payload } => (
                Decimal32::from_bits(crate::bid::pack_quiet_nan(sign, payload)),
                Status::OK,
            ),
            Class::Infinity { sign: false } => (Decimal32::INFINITY, Status::OK),
            Class::Infinity { sign: true } => (Decimal32::NAN, Status::INVALID),
            Class::Zero { sign, biased_exp } => {
                // sqrt(±0) = ±0 (sign preserved). Quantum = floor(Q/2).
                let exp = biased_exp as i32 - BIAS as i32;
                let q = exp.div_euclid(2);
                (
                    Decimal32::from_bits(crate::bid::pack_finite(
                        sign,
                        (q + BIAS as i32) as u32,
                        0,
                    )),
                    Status::OK,
                )
            }
            Class::Finite { sign: true, .. } => {
                // sqrt(negative finite) → NaN + INVALID.
                (Decimal32::NAN, Status::INVALID)
            }
            Class::Finite {
                sign: false,
                biased_exp,
                coefficient,
            } => sqrt_positive_finite(u64::from(coefficient), biased_exp, rm),
        }
    }
}

fn sqrt_positive_finite(
    coef: u64,
    biased_exp: u32,
    rm: RoundingMode,
) -> (Decimal32, Status) {
    let exp = biased_exp as i32 - BIAS as i32;
    let q_preferred = exp.div_euclid(2);

    // Make the working exponent even so that the half-exponent is well-
    // defined. If exp is odd, multiply coefficient by 10 and decrement
    // exp by 1.
    let (mut working_coef, mut working_exp) = if exp & 1 != 0 {
        (coef * 10, exp - 1)
    } else {
        (coef, exp)
    };

    // Scale further so working_coef has 15 or 16 digits — isqrt then
    // lands in [10⁷, 10⁸) (= 8 digits, one above PRECISION for correct
    // rounding). The shift's parity matches working_coef's digit count
    // parity so working_exp - scale stays even.
    let d = decimal_digit_count(working_coef as u32);
    let scale = if d % 2 == 0 { 16 - d } else { 15 - d };
    debug_assert!(scale % 2 == 0);
    debug_assert!((scale as usize) < POW10_U64.len());

    working_coef *= POW10_U64[scale as usize];
    working_exp -= scale as i32;

    // working_exp is now even (subtracting an even scale from an even
    // working_exp).
    debug_assert!(working_exp & 1 == 0);

    let isqrt_val = working_coef.isqrt();
    let isqrt_squared = isqrt_val * isqrt_val;
    let sticky = isqrt_squared != working_coef;

    let result_exp = working_exp / 2;

    round_and_pack_finite(
        isqrt_val,
        result_exp,
        q_preferred,
        false,
        sticky,
        rm,
        Status::OK,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bid::pack_finite;

    fn from_int(n: i32, exp: i32) -> Decimal32 {
        Decimal32::try_new(n, exp).unwrap()
    }

    #[test]
    fn sqrt_perfect_squares() {
        // sqrt(4) = 2
        let (r, s) = from_int(4, 0).sqrt(RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), from_int(2, 0).to_bits());
        assert!(s.is_ok());

        // sqrt(9) = 3
        let (r, _) = from_int(9, 0).sqrt(RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), from_int(3, 0).to_bits());

        // sqrt(100) = 10
        let (r, _) = from_int(100, 0).sqrt(RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), from_int(10, 0).to_bits());

        // sqrt(10000) = 100
        let (r, _) = from_int(10_000, 0).sqrt(RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), from_int(100, 0).to_bits());
    }

    #[test]
    fn sqrt_inexact_two() {
        // sqrt(2) ≈ 1.4142136 (NearestEven, 7 digits).
        let (r, s) = from_int(2, 0).sqrt(RoundingMode::NearestEven);
        let expected = Decimal32::from_bits(pack_finite(false, BIAS - 6, 1_414_214));
        // sqrt(2) = 1.4142135623... → 7-digit nearest-even = 1.414214.
        assert_eq!(r.to_bits(), expected.to_bits());
        assert!(s.inexact());
    }

    #[test]
    fn sqrt_zero() {
        let (r, s) = Decimal32::ZERO.sqrt(RoundingMode::NearestEven);
        assert!(r.is_zero() && !r.is_sign_negative());
        assert!(s.is_ok());

        // sqrt(-0) = -0 (sign preserved per IEEE 754).
        let (r, s) = Decimal32::NEG_ZERO.sqrt(RoundingMode::NearestEven);
        assert!(r.is_zero() && r.is_sign_negative());
        assert!(s.is_ok());
    }

    #[test]
    fn sqrt_one() {
        let (r, _) = Decimal32::ONE.sqrt(RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), Decimal32::ONE.to_bits());
    }

    #[test]
    fn sqrt_negative_invalid() {
        let (r, s) = from_int(-4, 0).sqrt(RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());

        let (r, s) = Decimal32::NEG_INFINITY.sqrt(RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());
    }

    #[test]
    fn sqrt_infinity() {
        let (r, s) = Decimal32::INFINITY.sqrt(RoundingMode::NearestEven);
        assert!(r.is_infinite() && !r.is_sign_negative());
        assert!(s.is_ok());
    }

    #[test]
    fn sqrt_nan_propagation() {
        let (r, s) = Decimal32::NAN.sqrt(RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.is_ok());

        let (r, s) = Decimal32::SIGNALING_NAN.sqrt(RoundingMode::NearestEven);
        assert!(r.is_quiet_nan());
        assert!(s.invalid());
    }

    #[test]
    fn sqrt_with_negative_exponent() {
        // sqrt(0.04) = 0.2
        let x = from_int(4, -2);
        let (r, _) = x.sqrt(RoundingMode::NearestEven);
        let expected = Decimal32::from_bits(pack_finite(false, BIAS - 1, 2));
        assert_eq!(r.to_bits(), expected.to_bits());
    }

    #[test]
    fn sqrt_large_exponent() {
        // sqrt(1e96) = 1e48
        let x = Decimal32::try_new(1_000_000, 90).unwrap(); // 1 × 10^96 represented at biased_exp=191
        let (r, _) = x.sqrt(RoundingMode::NearestEven);
        // sqrt(10^96) = 10^48. Our result preserves cohort selection;
        // verify by checking the encoded value is finite and equal to
        // 10^48 numerically.
        assert!(r.is_finite() && !r.is_zero());
        // 10^48 = 1 × 10^48 in canonical cohort: pack_finite(false, BIAS+48, 1)
        // but cohort can be (10^7) × 10^41 = 10^48 too. Just check magnitude.
        let class = crate::bid::classify_bits(r.to_bits());
        match class {
            Class::Finite { sign, biased_exp, coefficient } => {
                assert!(!sign);
                let unbiased = biased_exp as i32 - BIAS as i32;
                let value_log10 = unbiased + decimal_digit_count(coefficient) as i32 - 1;
                assert_eq!(value_log10, 48, "sqrt(10^96) should have log10 = 48");
            }
            _ => panic!("expected Finite, got {class:?}"),
        }
    }

    #[test]
    fn sqrt_perfect_square_at_seven_digits() {
        // 9_999_999² = 99_999_980_000_001, but that doesn't fit in u32.
        // Use 3_162_277² = 9_999_995_032_729 — close to but not 10^7.
        // Try 1234² = 1_522_756.
        let x = from_int(1_522_756, 0);
        let (r, s) = x.sqrt(RoundingMode::NearestEven);
        assert_eq!(r.to_bits(), from_int(1234, 0).to_bits());
        assert!(s.is_ok());
    }
}
