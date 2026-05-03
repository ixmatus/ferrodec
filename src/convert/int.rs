//! Integer ↔ [`Decimal128`](crate::Decimal128) conversions.
//!
//! ## From-integer
//!
//! Every standard signed and unsigned integer up through 64 bits fits
//! exactly in `Decimal128` (coefficient ≤ `2^64 < 10^20 < 10^34`), so
//! the from-conversions are infallible and rounding-free. We expose
//! them as inherent `from_*` constructors and [`From`] impls.
//!
//! `i128` and `u128` can exceed the 34-digit precision (anything above
//! `10^34` rounds), so they take a [`RoundingMode`] and return
//! `(Decimal128, Status)`.
//!
//! ## To-integer
//!
//! Going the other way, every conversion takes a [`RoundingMode`] and
//! returns `(int, Status)`:
//!
//! * NaN → `0` with `INVALID`.
//! * `±∞` → `i*::MAX` / `i*::MIN` (or `u*::MAX` / `0`) with `INVALID`.
//! * Out-of-range finite → clamped to `MIN`/`MAX` with `INVALID`.
//! * In-range non-integer-valued → rounded per `rm`, with `INEXACT`.
//! * In-range integer-valued → exact, no flags.

use crate::bid::{
    classify_bits, decimal_digit_count, pack_finite, Class, BIAS, COEFFICIENT_LIMIT,
    COEFFICIENT_FIELD_LIMIT, PRECISION,
};
use crate::decimal::Decimal128;
use crate::multiword::U256;
use crate::ops::round_and_pack_finite;
use crate::status::{RoundingMode, Status};

// ---------------------------------------------------------------------------
// From-integer

impl Decimal128 {
    /// Exact `i32` → `Decimal128`. Quantum is `0`.
    #[inline]
    #[must_use]
    pub const fn from_i32(n: i32) -> Self {
        from_signed_small(n as i64)
    }

    /// Exact `u32` → `Decimal128`. Quantum is `0`.
    #[inline]
    #[must_use]
    pub const fn from_u32(n: u32) -> Self {
        from_unsigned_small(n as u128)
    }

    /// Exact `i64` → `Decimal128`. Quantum is `0`.
    #[inline]
    #[must_use]
    pub const fn from_i64(n: i64) -> Self {
        from_signed_small(n)
    }

    /// Exact `u64` → `Decimal128`. Quantum is `0`.
    #[inline]
    #[must_use]
    pub const fn from_u64(n: u64) -> Self {
        from_unsigned_small(n as u128)
    }

    /// `i128` → `Decimal128`, possibly rounded.
    ///
    /// `|n| < 10^34` is exact. Above that, the rounding mode and
    /// `INEXACT` flag describe how the low digits were dropped.
    #[must_use]
    pub fn from_i128(n: i128, rm: RoundingMode) -> (Self, Status) {
        if n == 0 {
            return (Self::ZERO, Status::OK);
        }
        let sign = n < 0;
        let abs: u128 = n.unsigned_abs();
        from_unsigned_with_rounding(sign, abs, rm)
    }

    /// `u128` → `Decimal128`, possibly rounded.
    #[must_use]
    pub fn from_u128(n: u128, rm: RoundingMode) -> (Self, Status) {
        if n == 0 {
            return (Self::ZERO, Status::OK);
        }
        from_unsigned_with_rounding(false, n, rm)
    }
}

impl From<i32> for Decimal128 {
    #[inline]
    fn from(n: i32) -> Self {
        Self::from_i32(n)
    }
}

impl From<u32> for Decimal128 {
    #[inline]
    fn from(n: u32) -> Self {
        Self::from_u32(n)
    }
}

impl From<i64> for Decimal128 {
    #[inline]
    fn from(n: i64) -> Self {
        Self::from_i64(n)
    }
}

impl From<u64> for Decimal128 {
    #[inline]
    fn from(n: u64) -> Self {
        Self::from_u64(n)
    }
}

#[inline]
const fn from_signed_small(n: i64) -> Decimal128 {
    if n == 0 {
        return Decimal128::ZERO;
    }
    let sign = n < 0;
    // i64::MIN's absolute value doesn't fit in i64; widen to i128 first.
    let abs = (n as i128).unsigned_abs();
    Decimal128::from_bits(pack_finite(sign, BIAS, abs))
}

#[inline]
const fn from_unsigned_small(n: u128) -> Decimal128 {
    if n == 0 {
        return Decimal128::ZERO;
    }
    debug_assert!(n < COEFFICIENT_FIELD_LIMIT);
    Decimal128::from_bits(pack_finite(false, BIAS, n))
}

fn from_unsigned_with_rounding(
    sign: bool,
    abs: u128,
    rm: RoundingMode,
) -> (Decimal128, Status) {
    if abs < COEFFICIENT_LIMIT {
        // Fits exactly in 34 digits.
        return (
            Decimal128::from_bits(pack_finite(sign, BIAS, abs)),
            Status::OK,
        );
    }
    // Round through the shared pipeline. Quantum is 0; the rounder
    // drops excess digits and renormalises.
    round_and_pack_finite(
        U256::from_u128(abs),
        0,
        sign,
        false,
        rm,
        Status::OK,
    )
}

// ---------------------------------------------------------------------------
// To-integer

impl Decimal128 {
    /// `Decimal128` → `i32`, with rounding.
    #[must_use]
    pub fn to_i32(self, rm: RoundingMode) -> (i32, Status) {
        let (n, s) = self.to_signed(rm, i32::MIN as i128, i32::MAX as i128);
        (n as i32, s)
    }

    /// `Decimal128` → `i64`, with rounding.
    #[must_use]
    pub fn to_i64(self, rm: RoundingMode) -> (i64, Status) {
        let (n, s) = self.to_signed(rm, i64::MIN as i128, i64::MAX as i128);
        (n as i64, s)
    }

    /// `Decimal128` → `i128`, with rounding.
    #[must_use]
    pub fn to_i128(self, rm: RoundingMode) -> (i128, Status) {
        self.to_signed(rm, i128::MIN, i128::MAX)
    }

    /// `Decimal128` → `u32`, with rounding.
    #[must_use]
    pub fn to_u32(self, rm: RoundingMode) -> (u32, Status) {
        let (n, s) = self.to_unsigned(rm, u32::MAX as u128);
        (n as u32, s)
    }

    /// `Decimal128` → `u64`, with rounding.
    #[must_use]
    pub fn to_u64(self, rm: RoundingMode) -> (u64, Status) {
        let (n, s) = self.to_unsigned(rm, u64::MAX as u128);
        (n as u64, s)
    }

    /// `Decimal128` → `u128`, with rounding.
    #[must_use]
    pub fn to_u128(self, rm: RoundingMode) -> (u128, Status) {
        self.to_unsigned(rm, u128::MAX)
    }

    fn to_signed(self, rm: RoundingMode, min: i128, max: i128) -> (i128, Status) {
        match classify_bits(self.to_bits()) {
            Class::QuietNaN { .. } | Class::SignalingNaN { .. } => (0, Status::INVALID),
            Class::Infinity { sign } => {
                let clamped = if sign { min } else { max };
                (clamped, Status::INVALID)
            }
            Class::Zero { .. } => (0, Status::OK),
            Class::Finite {
                sign,
                biased_exp,
                coefficient,
            } => {
                let unbiased = biased_exp as i32 - BIAS as i32;
                let (rounded_abs, status) = round_to_integer(coefficient, unbiased, sign, rm);
                if rounded_abs == 0 {
                    return (0, status);
                }
                if sign {
                    // Negative: |n| ≤ |min|.
                    let abs_min = min.unsigned_abs();
                    if rounded_abs > abs_min {
                        return (min, Status::INVALID);
                    }
                    if rounded_abs == abs_min {
                        return (min, status);
                    }
                    let neg = -(rounded_abs as i128);
                    (neg, status)
                } else {
                    if rounded_abs > max as u128 {
                        return (max, Status::INVALID);
                    }
                    (rounded_abs as i128, status)
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
                if sign {
                    // Any negative value is out-of-range for unsigned.
                    return (0, Status::INVALID);
                }
                let unbiased = biased_exp as i32 - BIAS as i32;
                let (rounded, status) = round_to_integer(coefficient, unbiased, false, rm);
                if rounded > max {
                    return (max, Status::INVALID);
                }
                (rounded, status)
            }
        }
    }
}

/// Round `coef × 10^unbiased` to an unsigned integer, applying the IEEE
/// rounding mode and emitting `INEXACT` when digits are dropped.
///
/// Returns `(rounded_abs, status)`. Caller applies the sign separately.
fn round_to_integer(
    coef: u128,
    unbiased: i32,
    sign: bool,
    rm: RoundingMode,
) -> (u128, Status) {
    if unbiased >= 0 {
        // Integer or larger: shift the coefficient up. May overflow u128
        // for very-large unbiased exponents — caller will catch via
        // out-of-range detection.
        let mut value = U256::from_u128(coef);
        let shift = unbiased as u32;
        if shift > 38 {
            // 10^39 already exceeds u128. The integer is unrepresentable;
            // return u128::MAX to force the out-of-range flag at the caller.
            return (u128::MAX, Status::INVALID);
        }
        value = value.mul_pow10(shift);
        if value.hi != 0 {
            return (u128::MAX, Status::INVALID);
        }
        return (value.lo, Status::OK);
    }
    // unbiased < 0 — fractional: drop digits with rounding.
    let drop = (-unbiased) as u32;
    let digits = decimal_digit_count(coef);
    if drop >= digits {
        // |value| < 1 — the integer part is 0 or ±1 depending on rm.
        // Construct round / sticky from the entire coefficient.
        let mut sticky = false;
        let mut round_digit = 0u32;
        let mut cur = coef;
        let mut i = 0u32;
        while i < drop {
            let r = (cur % 10) as u32;
            if i == drop - 1 {
                round_digit = r;
            } else if r != 0 {
                sticky = true;
            }
            cur /= 10;
            i += 1;
            if cur == 0 && i + 1 < drop {
                // Remaining digits all-zero from here.
                if i < drop - 1 {
                    // round_digit stays 0; sticky already accumulated.
                }
                break;
            }
        }
        debug_assert!(cur == 0);
        let last_kept = 0u32;
        let round_up = should_round_up_int(rm, sign, last_kept, round_digit, sticky);
        let mut status = Status::OK;
        if round_digit != 0 || sticky {
            status |= Status::INEXACT;
        }
        return (if round_up { 1 } else { 0 }, status);
    }
    // drop < digits: extract integer part and round.
    let divisor = 10u128.pow(drop);
    let int_part = coef / divisor;
    let frac = coef - int_part * divisor;
    let round_digit = if drop == 0 {
        0
    } else {
        ((frac / 10u128.pow(drop - 1)) as u32) % 10
    };
    let sticky = if drop <= 1 { false } else { (frac % 10u128.pow(drop - 1)) != 0 };
    let last_kept = (int_part % 10) as u32;
    let mut rounded = int_part;
    let round_up = should_round_up_int(rm, sign, last_kept, round_digit, sticky);
    if round_up {
        rounded = rounded.saturating_add(1);
    }
    let mut status = Status::OK;
    if round_digit != 0 || sticky {
        status |= Status::INEXACT;
    }
    (rounded, status)
}

/// Same rounding rules as the arithmetic core, duplicated locally to
/// avoid a public re-export of the helper. Match `ops::round`.
fn should_round_up_int(
    rm: RoundingMode,
    sign: bool,
    last_kept: u32,
    round_digit: u32,
    sticky: bool,
) -> bool {
    let dropped_nonzero = round_digit != 0 || sticky;
    if !dropped_nonzero {
        return false;
    }
    match rm {
        RoundingMode::TowardZero => false,
        RoundingMode::TowardPositive => !sign,
        RoundingMode::TowardNegative => sign,
        RoundingMode::NearestAway => round_digit >= 5,
        RoundingMode::NearestEven => match round_digit.cmp(&5) {
            core::cmp::Ordering::Less => false,
            core::cmp::Ordering::Greater => true,
            core::cmp::Ordering::Equal => sticky || (last_kept & 1) == 1,
        },
    }
}

// Suppress dead-code on the const PRECISION import in case the optimiser
// drops the assertion in release.
#[allow(dead_code)]
const _PRECISION_KEEPALIVE: u32 = PRECISION;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_zero() {
        assert_eq!(Decimal128::from_i32(0).to_bits(), Decimal128::ZERO.to_bits());
        assert_eq!(Decimal128::from_u64(0).to_bits(), Decimal128::ZERO.to_bits());
    }

    #[test]
    fn from_small_signed() {
        let one = Decimal128::from_i32(1);
        assert_eq!(one.to_bits(), Decimal128::ONE.to_bits());
        let neg_one = Decimal128::from_i32(-1);
        assert_eq!(neg_one.to_bits(), Decimal128::NEG_ONE.to_bits());

        let i64_min = Decimal128::from_i64(i64::MIN);
        assert!(i64_min.is_finite());
        assert!(i64_min.is_sign_negative());
        let (n, s) = i64_min.to_i64(RoundingMode::default());
        assert!(s.is_ok());
        assert_eq!(n, i64::MIN);
    }

    #[test]
    fn from_unsigned_max() {
        let big = Decimal128::from_u64(u64::MAX);
        let (n, s) = big.to_u64(RoundingMode::default());
        assert!(s.is_ok());
        assert_eq!(n, u64::MAX);
    }

    #[test]
    fn from_u128_under_precision_limit_exact() {
        let n = 10u128.pow(34) - 1;
        let (d, s) = Decimal128::from_u128(n, RoundingMode::default());
        assert!(s.is_ok());
        let (back, s) = d.to_u128(RoundingMode::default());
        assert!(s.is_ok());
        assert_eq!(back, n);
    }

    #[test]
    fn from_u128_above_precision_rounds_inexact() {
        let n = u128::MAX; // way above 10^34
        let (d, s) = Decimal128::from_u128(n, RoundingMode::NearestEven);
        assert!(d.is_finite());
        assert!(s.inexact());
    }

    #[test]
    fn to_int_nan_is_invalid() {
        let (n, s) = Decimal128::NAN.to_i64(RoundingMode::default());
        assert_eq!(n, 0);
        assert!(s.invalid());

        let (n, s) = Decimal128::SIGNALING_NAN.to_u64(RoundingMode::default());
        assert_eq!(n, 0);
        assert!(s.invalid());
    }

    #[test]
    fn to_int_inf_clamps_with_invalid() {
        let (n, s) = Decimal128::INFINITY.to_i32(RoundingMode::default());
        assert_eq!(n, i32::MAX);
        assert!(s.invalid());
        let (n, s) = Decimal128::NEG_INFINITY.to_i32(RoundingMode::default());
        assert_eq!(n, i32::MIN);
        assert!(s.invalid());
        let (n, s) = Decimal128::INFINITY.to_u64(RoundingMode::default());
        assert_eq!(n, u64::MAX);
        assert!(s.invalid());
        let (n, s) = Decimal128::NEG_INFINITY.to_u64(RoundingMode::default());
        assert_eq!(n, 0);
        assert!(s.invalid());
    }

    #[test]
    fn to_int_negative_to_unsigned_invalid() {
        let (n, s) = Decimal128::NEG_ONE.to_u32(RoundingMode::default());
        assert_eq!(n, 0);
        assert!(s.invalid());
    }

    #[test]
    fn to_int_overflow_clamps() {
        // 2^31 = 2147483648, just above i32::MAX (2147483647).
        let big = Decimal128::from_u64(2_147_483_648);
        let (n, s) = big.to_i32(RoundingMode::default());
        assert_eq!(n, i32::MAX);
        assert!(s.invalid());
    }

    #[test]
    fn to_int_rounds() {
        // 1.5 — round to even gives 2, away gives 2, toward-zero gives 1.
        let one_half = Decimal128::from_bits(pack_finite(false, BIAS - 1, 15));
        let (n, _) = one_half.to_i32(RoundingMode::NearestEven);
        assert_eq!(n, 2);
        let (n, _) = one_half.to_i32(RoundingMode::TowardZero);
        assert_eq!(n, 1);

        // 2.5 — nearest-even gives 2 (round to even), away gives 3.
        let two_half = Decimal128::from_bits(pack_finite(false, BIAS - 1, 25));
        let (n, _) = two_half.to_i32(RoundingMode::NearestEven);
        assert_eq!(n, 2);
        let (n, _) = two_half.to_i32(RoundingMode::NearestAway);
        assert_eq!(n, 3);

        // 0.4 → 0 (any nearest mode), 0.6 → 1 nearest, 0 toward-zero.
        let four_tenths = Decimal128::from_bits(pack_finite(false, BIAS - 1, 4));
        let (n, _) = four_tenths.to_i32(RoundingMode::NearestEven);
        assert_eq!(n, 0);
        let six_tenths = Decimal128::from_bits(pack_finite(false, BIAS - 1, 6));
        let (n, _) = six_tenths.to_i32(RoundingMode::NearestEven);
        assert_eq!(n, 1);
        let (n, _) = six_tenths.to_i32(RoundingMode::TowardZero);
        assert_eq!(n, 0);
    }

    #[test]
    fn from_to_roundtrip_i64() {
        for &v in &[
            0i64,
            1,
            -1,
            i64::MAX,
            i64::MIN,
            123_456_789,
            -987_654_321,
            1_000_000_000_000,
        ] {
            let d: Decimal128 = v.into();
            let (back, s) = d.to_i64(RoundingMode::default());
            assert!(s.is_ok());
            assert_eq!(back, v);
        }
    }

    #[test]
    fn negative_rounding_directions() {
        // -1.5 with TowardPositive should round to -1 (toward +∞).
        let neg_one_half = Decimal128::from_bits(pack_finite(true, BIAS - 1, 15));
        let (n, _) = neg_one_half.to_i32(RoundingMode::TowardPositive);
        assert_eq!(n, -1);
        let (n, _) = neg_one_half.to_i32(RoundingMode::TowardNegative);
        assert_eq!(n, -2);
    }

    #[test]
    fn integer_value_is_exact() {
        // 7 has no fraction; rounding mode should not affect.
        for &rm in &[
            RoundingMode::NearestEven,
            RoundingMode::NearestAway,
            RoundingMode::TowardZero,
            RoundingMode::TowardPositive,
            RoundingMode::TowardNegative,
        ] {
            let (n, s) = Decimal128::from_i32(7).to_i32(rm);
            assert_eq!(n, 7);
            assert!(s.is_ok());
        }
    }
}
