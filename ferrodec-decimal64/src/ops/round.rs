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
    pack_finite, pack_infinity, BiasedExp, Coefficient, BIAS, BIASED_EXP_MAX, COEFFICIENT_LIMIT,
    E_MIN, PRECISION,
};
use crate::decimal::Decimal64;
use ferrodec_ieee::{should_round_up, RoundingMode, Status};

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
        // q_clamped is in the representable unbiased range by construction.
        let biased_exp = BiasedExp::try_from_unbiased(q_clamped)
            .expect("q_clamped in [-BIAS, BIASED_EXP_MAX - BIAS]");
        return (
            Decimal64::from_bits(pack_finite(sign, biased_exp, Coefficient::ZERO)),
            status,
        );
    }

    let digits = digit_count_u64(coef);
    // Single-rounding subnormal fix (fd-dc6): the old code rounded to
    // PRECISION here and then `finalise_finite`'s `biased < 0` arm
    // rounded a *second* time into the subnormal quantum. A residue
    // that landed strictly above the subnormal tie but below the
    // PRECISION tie was collapsed by the first rounding, so the second
    // saw an exact tie and rounded the wrong way (e.g.
    // `fma(2.064141013983096e-361, 8.386823222860694e-24, +0e+113)`
    // produced a coefficient ending `326` instead of `327`). Drop to
    // the wider of the PRECISION and subnormal-quantum requirements in
    // ONE rounding, the sibling analogue of the parent `Decimal128`
    // fd-42l fix. This leaves `finalise_finite`'s `biased < 0` arm
    // unreachable for non-zero results.
    let qmin = -(BIAS as i32);
    let precision_excess = digits.saturating_sub(PRECISION);
    let subnormal_excess = u32::try_from((qmin - unbiased_exp).max(0)).unwrap_or(u32::MAX);
    let excess = precision_excess.max(subnormal_excess);
    let (kept, kept_exp, round_digit, sticky) = if excess == 0 {
        (coef, unbiased_exp, 0u32, pre_sticky)
    } else {
        drop_excess_digits(coef, excess, pre_sticky, unbiased_exp)
    };
    let mut kept_digits = digits.saturating_sub(excess).clamp(1, PRECISION);

    if round_digit != 0 || sticky {
        status |= Status::INEXACT;
    }
    // IEEE 754-2019 §7.5: UNDERFLOW signals on a result that is tiny
    // and inexact, with tininess detected on the *pre-rounding* value
    // (the convention decTest pins). The single-step subnormal drop
    // above makes `finalise_finite`'s `biased < 0` branch unreachable
    // for non-zero results, and its post-rounding tininess check keys
    // on the *rounded* digit count, which misses a subnormal value
    // that rounding lifts to the Emin boundary. Decide tininess from
    // the original coefficient's adjusted exponent instead.
    let tiny_pre = digits as i32 + unbiased_exp - 1 < E_MIN;
    if tiny_pre && status.inexact() {
        status |= Status::UNDERFLOW;
    }

    let last_kept_lsb = (kept % 10) as u32;
    let round_up = should_round_up(rm, sign, last_kept_lsb, round_digit, sticky);

    let mut rounded = kept;
    let mut exp_after = kept_exp;
    if round_up {
        rounded += 1;
        // Invariant maintained here: `kept_digits == digit count of
        // `rounded``. A round-up carry changes the digit count in
        // exactly two ways. (1) `rounded` reaches `COEFFICIENT_LIMIT`
        // (10^PRECISION): the divide brings it back to PRECISION
        // digits, and `kept_digits` was already `PRECISION` (it is
        // `min(digits, PRECISION)` and only a PRECISION-digit `kept`
        // can carry that far), so it stays correct without a
        // decrement. (2) `rounded` crosses the next power of ten
        // (e.g. 99 → 100): the digit count grows by one, matched by
        // the `kept_digits += 1` below.
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
        // IEEE 754-2019 §6.3: the down-shift target is
        // MAX(q_preferred, qmin); padding toward a quantum below qmin
        // is not representable. (fd-dc6, matching the parent fd-42l.)
        let down_target = q_preferred.max(qmin);
        if exp_after > down_target {
            let max_shift = PRECISION as i32 - kept_digits as i32;
            let want_shift = exp_after - down_target;
            let shift = want_shift.min(max_shift);
            if shift > 0 {
                rounded *= POW10_U64[shift as usize];
                exp_after -= shift;
            }
        } else if exp_after < q_preferred && !status.inexact() {
            // Cohort lowering: raise the exponent toward the
            // preferred quantum by stripping trailing zeros. The loop
            // exits on the first of two conditions. `want_shift == 0`
            // means the preferred quantum has been reached, so no
            // further raise is wanted. `rounded % 10 != 0` means the
            // least significant digit is non-zero, so dividing by ten
            // would discard a significant digit and change the value;
            // only an exact trailing zero can be removed without
            // altering the numeric result. The `!status.inexact()`
            // guard above ensures this path runs only for exact
            // results, where every removable digit is genuinely zero.
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

    // Zero with any quantum is always representable: clamp the
    // exponent into the encodable range and emit a canonical zero.
    // Reachable when the caller passes pre_sticky = true and the
    // dropped digits round to zero — the up-front fast path at
    // round_and_pack_finite's entry only fires when pre_sticky is
    // false. Without this short-circuit, an extreme alignment
    // cancellation that produces a zero coefficient with an
    // out-of-range biased exponent would spuriously raise OVERFLOW
    // and round to ±∞ instead.
    if coef == 0 {
        let clamped = biased.clamp(0, biased_exp_max);
        // clamped is in [0, BIASED_EXP_MAX] by clamp() above.
        let biased_exp =
            BiasedExp::try_from_biased(clamped as u32).expect("clamped in [0, BIASED_EXP_MAX]");
        if clamped != biased {
            // The zero's preferred exponent fell outside the format
            // range and was clamped into it. IEEE 754-2019 §7.4
            // Clamped — informational; a zero is exact at every
            // exponent. Matches decTest cases like dddiv497
            // (`0E+380 / 1000E-13 -> 0E+369 Clamped`).
            status |= Status::CLAMPED;
        }
        return (
            Decimal64::from_bits(pack_finite(sign, biased_exp, Coefficient::ZERO)),
            status,
        );
    }

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
                let shifted_coef =
                    Coefficient::try_new(shifted).expect("shifted < COEFFICIENT_LIMIT");
                // IEEE 754-2019 §7.4: the preferred exponent exceeded
                // the format range and was clamped down (the
                // coefficient absorbed the shift as trailing zeros).
                // The condition is informational; the value is exact.
                status |= Status::CLAMPED;
                return (
                    Decimal64::from_bits(pack_finite(sign, BiasedExp::MAX, shifted_coef)),
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
            pack_finite(sign, BiasedExp::MAX, Coefficient::MAX)
        };
        return (Decimal64::from_bits(bits), status);
    }

    if biased < 0 {
        let shift = (-biased) as u32;
        let (kept, _, round_digit, sticky) = drop_excess_digits(coef, shift, false, biased);
        let last_lsb = (kept % 10) as u32;
        let round_up = should_round_up(rm, sign, last_lsb, round_digit, sticky);
        let final_coef = if round_up { kept + 1 } else { kept };
        // A `biased < 0` result is subnormal by construction (its
        // exponent is below Etiny and it is denormalised up into the
        // representable range). IEEE 754-2019 §7.5 signals UNDERFLOW
        // when a subnormal result is inexact. Inexactness can arise
        // either from this arm's own digit drop *or* from an earlier
        // precision rounding the caller already recorded in `status`
        // (decTest ddfma2901: the 19→16 digit rounding set INEXACT,
        // then the final 1-digit shift here was exact, which the old
        // `round_digit != 0 || sticky` test mistook for "no
        // underflow"). Agent 3 finding F5 / M1.
        if round_digit != 0 || sticky {
            status |= Status::INEXACT;
        }
        if status.inexact() {
            status |= Status::UNDERFLOW;
        }
        if final_coef >= COEFFICIENT_LIMIT {
            let bumped = final_coef / 10;
            // bumped < COEFFICIENT_LIMIT by construction (final_coef < 10 * COEFFICIENT_LIMIT).
            let bumped_coef = Coefficient::try_new(bumped).expect("bumped < COEFFICIENT_LIMIT");
            return (
                Decimal64::from_bits(pack_finite(
                    sign,
                    BiasedExp::try_from_biased(1).unwrap(),
                    bumped_coef,
                )),
                status,
            );
        }
        let final_coefficient =
            Coefficient::try_new(final_coef).expect("final_coef < COEFFICIENT_LIMIT");
        return (
            Decimal64::from_bits(pack_finite(sign, BiasedExp::MIN, final_coefficient)),
            status,
        );
    }

    // biased ∈ [0, biased_exp_max] from the if-arms above, coef < COEFFICIENT_LIMIT.
    //
    // IEEE 754-2019 §7.5: a result that is representable (biased ≥ 0)
    // but tiny is still subnormal when its adjusted exponent falls
    // below E_MIN. The deeply-subnormal `biased < 0` arm above
    // already raises UNDERFLOW; this catches the representable
    // subnormal that the `biased < 0` test misses. Underflow is
    // signalled only together with inexactness (an exact subnormal is
    // `Subnormal` but not `Underflow`), so it gates on the INEXACT
    // the rounding step already accumulated. Closes decTest
    // ddfma2901 (`9.00000000600000E-384` is Underflow Inexact
    // Subnormal, was Inexact only). Agent 3 finding F5 / M1.
    let adjusted_exp = unbiased_exp + digit_count_u64(coef) as i32 - 1;
    if adjusted_exp < E_MIN && status.inexact() {
        status |= Status::UNDERFLOW;
    }

    let biased_exp =
        BiasedExp::try_from_biased(biased as u32).expect("biased in [0, BIASED_EXP_MAX]");
    let coefficient = Coefficient::try_new(coef).expect("coef < COEFFICIENT_LIMIT");
    (
        Decimal64::from_bits(pack_finite(sign, biased_exp, coefficient)),
        status,
    )
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
        let (d, s) = round_and_pack_finite(
            123,
            0,
            0,
            false,
            false,
            RoundingMode::NearestEven,
            Status::OK,
        );
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
        assert_eq!(d.to_bits(), pack(false, 1, 1_234_567_890_123_457).to_bits());
        assert!(s.inexact());
    }

    #[test]
    fn round_zero_with_pre_sticky_at_overflow_exp_clamps() {
        // Regression: coef = 0, pre_sticky = true, unbiased_exp far
        // beyond E_MAX. Without the zero short-circuit in
        // finalise_finite this would raise OVERFLOW and round to
        // ±∞ — wrong, because zero with any quantum is representable
        // (the encoded biased_exp is just clamped).
        let (d, s) = round_and_pack_finite(
            0,
            500,
            500,
            false,
            true, // pre_sticky
            RoundingMode::NearestEven,
            Status::OK,
        );
        assert!(d.is_zero(), "expected zero, got {d:?}");
        assert!(!d.is_sign_negative());
        assert!(!s.overflow());
    }

    #[test]
    fn round_overflow_to_infinity_nearest() {
        // True overflow: 10^16 × 10^384 has adjusted exponent
        // 16 + 384 - 1 = 399 > E_MAX = 384, so clamping is not
        // possible. Expect ±∞ + OVERFLOW + INEXACT.
        let (d, s) = round_and_pack_finite(
            9_999_999_999_999_999,
            384,
            384,
            false,
            false,
            RoundingMode::NearestEven,
            Status::OK,
        );
        assert!(d.is_infinite() && !d.is_sign_negative());
        assert!(s.overflow() && s.inexact());
    }

    #[test]
    fn round_clamp_when_adjusted_in_range() {
        // 1E+384 — biased = 782 > 767, but adjusted_exp = 384 ≤ E_MAX.
        // Clamp pads coef to 10^15 at biased_exp = 767, giving the
        // same numeric value at a representable cohort.
        let (d, s) = round_and_pack_finite(
            1,
            384,
            384,
            false,
            false,
            RoundingMode::NearestEven,
            Status::OK,
        );
        assert!(d.is_finite() && !d.is_zero());
        assert!(!s.overflow());
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
