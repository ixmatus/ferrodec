//! Decimal rounding to 34 digits, with status-flag emission.
//!
//! The arithmetic ops compute an unrounded coefficient as a 256-bit
//! intermediate (because alignment by `10^k` can grow up to 226 bits),
//! associate it with a target quantum exponent, and call here to:
//!
//! 1. Drop digits below position `PRECISION` from the coefficient,
//!    tracking guard / sticky bits.
//! 2. Apply the [`RoundingMode`] direction.
//! 3. Renormalize if the rounding bumped the digit count over 34.
//! 4. Convert to a biased exponent, watching for overflow / underflow.
//! 5. Emit `INEXACT` / `OVERFLOW` / `UNDERFLOW` flags as appropriate.
//!
//! The contract:
//!
//! * Caller provides `coef: U256`, `unbiased_exp: i32` (the quantum
//!   exponent of `coef`), `sign: bool`, and `pre_sticky: bool` (set when
//!   the alignment step had to drop low-order digits below the U256
//!   envelope).
//! * Returns `(Decimal128, Status)`. The status is *added to* an existing
//!   `Status` passed by the caller — sticky-NaN style.

use crate::bid::{
    pack_finite, pack_infinity, BIAS, BIASED_EXP_MAX, COEFFICIENT_LIMIT, E_MAX, E_MIN, PRECISION,
};
use crate::decimal::Decimal128;
use crate::multiword::u256::POW10_U128;
use crate::multiword::U256;
use crate::status::{RoundingMode, Status};
use ferrodec_ieee::should_round_up;

/// Round `coef × 10^unbiased_exp` (with sign `sign`) to a canonical
/// `Decimal128`, accumulating the operation's prior `status`.
///
/// `q_preferred` is the IEEE 754-2019 §6.3 *preferred* quantum exponent
/// for the operation (e.g. `min(qa, qb)` for add, `qa + qb` for mul).
/// After rounding we adjust the coefficient toward `q_preferred` —
/// padding with trailing zeros on inexact results so the encoded
/// coefficient reaches `PRECISION` digits, or shifting down to
/// `q_preferred` directly when that fits without exceeding precision.
pub(crate) fn round_and_pack_finite(
    coef: U256,
    unbiased_exp: i32,
    q_preferred: i32,
    sign: bool,
    pre_sticky: bool,
    rm: RoundingMode,
    mut status: Status,
) -> (Decimal128, Status) {
    if coef.is_zero() && !pre_sticky {
        // Pure zero — let caller decide sign for cancellation; here we
        // honour the sign passed in. Clamp the chosen quantum to the
        // storable range so a parse like `0E+6144` doesn't blow the
        // `BIASED_EXP_MAX` debug-assert.
        let q = q_preferred.min(unbiased_exp);
        let q_clamped = q.clamp(-(BIAS as i32), BIASED_EXP_MAX as i32 - BIAS as i32);
        return (
            Decimal128::from_bits(pack_finite(sign, biased(q_clamped), 0)),
            status,
        );
    }

    // Step 1: drop excess digits. Cache the coefficient's decimal digit
    // count once — `rounded` and `kept` track it incrementally below so
    // we don't re-walk the U256 with `decimal_digit_count` on the
    // post-rounding overflow check or the preferred-quantum pad.
    let digits = coef.decimal_digit_count();
    let (kept, kept_exp, round_digit, sticky) = if digits > PRECISION {
        let excess = digits - PRECISION;
        drop_excess_digits(coef, excess, pre_sticky, unbiased_exp)
    } else {
        // No excess to drop; pre-sticky still feeds the round logic.
        (coef, unbiased_exp, 0u32, pre_sticky)
    };
    let mut kept_digits = digits.min(PRECISION);

    if round_digit != 0 || sticky {
        status |= Status::INEXACT;
    }

    // Step 2: apply rounding direction.
    let last_kept_lsb = kept.div_rem10().1; // LSB digit of `kept`
    let round_up = should_round_up(rm, sign, last_kept_lsb, round_digit, sticky);

    let mut rounded = kept;
    let mut exp_after = kept_exp;
    if round_up {
        rounded = rounded.add(U256::from_u128(1));
        // Step 3: renormalize if rounding crossed a power-of-10 boundary
        // (`kept` was `10^kept_digits − 1`, `rounded` is `10^kept_digits`).
        // Two sub-cases:
        // * If the new value also exceeds `COEFFICIENT_LIMIT = 10^PRECISION`,
        //   we have to divide by 10 and bump the exponent — the digit
        //   count stays at `PRECISION` after that division.
        // * Otherwise the digit count just increased by 1 (still ≤ PRECISION).
        // Both cases avoid `decimal_digit_count` on the U256.
        if rounded.hi != 0 || rounded.lo >= COEFFICIENT_LIMIT {
            let (q, _) = rounded.div_rem10();
            rounded = q;
            exp_after += 1;
        } else if (kept_digits as usize) < POW10_U128.len()
            && rounded.lo == POW10_U128[kept_digits as usize]
        {
            kept_digits += 1;
        }
    }

    // Step 3.5: shift toward the preferred quantum.
    //
    // IEEE 754-2019 §6.3: target quantum = MAX(q_preferred, q_emin),
    // where q_emin is the lowest quantum at which the coefficient still
    // has ≤ PRECISION digits.
    //
    // Two directions of shift to consider:
    // * `exp_after > q_preferred`: shift *down* — multiply the
    //   coefficient by 10 up to `PRECISION − digits` times. This pads
    //   the coefficient with trailing zeros to fill PRECISION digits.
    //   Used by inexact-result preferred-quantum padding.
    // * `exp_after < q_preferred`: shift *up* — divide the coefficient
    //   by 10 while the LSD is zero, until either we reach
    //   `q_preferred` or the LSD becomes non-zero (loss-free shift).
    //   Used to strip trailing zeros from exact results so divisions
    //   like `1/1 → 1` don't appear as `1.0000…000`.
    if !rounded.is_zero() {
        if exp_after > q_preferred {
            let max_shift = PRECISION as i32 - kept_digits as i32;
            let want_shift = exp_after - q_preferred;
            let shift = want_shift.min(max_shift);
            if shift > 0 {
                rounded = rounded.mul_pow10(shift as u32);
                exp_after -= shift;
            }
        } else if exp_after < q_preferred && !status.inexact() {
            // Strip-up only on *exact* results — for inexact results
            // the trailing zeros encode the rounded precision and must
            // not be removed (`12345 / 5.01 → 2464.…210` keeps the
            // final 0 at PRECISION rep). Loss-free: stop the loop as
            // soon as the LSD becomes non-zero.
            let mut want_shift = q_preferred - exp_after;
            while want_shift > 0 {
                let (q, r) = rounded.div_rem10();
                if r != 0 {
                    break;
                }
                rounded = q;
                exp_after += 1;
                want_shift -= 1;
            }
        }
    }

    // Step 4 + 5: pack with overflow / underflow checks.
    finalize_finite(rounded, exp_after, sign, rm, status)
}

/// `(round_digit, sticky)` for the "drop everything" subnormal-underflow
/// case where `shift >= digits`. The kept value is provably zero; this
/// helper only computes the rounding inputs.
///
/// Two sub-cases:
/// * `shift > digits`: every digit of `coef` lies strictly below the
///   round position, so `round_digit = 0` (a leading-zero position
///   above MSD) and `sticky = (coef != 0)`.
/// * `shift == digits`: the round position is exactly `coef`'s MSD.
///   `sticky` is the OR of every digit below MSD; we extract it via a
///   bounded `digits - 1` iteration of `div_rem10` (worst case ≤ 77),
///   an order of magnitude tighter than the original `drop_excess_digits`
///   loop that ran `shift` ≈ 6111 times for `MIN_SUBNORMAL / MAX`.
#[inline]
fn round_digit_for_full_drop(coef: U256, shift: u32, digits: u32) -> (u32, bool) {
    debug_assert!(shift >= digits);
    if shift > digits {
        return (0, !coef.is_zero());
    }
    // shift == digits: extract the MSD plus the sticky over the lower
    // (digits - 1) digits.
    if digits == 1 {
        let (_, msd) = coef.div_rem10();
        return (msd, false);
    }
    let mut acc = coef;
    let mut sticky = false;
    let mut i = 1u32;
    while i < digits {
        let (q, r) = acc.div_rem10();
        if r != 0 {
            sticky = true;
        }
        acc = q;
        i += 1;
    }
    let (_, msd) = acc.div_rem10();
    (msd, sticky)
}

/// Drop `n` low-order decimal digits from `coef`, returning
/// `(kept, round_digit, sticky)`.
///
/// `round_digit` is the most significant of the dropped digits — i.e. the
/// digit immediately below the new LSB after `n` extractions. `sticky` is
/// the OR over any further-dropped digit being non-zero, seeded with
/// `pre_sticky`.
///
/// Single source of truth for the digit-extraction loop used by both
/// `drop_excess_digits` (precision-overflow path) and `shift_right_decimal`
/// (subnormal underflow path). The cost is O(n) U256 `div_rem10` calls,
/// so callers with potentially large `n` (the underflow branch can hit
/// n ≈ 6178 for `MIN_SUBNORMAL / MAX`) should short-circuit upstream
/// via [`round_digit_for_full_drop`] when `n >= digit_count(coef)`.
/// Pushing the digit-count check into this hot path costs ~30% on the
/// common `div` path because `decimal_digit_count` itself is O(digits).
#[inline]
fn extract_dropped_digits(mut coef: U256, n: u32, pre_sticky: bool) -> (U256, u32, bool) {
    let mut sticky = pre_sticky;
    let mut round_digit = 0u32;
    let mut i = 0u32;
    while i < n {
        let (q, r) = coef.div_rem10();
        if i == n - 1 {
            round_digit = r;
        } else if r != 0 {
            sticky = true;
        }
        coef = q;
        i += 1;
    }
    (coef, round_digit, sticky)
}

/// Drop `excess` decimal digits from `coef`, returning
/// `(kept, kept_exp, round_digit, sticky)`.
fn drop_excess_digits(
    coef: U256,
    excess: u32,
    pre_sticky: bool,
    unbiased_exp: i32,
) -> (U256, i32, u32, bool) {
    let (kept, round_digit, sticky) = extract_dropped_digits(coef, excess, pre_sticky);
    (kept, unbiased_exp + excess as i32, round_digit, sticky)
}

/// Convert a fully rounded `(coef, unbiased_exp, sign)` triple to the
/// final `Decimal128`, deciding overflow → Inf/MAX or underflow → 0.
fn finalize_finite(
    coef: U256,
    unbiased_exp: i32,
    sign: bool,
    rm: RoundingMode,
    mut status: Status,
) -> (Decimal128, Status) {
    if coef.is_zero() {
        // Never an exception path — emit canonical zero with the given exp
        // (clamped if it falls out of range).
        let clamped_exp = unbiased_exp.clamp(-(BIAS as i32), BIASED_EXP_MAX as i32 - BIAS as i32);
        return (
            Decimal128::from_bits(pack_finite(sign, biased(clamped_exp), 0)),
            status,
        );
    }

    // Overflow check: largest representable magnitude is 9.999…9 × 10^E_MAX,
    // i.e. coef < 10^34 and quantum_exp ≤ E_MAX − (digits − 1).
    // Equivalently: digits + quantum_exp − 1 ≤ E_MAX, i.e.
    //   digits + unbiased_exp <= E_MAX + 1
    let digits = coef.decimal_digit_count() as i32;
    let scale = digits + unbiased_exp; // log10(magnitude) + 1, roughly

    if scale > E_MAX + 1 {
        status |= Status::OVERFLOW | Status::INEXACT;
        return (overflow_result(sign, rm), status);
    }

    let mut biased_exp = unbiased_exp + BIAS as i32;
    let mut coef = coef;

    // Up-renormalize when the quantum exponent exceeds the storable range
    // but the value's magnitude is still within MAX. Multiplying the
    // coefficient by 10 and decrementing `biased_exp` is exact — and the
    // overflow check above guarantees we have enough digits of slack
    // (`PRECISION − digits`) to absorb the excess.
    if biased_exp > BIASED_EXP_MAX as i32 {
        let excess = biased_exp - BIASED_EXP_MAX as i32;
        let slack = PRECISION as i32 - digits;
        debug_assert!(excess <= slack, "scale check should have caught this");
        coef = coef.mul_pow10(excess as u32);
        biased_exp -= excess;
    }

    // Underflow: biased_exp < 0 means the quantum is below qmin. Try to
    // shift the coefficient right until we either fit or hit zero.
    if biased_exp < 0 {
        let shift = (-biased_exp) as u32;
        if shift >= digits as u32 {
            // Entire coefficient sits below the smallest subnormal LSD.
            // Apply the rounding mode: the result is either ±0 or
            // ±MIN_POSITIVE (= 1 at biased_exp 0).
            //
            // Fast path (M10): the original `drop_excess_digits` call
            // looped `shift` times, hitting ~6111 U256 div_rem10
            // iterations on the `MIN_SUBNORMAL / MAX` shape. We don't
            // need any of those iterations — we already know the kept
            // value is zero (`shift >= digits`), and the only inputs
            // that govern the rounding decision are `round_digit` and
            // `sticky`. Compute them in O(digits) bounded work
            // (digits ≤ 78), the `decimal_digit_count` cost is amortised
            // by the same call from line 251 above.
            let (round_digit, sticky_eff) = round_digit_for_full_drop(coef, shift, digits as u32);
            let last_kept = 0;
            let round_up = should_round_up(rm, sign, last_kept, round_digit, sticky_eff);
            let result_coef = u128::from(round_up);
            status |= Status::UNDERFLOW | Status::INEXACT;
            return (
                Decimal128::from_bits(pack_finite(sign, 0, result_coef)),
                status,
            );
        }
        // Shift right by `shift` decimal digits, applying the rounding mode
        // again to the discarded digits.
        let (shifted, dropped_sticky, round_digit) = shift_right_decimal(coef, shift);
        let last_kept = shifted.div_rem10().1;
        let round_up = should_round_up(rm, sign, last_kept, round_digit, dropped_sticky);
        let mut adjusted = shifted;
        if round_up {
            adjusted = adjusted.add(U256::from_u128(1));
        }
        // Raise UNDERFLOW if any digits were dropped here, OR if the
        // upstream rounding had already raised INEXACT — in either case
        // the subnormal result is inexact, which is the IEEE 754 §7.5
        // signal trigger.
        if round_digit != 0 || dropped_sticky {
            status |= Status::UNDERFLOW | Status::INEXACT;
        } else if status.inexact() {
            status |= Status::UNDERFLOW;
        }
        let coef_u128 = adjusted.to_u128();
        return (
            Decimal128::from_bits(pack_finite(sign, 0, coef_u128)),
            status,
        );
    }

    debug_assert!(biased_exp >= 0 && biased_exp as u32 <= BIASED_EXP_MAX);
    debug_assert!(coef.hi == 0);
    debug_assert!(coef.lo < COEFFICIENT_LIMIT);

    // IEEE 754 §7.5: raise `UNDERFLOW` for inexact subnormal results.
    // A value is subnormal when its magnitude is below `10^E_MIN` —
    // equivalently, when `digit_count(coef) + unbiased_exp < E_MIN + 1`.
    if status.inexact() {
        let digits = coef.decimal_digit_count() as i32;
        if digits + unbiased_exp < E_MIN + 1 && !coef.is_zero() {
            status |= Status::UNDERFLOW;
        }
    }
    (
        Decimal128::from_bits(pack_finite(sign, biased_exp as u32, coef.to_u128())),
        status,
    )
}

/// Shift `coef` right by `n` decimal digits, returning the quotient, the
/// sticky bit (any non-zero digit beyond the round digit), and the round
/// digit (the most-significant dropped digit).
fn shift_right_decimal(coef: U256, n: u32) -> (U256, bool, u32) {
    let (kept, round_digit, sticky) = extract_dropped_digits(coef, n, false);
    (kept, sticky, round_digit)
}

/// What an overflowed result decodes to under each rounding direction:
/// either signed Infinity (the usual case) or ± `MAX` (when the mode
/// rounds *toward* the underflow direction relative to sign).
fn overflow_result(sign: bool, rm: RoundingMode) -> Decimal128 {
    match rm {
        RoundingMode::TowardZero => {
            if sign {
                Decimal128::MIN
            } else {
                Decimal128::MAX
            }
        }
        RoundingMode::TowardPositive => {
            if sign {
                Decimal128::MIN
            } else {
                Decimal128::from_bits(pack_infinity(false))
            }
        }
        RoundingMode::TowardNegative => {
            if sign {
                Decimal128::from_bits(pack_infinity(true))
            } else {
                Decimal128::MAX
            }
        }
        RoundingMode::NearestEven | RoundingMode::NearestAway => {
            Decimal128::from_bits(pack_infinity(sign))
        }
    }
}

#[inline]
const fn biased(unbiased_exp: i32) -> u32 {
    (unbiased_exp + BIAS as i32) as u32
}
