//! Decimal rounding to 7 digits, with IEEE 754 status-flag emission.
//!
//! Both `parse_str` and the arithmetic ops compute an unrounded
//! coefficient (up to ~16 decimal digits, fitting in `u64`), associate
//! it with a target quantum exponent, and call here to:
//!
//! 1. Drop digits below position `PRECISION` from the coefficient,
//!    tracking guard / sticky bits.
//! 2. Apply the [`RoundingMode`] direction.
//! 3. Renormalise if the rounding bumped the digit count over 7.
//! 4. Convert to a biased exponent, watching for overflow / underflow.
//! 5. Emit `INEXACT` / `OVERFLOW` / `UNDERFLOW` flags as appropriate.
//!
//! For `Decimal32`, the working coefficient fits in a single `u64`;
//! ferrodec's BID-128 equivalent uses `U256` because Decimal128's
//! 34-digit precision can't be held in `u128` after alignment expansion.
//! This module is the simpler analogue.

use crate::bid::{
    pack_finite, pack_infinity, BIAS, BIASED_EXP_MAX, COEFFICIENT_LIMIT, PRECISION,
};
use crate::decimal::Decimal32;
use ferrodec_ieee::{RoundingMode, Status};

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

/// Decimal digit count of a `u64`. Returns `1` when `n == 0`.
#[inline]
fn digit_count_u64(n: u64) -> u32 {
    if n == 0 {
        1
    } else {
        n.ilog10() + 1
    }
}

/// Round `coef × 10^unbiased_exp` (with sign `sign`) to a canonical
/// `Decimal32`, accumulating the operation's prior `status`.
///
/// `q_preferred` is the IEEE 754-2019 §6.3 *preferred* quantum exponent
/// for the operation (e.g. `min(qa, qb)` for add, `qa + qb` for mul,
/// the parsed input's quantum for `parse_str`). After rounding we adjust
/// the coefficient toward `q_preferred` — padding with trailing zeros
/// on inexact results so the encoded coefficient reaches `PRECISION`
/// digits, or stripping trailing zeros on exact results so the cohort
/// matches the spec's preferred form.
///
/// `pre_sticky` carries any low-order bits the caller had to drop
/// before reaching this entry point (e.g. parse digits beyond the
/// accumulator's capacity).
#[allow(clippy::too_many_arguments)] // each argument is load-bearing
pub(crate) fn round_and_pack_finite(
    coef: u64,
    unbiased_exp: i32,
    q_preferred: i32,
    sign: bool,
    pre_sticky: bool,
    rm: RoundingMode,
    mut status: Status,
) -> (Decimal32, Status) {
    if coef == 0 && !pre_sticky {
        let q = q_preferred.min(unbiased_exp);
        let bias = BIAS as i32;
        let q_clamped = q.clamp(-bias, BIASED_EXP_MAX as i32 - bias);
        return (
            Decimal32::from_bits(pack_finite(sign, biased(q_clamped), 0)),
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
        // Renormalise if rounding crossed a power-of-10 boundary. Two
        // sub-cases:
        // * Rounded value crosses `COEFFICIENT_LIMIT`: divide by 10 and
        //   bump the exponent. Digit count stays at PRECISION.
        // * Rounded value's digit count just increased by 1 but stays
        //   ≤ PRECISION.
        if rounded >= u64::from(COEFFICIENT_LIMIT) {
            rounded /= 10;
            exp_after += 1;
        } else if (kept_digits as usize) < POW10_U64.len()
            && rounded == POW10_U64[kept_digits as usize]
        {
            kept_digits += 1;
        }
    }

    // Shift toward the preferred quantum for cohort selection.
    if rounded != 0 {
        if exp_after > q_preferred {
            // Pad with trailing zeros up to PRECISION digits.
            let max_shift = PRECISION as i32 - kept_digits as i32;
            let want_shift = exp_after - q_preferred;
            let shift = want_shift.min(max_shift);
            if shift > 0 {
                rounded *= POW10_U64[shift as usize];
                exp_after -= shift;
            }
        } else if exp_after < q_preferred && !status.inexact() {
            // Strip trailing zeros on exact results.
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

/// Drop `n` low-order decimal digits from `coef`, returning
/// `(kept, kept_exp, round_digit, sticky)`.
///
/// `round_digit` is the most significant of the dropped digits (the
/// digit immediately below the new LSB). `sticky` is the OR over the
/// remaining dropped digits being non-zero, seeded with `pre_sticky`.
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

/// Per-mode rounding decision: should we round the kept coefficient up
/// by 1 (toward larger magnitude) given the dropped guard / sticky?
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

/// Finalise a rounded `(coef, unbiased_exp, sign)` by checking
/// overflow / underflow and packing.
fn finalise_finite(
    coef: u64,
    unbiased_exp: i32,
    sign: bool,
    rm: RoundingMode,
    mut status: Status,
) -> (Decimal32, Status) {
    let bias = BIAS as i32;
    let biased_exp_max = BIASED_EXP_MAX as i32;
    let biased = unbiased_exp + bias;

    if biased > biased_exp_max {
        // Overflow: rounded magnitude exceeds MAX. Per IEEE 754-2019
        // §7.4, the result depends on the rounding mode.
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
            // Round toward the bounded extremum.
            pack_finite(sign, BIASED_EXP_MAX, COEFFICIENT_LIMIT - 1)
        };
        return (Decimal32::from_bits(bits), status);
    }

    if biased < 0 {
        // Underflow toward subnormal or zero. Shift the coefficient
        // right by `-biased` decimal positions, accumulating a sticky
        // bit; pack at biased_exp = 0. drop_excess_digits handles the
        // shift >= digit_count case correctly (final coefficient is
        // zero with the MSD becoming the round digit), so the
        // rounding decision below covers both "zero result" and
        // "rounds up to MIN_POSITIVE".
        let shift = (-biased) as u32;
        let (kept, _, round_digit, sticky) = drop_excess_digits(coef, shift, false, biased);
        let last_lsb = (kept % 10) as u32;
        let round_up = should_round_up(rm, sign, last_lsb, round_digit, sticky);
        let final_coef = if round_up { kept + 1 } else { kept };
        if round_digit != 0 || sticky {
            status |= Status::INEXACT | Status::UNDERFLOW;
        }
        // After rounding the subnormal could cross back over to normal
        // if it gained a digit (e.g. 9_999_999 + ulp = 10_000_000): in
        // that case re-pack at biased_exp = 1.
        if final_coef >= u64::from(COEFFICIENT_LIMIT) {
            let bumped = final_coef / 10;
            return (
                Decimal32::from_bits(pack_finite(sign, 1, bumped as u32)),
                status,
            );
        }
        return (
            Decimal32::from_bits(pack_finite(sign, 0, final_coef as u32)),
            status,
        );
    }

    debug_assert!(coef < u64::from(COEFFICIENT_LIMIT));
    (
        Decimal32::from_bits(pack_finite(sign, biased as u32, coef as u32)),
        status,
    )
}

/// Bias an unbiased quantum exponent.
#[inline]
fn biased(unbiased_exp: i32) -> u32 {
    (unbiased_exp + BIAS as i32) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack(sign: bool, q: i32, coef: u32) -> Decimal32 {
        Decimal32::try_new_unsigned(coef, q)
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
    fn round_seven_digits_exact() {
        let (d, s) =
            round_and_pack_finite(9_999_999, 0, 0, false, false, RoundingMode::NearestEven, Status::OK);
        assert_eq!(d.to_bits(), pack(false, 0, 9_999_999).to_bits());
        assert!(s.is_ok());
    }

    #[test]
    fn round_eight_digits_nearest_even() {
        // 12_345_678 → 8 digits, round to 7 at nearest-even: drop '8',
        // round up because round_digit = 8 > 5.
        let (d, s) = round_and_pack_finite(
            12_345_678,
            0,
            0,
            false,
            false,
            RoundingMode::NearestEven,
            Status::OK,
        );
        assert_eq!(d.to_bits(), pack(false, 1, 1_234_568).to_bits());
        assert!(s.inexact());
    }

    #[test]
    fn round_halfway_to_even() {
        // 12_345_655 → kept = 1_234_565, round_digit = 5, sticky = false,
        // last_lsb = 5 (odd) → round up to 1_234_566.
        let (d, _) = round_and_pack_finite(
            12_345_655,
            0,
            0,
            false,
            false,
            RoundingMode::NearestEven,
            Status::OK,
        );
        assert_eq!(d.to_bits(), pack(false, 1, 1_234_566).to_bits());

        // 12_345_645 → kept = 1_234_564, round_digit = 5, sticky = false,
        // last_lsb = 4 (even) → no round (already even).
        let (d, _) = round_and_pack_finite(
            12_345_645,
            0,
            0,
            false,
            false,
            RoundingMode::NearestEven,
            Status::OK,
        );
        assert_eq!(d.to_bits(), pack(false, 1, 1_234_564).to_bits());

        // 12_345_644 → kept = 1_234_564, round_digit = 4 → round down.
        let (d, _) = round_and_pack_finite(
            12_345_644,
            0,
            0,
            false,
            false,
            RoundingMode::NearestEven,
            Status::OK,
        );
        assert_eq!(d.to_bits(), pack(false, 1, 1_234_564).to_bits());
    }

    #[test]
    fn round_carry_renormalises() {
        // 99_999_995 → kept = 9_999_999, round_digit = 5, sticky = false,
        // last_lsb = 9 (odd) → round up. Rounded coefficient becomes
        // 10_000_000, which equals COEFFICIENT_LIMIT — renormalise by
        // dividing by 10 and bumping the exponent.
        let (d, _) = round_and_pack_finite(
            99_999_995,
            0,
            0,
            false,
            false,
            RoundingMode::NearestEven,
            Status::OK,
        );
        assert_eq!(d.to_bits(), pack(false, 2, 1_000_000).to_bits());
    }

    #[test]
    fn round_overflow_to_infinity_nearest() {
        let (d, s) = round_and_pack_finite(
            1,
            96,
            96,
            false,
            false,
            RoundingMode::NearestEven,
            Status::OK,
        );
        // Just at boundary: 1E+96 — biased = 96 + 101 = 197 > 191.
        // Wait, 1E+96 = 1 × 10^96. Adjusted: digit_count(1) + 96 = 97 > 96
        // = E_MAX, so this overflows. Per nearest, → +∞.
        assert!(d.is_infinite() && !d.is_sign_negative());
        assert!(s.overflow() && s.inexact());
    }

    #[test]
    fn round_overflow_to_max_toward_zero() {
        let (d, s) = round_and_pack_finite(
            10_000_000,
            96,
            96,
            false,
            false,
            RoundingMode::TowardZero,
            Status::OK,
        );
        // 10^7 × 10^96 = 10^103, way over MAX. TowardZero → ±MAX.
        assert_eq!(d.to_bits(), Decimal32::MAX.to_bits());
        assert!(s.overflow() && s.inexact());
    }

    #[test]
    fn round_underflow_to_zero() {
        // 1 × 10^-200 — well below MIN_POSITIVE = 10^-101.
        let (d, s) = round_and_pack_finite(
            1,
            -200,
            -200,
            false,
            false,
            RoundingMode::NearestEven,
            Status::OK,
        );
        assert!(d.is_zero());
        assert!(s.inexact() && s.underflow());
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
        // Zero with quantum -3 ≡ "0E-3" = "0.000".
        assert!(d.is_zero());
        assert_eq!(d.to_bits(), pack(false, -3, 0).to_bits());
        assert!(s.is_ok());
    }
}
