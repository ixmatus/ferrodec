//! Decimal rounding to 16 digits, with IEEE 754 status-flag emission.
//!
//! Both `parse_str` and the arithmetic ops compute an unrounded
//! coefficient (up to ~19 decimal digits, fitting in `u64`), associate
//! it with a target quantum exponent, and call here to:
//!
//! 1. Drop digits below position `PRECISION` from the coefficient,
//!    tracking guard / sticky bits.
//! 2. Apply the [`RoundingMode`] direction.
//! 3. Renormalise if the rounding bumped the digit count over 16.
//! 4. Convert to a biased exponent, watching for overflow / underflow.
//! 5. Emit `INEXACT` / `OVERFLOW` / `UNDERFLOW` flags as appropriate.
//!
//! Working precision fits in a single `u64` for the basic arithmetic
//! ops and `parse_str`; multiply / FMA use a `u128` exact product
//! that compresses back to `u64` via sticky tracking before routing
//! here.

use crate::bid::{
    pack_finite, pack_infinity, BIAS, BIASED_EXP_MAX, COEFFICIENT_LIMIT, PRECISION,
};
use crate::decimal::Decimal64;
use crate::status::{RoundingMode, Status};

/// `10^k` for `k <= 19` (the largest power of ten that fits in `u64`).
const POW10_U64: [u64; 20] = [
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
    10_000_000_000_000_000,
    100_000_000_000_000_000,
    1_000_000_000_000_000_000,
    10_000_000_000_000_000_000,
];

#[inline]
fn digit_count_u64(n: u64) -> u32 {
    if n == 0 {
        1
    } else {
        n.ilog10() + 1
    }
}

/// Round `coef × 10^unbiased_exp` (with sign `sign`) to a canonical
/// `Decimal64`, accumulating the operation's prior `status`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn round_and_pack_finite(
    coef: u64,
    unbiased_exp: i32,
    q_preferred: i32,
    sign: bool,
    pre_sticky: bool,
    rm: RoundingMode,
    mut status: Status,
) -> (Decimal64, Status) {
    if coef == 0 && !pre_sticky {
        let q = q_preferred.min(unbiased_exp);
        let bias = BIAS as i32;
        let q_clamped = q.clamp(-bias, BIASED_EXP_MAX as i32 - bias);
        return (
            Decimal64::from_bits(pack_finite(sign, biased(q_clamped), 0)),
            status,
        );
    }

    let digits = digit_count_u64(coef);
    let (kept, kept_exp, round_digit, sticky) = if digits > PRECISION {
        let excess = digits - PRECISION;
        drop_excess_digits(coef, excess, pre_sticky, unbiased_exp)
    } else {
        (coef, unbiased_exp, 0u32, pre_sticky)
    };
    let mut kept_digits = digits.min(PRECISION);

    if round_digit != 0 || sticky {
        status |= Status::INEXACT;
    }

    let last_kept_lsb = (kept % 10) as u32;
    let round_up = should_round_up(rm, sign, last_kept_lsb, round_digit, sticky);

    let mut rounded = kept;
    let mut exp_after = kept_exp;
    if round_up {
        rounded += 1;
        if rounded >= COEFFICIENT_LIMIT {
            rounded /= 10;
            exp_after += 1;
        } else if (kept_digits as usize) < POW10_U64.len()
            && rounded == POW10_U64[kept_digits as usize]
        {
            kept_digits += 1;
        }
    }

    if rounded != 0 {
        if exp_after > q_preferred {
            let max_shift = PRECISION as i32 - kept_digits as i32;
            let want_shift = exp_after - q_preferred;
            let shift = want_shift.min(max_shift);
            if shift > 0 {
                rounded *= POW10_U64[shift as usize];
                exp_after -= shift;
            }
        } else if exp_after < q_preferred && !status.inexact() {
            let mut want_shift = q_preferred - exp_after;
            while want_shift > 0 && rounded % 10 == 0 {
                rounded /= 10;
                exp_after += 1;
                want_shift -= 1;
            }
        }
    }

    finalise_finite(rounded, exp_after, sign, rm, status)
}

fn drop_excess_digits(
    mut coef: u64,
    n: u32,
    pre_sticky: bool,
    unbiased_exp: i32,
) -> (u64, i32, u32, bool) {
    let mut sticky = pre_sticky;
    let mut round_digit = 0u32;
    let mut i = 0u32;
    while i < n {
        let r = (coef % 10) as u32;
        coef /= 10;
        if i == n - 1 {
            round_digit = r;
        } else if r != 0 {
            sticky = true;
        }
        i += 1;
    }
    (coef, unbiased_exp + n as i32, round_digit, sticky)
}

#[allow(clippy::similar_names)]
fn should_round_up(
    rm: RoundingMode,
    sign: bool,
    last_kept_lsb: u32,
    round_digit: u32,
    sticky: bool,
) -> bool {
    match rm {
        RoundingMode::NearestEven => match round_digit.cmp(&5) {
            core::cmp::Ordering::Less => false,
            core::cmp::Ordering::Greater => true,
            core::cmp::Ordering::Equal => sticky || (last_kept_lsb & 1) == 1,
        },
        RoundingMode::NearestAway => round_digit >= 5,
        RoundingMode::TowardZero => false,
        RoundingMode::TowardPositive => !sign && (round_digit > 0 || sticky),
        RoundingMode::TowardNegative => sign && (round_digit > 0 || sticky),
    }
}

fn finalise_finite(
    coef: u64,
    unbiased_exp: i32,
    sign: bool,
    rm: RoundingMode,
    mut status: Status,
) -> (Decimal64, Status) {
    let bias = BIAS as i32;
    let biased_exp_max = BIASED_EXP_MAX as i32;
    let biased = unbiased_exp + bias;

    if biased > biased_exp_max {
        // Try IEEE 754-2019 §6.3 exponent clamping: if the adjusted
        // exponent is within the format's range, pad the coefficient
        // with trailing zeros to bring biased down to BIASED_EXP_MAX.
        // This is the "Clamped" condition — informational only, no
        // OVERFLOW raised.
        let shift_needed = (biased - biased_exp_max) as u32;
        let digits = digit_count_u64(coef);
        if (digits + shift_needed) as i32 <= PRECISION as i32
            && (shift_needed as usize) < POW10_U64.len()
            && coef != 0
        {
            let shifted = coef * POW10_U64[shift_needed as usize];
            if shifted < COEFFICIENT_LIMIT {
                return (
                    Decimal64::from_bits(pack_finite(sign, BIASED_EXP_MAX, shifted)),
                    status,
                );
            }
        }
        status |= Status::OVERFLOW | Status::INEXACT;
        let to_inf = match rm {
            RoundingMode::NearestEven | RoundingMode::NearestAway => true,
            RoundingMode::TowardZero => false,
            RoundingMode::TowardPositive => !sign,
            RoundingMode::TowardNegative => sign,
        };
        let bits = if to_inf {
            pack_infinity(sign)
        } else {
            pack_finite(sign, BIASED_EXP_MAX, COEFFICIENT_LIMIT - 1)
        };
        return (Decimal64::from_bits(bits), status);
    }

    if biased < 0 {
        let shift = (-biased) as u32;
        let (kept, _, round_digit, sticky) = drop_excess_digits(coef, shift, false, biased);
        let last_lsb = (kept % 10) as u32;
        let round_up = should_round_up(rm, sign, last_lsb, round_digit, sticky);
        let final_coef = if round_up { kept + 1 } else { kept };
        if round_digit != 0 || sticky {
            status |= Status::INEXACT | Status::UNDERFLOW;
        }
        if final_coef >= COEFFICIENT_LIMIT {
            let bumped = final_coef / 10;
            return (
                Decimal64::from_bits(pack_finite(sign, 1, bumped)),
                status,
            );
        }
        return (
            Decimal64::from_bits(pack_finite(sign, 0, final_coef)),
            status,
        );
    }

    debug_assert!(coef < COEFFICIENT_LIMIT);
    (
        Decimal64::from_bits(pack_finite(sign, biased as u32, coef)),
        status,
    )
}

#[inline]
fn biased(unbiased_exp: i32) -> u32 {
    (unbiased_exp + BIAS as i32) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack(sign: bool, q: i32, coef: u64) -> Decimal64 {
        Decimal64::try_new_unsigned(coef, q)
            .map(|d| if sign { d.neg() } else { d })
            .unwrap()
    }

    #[test]
    fn round_no_rounding_required() {
        let (d, s) = round_and_pack_finite(123, 0, 0, false, false, RoundingMode::NearestEven, Status::OK);
        assert_eq!(d.to_bits(), pack(false, 0, 123).to_bits());
        assert!(s.is_ok());
    }

    #[test]
    fn round_sixteen_digits_exact() {
        let (d, s) = round_and_pack_finite(
            9_999_999_999_999_999,
            0,
            0,
            false,
            false,
            RoundingMode::NearestEven,
            Status::OK,
        );
        assert_eq!(d.to_bits(), pack(false, 0, 9_999_999_999_999_999).to_bits());
        assert!(s.is_ok());
    }

    #[test]
    fn round_seventeen_digits_nearest_even() {
        // 12345678901234568 (17 digits) → drop trailing 8 → kept =
        // 1234567890123456, round_digit = 8 → round up.
        let (d, s) = round_and_pack_finite(
            12_345_678_901_234_568,
            0,
            0,
            false,
            false,
            RoundingMode::NearestEven,
            Status::OK,
        );
        assert_eq!(
            d.to_bits(),
            pack(false, 1, 1_234_567_890_123_457).to_bits()
        );
        assert!(s.inexact());
    }

    #[test]
    fn round_overflow_to_infinity_nearest() {
        let (d, s) = round_and_pack_finite(
            1,
            384,
            384,
            false,
            false,
            RoundingMode::NearestEven,
            Status::OK,
        );
        // 1E+384 — biased = 384 + 398 = 782 > 767. Overflow.
        assert!(d.is_infinite() && !d.is_sign_negative());
        assert!(s.overflow() && s.inexact());
    }

    #[test]
    fn round_underflow_to_zero() {
        let (d, s) = round_and_pack_finite(
            1,
            -500,
            -500,
            false,
            false,
            RoundingMode::NearestEven,
            Status::OK,
        );
        assert!(d.is_zero());
        assert!(s.inexact() && s.underflow());
    }

    #[test]
    fn round_carry_renormalises() {
        // 9_999_999_999_999_995 with round-up trips the carry.
        let (d, _) = round_and_pack_finite(
            99_999_999_999_999_995,
            0,
            0,
            false,
            false,
            RoundingMode::NearestEven,
            Status::OK,
        );
        // kept = 9_999_999_999_999_999, round_digit = 5, last_lsb = 9
        // (odd) → round up. Result = 10_000_000_000_000_000 ⇒
        // renormalise: divide by 10, bump exp.
        assert_eq!(d.to_bits(), pack(false, 2, 1_000_000_000_000_000).to_bits());
    }

    #[test]
    fn round_zero_preserves_quantum() {
        let (d, s) = round_and_pack_finite(
            0,
            -3,
            -3,
            false,
            false,
            RoundingMode::NearestEven,
            Status::OK,
        );
        assert!(d.is_zero());
        assert_eq!(d.to_bits(), pack(false, -3, 0).to_bits());
        assert!(s.is_ok());
    }
}
