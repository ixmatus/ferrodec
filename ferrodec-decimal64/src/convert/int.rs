//! [`Decimal64`] → integer conversions.
//!
//! Every conversion takes a [`RoundingMode`] and returns
//! `(int, Status)`, following IEEE 754-2019 §5.4.1 convertToInteger:
//!
//! * NaN → `0` with `INVALID`.
//! * `±∞` → `MIN` / `MAX` (signed) or `0` / `MAX` (unsigned) with
//!   `INVALID`.
//! * Out-of-range finite → clamped to `MIN` / `MAX` with `INVALID`.
//! * In-range non-integer → rounded per `rm`, with `INEXACT`.
//! * In-range integer → exact, no flags.
//!
//! The arithmetic stays in the decimal domain: the coefficient
//! (`u64`, at most 16 digits) is scaled by an integer power of ten in
//! `u128` and rounded with the shared
//! [`ferrodec_ieee::should_round_up`] rule. The earlier
//! `ToPrimitive` path went through `f64`, which is exact only up to
//! `2^53`; a Decimal64 integer in `(2^53, 10^16)` (or any power of
//! ten above `1e15`) lost its low bits. Routing through `f64` is the
//! M5 bug this module closes.
//!
//! Any Decimal64 magnitude at or above `10^39` exceeds `u128::MAX`,
//! so it is out of range for every target here and clamps with
//! `INVALID`; that bound is detected with `checked_mul`, never by an
//! f64 round trip.

use crate::bid::{classify_bits, decimal_digit_count, Class, BIAS};
use crate::decimal::Decimal64;
use ferrodec_ieee::{should_round_up, RoundingMode, Status};

impl Decimal64 {
    /// Convert to `i32`, rounding by `rm`. The module documentation
    /// states the full `(int, Status)` contract.
    #[must_use]
    pub fn to_i32(self, rm: RoundingMode) -> (i32, Status) {
        let (n, s) = self.to_signed(rm, i128::from(i32::MIN), i128::from(i32::MAX));
        (n as i32, s)
    }

    /// Convert to `i64`, rounding by `rm`.
    #[must_use]
    pub fn to_i64(self, rm: RoundingMode) -> (i64, Status) {
        let (n, s) = self.to_signed(rm, i128::from(i64::MIN), i128::from(i64::MAX));
        (n as i64, s)
    }

    /// Convert to `i128`, rounding by `rm`.
    #[must_use]
    pub fn to_i128(self, rm: RoundingMode) -> (i128, Status) {
        self.to_signed(rm, i128::MIN, i128::MAX)
    }

    /// Convert to `u32`, rounding by `rm`.
    #[must_use]
    pub fn to_u32(self, rm: RoundingMode) -> (u32, Status) {
        let (n, s) = self.to_unsigned(rm, u128::from(u32::MAX));
        (n as u32, s)
    }

    /// Convert to `u64`, rounding by `rm`.
    #[must_use]
    pub fn to_u64(self, rm: RoundingMode) -> (u64, Status) {
        let (n, s) = self.to_unsigned(rm, u128::from(u64::MAX));
        (n as u64, s)
    }

    /// Convert to `u128`, rounding by `rm`.
    #[must_use]
    pub fn to_u128(self, rm: RoundingMode) -> (u128, Status) {
        self.to_unsigned(rm, u128::MAX)
    }

    fn to_signed(self, rm: RoundingMode, min: i128, max: i128) -> (i128, Status) {
        match classify_bits(self.to_bits()) {
            Class::QuietNaN { .. } | Class::SignalingNaN { .. } => (0, Status::INVALID),
            Class::Infinity { sign } => (if sign { min } else { max }, Status::INVALID),
            Class::Zero { .. } => (0, Status::OK),
            Class::Finite {
                sign,
                biased_exp,
                coefficient,
            } => {
                let unbiased = biased_exp as i32 - BIAS as i32;
                let (abs, status) = round_to_integer(coefficient, unbiased, sign, rm);
                if abs == 0 {
                    return (0, status);
                }
                if sign {
                    let abs_min = min.unsigned_abs();
                    if abs > abs_min {
                        return (min, Status::INVALID);
                    }
                    if abs == abs_min {
                        return (min, status);
                    }
                    (-(abs as i128), status)
                } else {
                    if abs > max as u128 {
                        return (max, Status::INVALID);
                    }
                    (abs as i128, status)
                }
            }
        }
    }

    fn to_unsigned(self, rm: RoundingMode, max: u128) -> (u128, Status) {
        match classify_bits(self.to_bits()) {
            Class::QuietNaN { .. } | Class::SignalingNaN { .. } => (0, Status::INVALID),
            Class::Infinity { sign } => {
                if sign {
                    (0, Status::INVALID)
                } else {
                    (max, Status::INVALID)
                }
            }
            Class::Zero { .. } => (0, Status::OK),
            Class::Finite {
                sign,
                biased_exp,
                coefficient,
            } => {
                let unbiased = biased_exp as i32 - BIAS as i32;
                let (abs, status) = round_to_integer(coefficient, unbiased, sign, rm);
                if sign {
                    // §5.4.1: a negative value that rounds to zero
                    // (e.g. −0.4 under any nearest mode) is in range
                    // and yields 0 + INEXACT, not INVALID. Any other
                    // negative is out of range for an unsigned type.
                    if abs == 0 {
                        return (0, status);
                    }
                    return (0, Status::INVALID);
                }
                if abs > max {
                    return (max, Status::INVALID);
                }
                (abs, status)
            }
        }
    }
}

/// Round `coef × 10^unbiased` to a non-negative integer magnitude,
/// applying `rm` and raising `INEXACT` when any digit is dropped.
///
/// Returns `(rounded_abs, status)`; the caller applies the sign and
/// the range check. An out-of-range magnitude returns `u128::MAX`
/// with `INVALID` so the caller's range test clamps it.
fn round_to_integer(coef: u64, unbiased: i32, sign: bool, rm: RoundingMode) -> (u128, Status) {
    debug_assert!(
        coef != 0,
        "classify_bits routes a zero coefficient to Class::Zero"
    );
    if unbiased >= 0 {
        let shift = unbiased as u32;
        // 10^39 already exceeds u128::MAX, and with a 16-digit
        // coefficient even smaller shifts overflow; `checked_mul` is
        // the exact gate.
        if shift > 38 {
            return (u128::MAX, Status::INVALID);
        }
        return match u128::from(coef).checked_mul(10u128.pow(shift)) {
            Some(v) => (v, Status::OK),
            None => (u128::MAX, Status::INVALID),
        };
    }
    // unbiased < 0: the value is coef × 10^-drop. The integer part is
    // floor(coef / 10^drop); the digit at the tenths place (coef
    // position drop−1) is the round digit, everything below it is
    // sticky.
    let drop = (-unbiased) as u32;
    let digits = decimal_digit_count(coef);
    if drop >= digits {
        // |value| < 1, so the integer part is 0. When drop == digits
        // the round digit is coef's leading digit; when drop > digits
        // the round position sits above coef entirely, so the round
        // digit is 0 and the whole (non-zero) coefficient is sticky.
        let (round_digit, sticky) = if drop == digits {
            let lead_pow = 10u64.pow(digits - 1);
            ((coef / lead_pow) as u32, coef % lead_pow != 0)
        } else {
            (0u32, true)
        };
        let round_up = should_round_up(rm, sign, 0, round_digit, sticky);
        let mut status = Status::OK;
        if round_digit != 0 || sticky {
            status |= Status::INEXACT;
        }
        return (u128::from(round_up), status);
    }
    // drop < digits ≤ 16, so 10^drop ≤ 10^15 fits in u64.
    let divisor = 10u64.pow(drop);
    let int_part = u128::from(coef / divisor);
    let frac = coef % divisor;
    let (round_digit, sticky) = if drop == 0 {
        (0u32, false)
    } else {
        let half_pow = 10u64.pow(drop - 1);
        (((frac / half_pow) % 10) as u32, frac % half_pow != 0)
    };
    let last_kept = (int_part % 10) as u32;
    let round_up = should_round_up(rm, sign, last_kept, round_digit, sticky);
    let rounded = if round_up {
        int_part.saturating_add(1)
    } else {
        int_part
    };
    let mut status = Status::OK;
    if round_digit != 0 || sticky {
        status |= Status::INEXACT;
    }
    (rounded, status)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dec(coef: i64, exp: i32) -> Decimal64 {
        Decimal64::try_new(coef, exp).unwrap()
    }

    #[test]
    fn exact_beyond_f64_mantissa() {
        // The whole point of M5: integers in (2^53, 10^16) are exact
        // in Decimal64 but not in f64. The old to_f64 route returned
        // a value off by a few ULPs; the decimal path is exact.
        let big = dec(9_999_999_999_999_999, 0); // 16 nines, > 2^53
        assert_eq!(
            big.to_i64(RoundingMode::NearestEven),
            (9_999_999_999_999_999, Status::OK)
        );
        assert_eq!(
            big.to_u64(RoundingMode::NearestEven),
            (9_999_999_999_999_999, Status::OK)
        );

        // 1e18: a power of ten f64 cannot hold exactly (needs ~60
        // bits), well inside i64.
        let p18 = dec(1, 18);
        assert_eq!(
            p18.to_i64(RoundingMode::NearestEven),
            (1_000_000_000_000_000_000, Status::OK)
        );
    }

    #[test]
    fn small_integers_and_sign() {
        assert_eq!(
            dec(42, 0).to_i64(RoundingMode::NearestEven),
            (42, Status::OK)
        );
        assert_eq!(
            dec(-3, 0).to_i64(RoundingMode::NearestEven),
            (-3, Status::OK)
        );
        assert_eq!(dec(0, 0).to_i64(RoundingMode::NearestEven), (0, Status::OK));
        // Trailing-zero cohort: 4500 × 10^-2 == 45 exactly.
        assert_eq!(
            dec(4500, -2).to_i64(RoundingMode::NearestEven),
            (45, Status::OK)
        );
    }

    #[test]
    fn fractional_rounding_modes() {
        let two_five = dec(25, -1); // 2.5
        assert_eq!(
            two_five.to_i64(RoundingMode::NearestEven).0,
            2,
            "2.5 rounds to even (2)"
        );
        assert_eq!(
            two_five.to_i64(RoundingMode::NearestAway).0,
            3,
            "2.5 rounds away (3)"
        );
        assert_eq!(two_five.to_i64(RoundingMode::TowardZero).0, 2);
        assert_eq!(dec(35, -1).to_i64(RoundingMode::NearestEven).0, 4); // 3.5 → 4
        let (n, s) = dec(27, -1).to_i64(RoundingMode::NearestEven); // 2.7
        assert_eq!(n, 3);
        assert!(s.inexact());
        // |value| < 1.
        assert_eq!(dec(4, -1).to_i64(RoundingMode::NearestEven).0, 0); // 0.4 → 0
        assert_eq!(dec(7, -1).to_i64(RoundingMode::NearestEven).0, 1); // 0.7 → 1
        assert_eq!(dec(7, -3).to_i64(RoundingMode::NearestEven).0, 0); // 0.007 → 0
                                                                       // Negative toward-negative rounds away from zero.
        assert_eq!(dec(-12, -1).to_i64(RoundingMode::TowardNegative).0, -2);
    }

    #[test]
    fn unsigned_negative_handling() {
        // −0.4 rounds to 0: in range, INEXACT, not INVALID.
        let (n, s) = dec(-4, -1).to_u64(RoundingMode::NearestEven);
        assert_eq!(n, 0);
        assert!(!s.invalid() && s.inexact());
        // −3 is genuinely out of range for u64.
        let (n, s) = dec(-3, 0).to_u64(RoundingMode::NearestEven);
        assert_eq!(n, 0);
        assert!(s.invalid());
    }

    #[test]
    fn out_of_range_clamps_invalid() {
        // 1e30 is representable in Decimal64 but far above i64::MAX.
        let huge = dec(1, 30);
        let (n, s) = huge.to_i64(RoundingMode::NearestEven);
        assert_eq!(n, i64::MAX);
        assert!(s.invalid());
        // ...yet fits i128 exactly.
        let (n, s) = huge.to_i128(RoundingMode::NearestEven);
        assert_eq!(n, 1_000_000_000_000_000_000_000_000_000_000_i128);
        assert!(!s.invalid());
        // 1e40 > u128::MAX: out of range even for u128 (shift > 38).
        let (n, s) = dec(1, 40).to_u128(RoundingMode::NearestEven);
        assert_eq!(n, u128::MAX);
        assert!(s.invalid());
        // Negative overflow clamps to MIN.
        let (n, s) = dec(-1, 30).to_i64(RoundingMode::NearestEven);
        assert_eq!(n, i64::MIN);
        assert!(s.invalid());
    }

    #[test]
    fn specials() {
        assert_eq!(
            Decimal64::NAN.to_i64(RoundingMode::NearestEven),
            (0, Status::INVALID)
        );
        let (n, s) = Decimal64::INFINITY.to_i64(RoundingMode::NearestEven);
        assert_eq!(n, i64::MAX);
        assert!(s.invalid());
        let (n, s) = Decimal64::NEG_INFINITY.to_i64(RoundingMode::NearestEven);
        assert_eq!(n, i64::MIN);
        assert!(s.invalid());
        let (n, s) = Decimal64::NEG_INFINITY.to_u64(RoundingMode::NearestEven);
        assert_eq!(n, 0);
        assert!(s.invalid());
        let (n, s) = Decimal64::INFINITY.to_u64(RoundingMode::NearestEven);
        assert_eq!(n, u64::MAX);
        assert!(s.invalid());
    }
}
