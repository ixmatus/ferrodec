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
        if q_clamped != q {
            // IEEE 754-2019 §7.4 Clamped (informational): the zero is
            // exact at every exponent, so the value is unchanged, but
            // its preferred quantum fell outside the format range and
            // was clamped into it. Mirrors decimal64 round.rs.
            status |= Status::CLAMPED;
        }
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
    // Single-rounding subnormal fix (fd-42l): drop to the wider of the
    // precision and subnormal-quantum requirements in ONE rounding.
    let qmin = -(BIAS as i32);
    let precision_excess = digits.saturating_sub(PRECISION);
    let subnormal_excess = u32::try_from((qmin - unbiased_exp).max(0)).unwrap_or(u32::MAX);
    let excess = precision_excess.max(subnormal_excess);
    let (kept, kept_exp, round_digit, sticky) = if excess == 0 {
        (coef, unbiased_exp, 0u32, pre_sticky)
    } else if excess >= digits {
        let (rd, st) = round_digit_for_full_drop(coef, excess, digits);
        (
            U256::from_u128(0),
            unbiased_exp + excess as i32,
            rd,
            st || pre_sticky,
        )
    } else {
        drop_excess_digits(coef, excess, pre_sticky, unbiased_exp)
    };
    let mut kept_digits = digits.saturating_sub(excess).clamp(1, PRECISION);

    if round_digit != 0 || sticky {
        status |= Status::INEXACT;
    }
    // IEEE 754-2019 §7.5 / GDA: UNDERFLOW signals on a result that is
    // *tiny and inexact*, with tininess detected on the **pre-rounding**
    // value (the convention decTest pins, cf. fd-99f). The single-step
    // subnormal drop here makes `finalize_finite`'s `biased_exp < 0`
    // branch unreachable for non-zero results, and its post-rounding
    // tininess check keys on the *rounded* digit count — which misses a
    // subnormal value that rounding lifts to the Emin boundary
    // (dqfma2908 / dqmul908). Decide tininess from the original
    // coefficient's adjusted exponent instead.
    let tiny_pre = digits as i32 + unbiased_exp - 1 < E_MIN;
    if tiny_pre && status.inexact() {
        status |= Status::UNDERFLOW;
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
        let down_target = q_preferred.max(qmin);
        if exp_after > down_target {
            let max_shift = PRECISION as i32 - kept_digits as i32;
            let want_shift = exp_after - down_target;
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

    // Step 4 + 5: pack with overflow / underflow checks. `q_ideal` is the
    // operation's preferred quantum before any range clamp; `finalize_finite`
    // uses it to raise §7.4 CLAMPED when a zero result's ideal exponent fell
    // out of range, because the Step-1 subnormal drop has already pulled
    // `exp_after` up to qmin and the delivered exponent no longer reveals the
    // clamp (fd-61r / ADR-0048).
    finalize_finite(
        rounded,
        exp_after,
        sign,
        rm,
        status,
        q_preferred.min(unbiased_exp),
    )
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
    q_ideal: i32,
) -> (Decimal128, Status) {
    let qmin = -(BIAS as i32);
    let qmax = BIASED_EXP_MAX as i32 - BIAS as i32;
    if coef.is_zero() {
        // Never an exception path — emit canonical zero with the given exp
        // (clamped if it falls out of range).
        let clamped_exp = unbiased_exp.clamp(qmin, qmax);
        if q_ideal < qmin || q_ideal > qmax {
            // §7.4 Clamped (informational): zero is exact at every
            // exponent, but its preferred quantum fell outside the format
            // range and was clamped into it. `q_ideal` is checked rather
            // than the delivered `unbiased_exp` because a subnormal
            // underflow that rounds to zero has already had its exponent
            // pulled up to qmin by the Step-1 drop (e.g. dqdiv1755
            // `1e-4277 / 1e+3311 -> 0E-6176 Clamped`).
            status |= Status::CLAMPED;
        }
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
        // §7.4 Clamped (informational): the preferred quantum exceeded the
        // format range and was pulled down to qmax, the coefficient
        // absorbing the difference as trailing zeros. The value is exact;
        // only the quantum was constrained. This branch is reachable only
        // when the ideal quantum genuinely exceeds qmax (the scale check
        // above already returned OVERFLOW for out-of-magnitude results, and
        // the Step 3.5 down-shift reaches qmax whenever the ideal is in
        // range), so the clamp is never spurious (fd-61r / ADR-0048).
        status |= Status::CLAMPED;
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

/// Width-bounded analogue of the rounding kernel's decision core, the
/// formal-verification target for S7 (ADR-0021).
///
/// [`round_and_pack_finite`] runs its drop / decide / power-of-ten-carry
/// core (Steps 1–3) on a `U256` over the full 34-digit domain — the
/// `decimal_digit_count` walk and the `div_rem10` drop loop make that
/// pipeline intractable for CBMC, so it stays out of Kani scope by
/// ADR-0016. The *logic* of that core is width-independent, so this
/// module reproduces it at the narrowest faithful width — `u32`, the
/// decimal32 kernel shape (`p = 7`, significand `< 10^9`). It is also
/// deliberately **loop-free**: the digit count is a comparison ladder
/// and the digit drop selects a *literal* power-of-ten divisor by a
/// `match` on the drop amount, so CBMC never has to unroll a loop or
/// bit-blast a symbolic-by-symbolic division. That is what keeps the
/// equivalence proof tractable; an earlier `u128`, loop-based draft did
/// not terminate (the plan's fallback ladder anticipated exactly this
/// and the narrowing is its prescribed remedy).
///
/// The single decision point — `should_round_up` — is the exact
/// production function, already proven exhaustively against the IEEE
/// 754-2019 §4.3.3 table by S6, so the kernel proof composes with the
/// decision proof rather than re-deriving it.
#[cfg(any(test, kani))]
pub(crate) mod bounded_kernel {
    use super::{should_round_up, RoundingMode};

    /// `10^k` for `k ≤ 9` (every value `≤ 10^9` fits `u32`). A `match`,
    /// not a loop, so CBMC sees a constant.
    pub(crate) const fn pow10_u32(k: u32) -> u32 {
        match k {
            0 => 1,
            1 => 10,
            2 => 100,
            3 => 1_000,
            4 => 10_000,
            5 => 100_000,
            6 => 1_000_000,
            7 => 10_000_000,
            8 => 100_000_000,
            _ => 1_000_000_000,
        }
    }

    /// Decimal digit count of `n` (with `0` having one digit) for
    /// `n < 10^9`, by a comparison ladder — loop-free, so CBMC encodes
    /// it as a small decision tree rather than an unrolled division
    /// chain.
    pub(crate) const fn decimal_digits_u32(n: u32) -> u32 {
        if n < 10 {
            1
        } else if n < 100 {
            2
        } else if n < 1_000 {
            3
        } else if n < 10_000 {
            4
        } else if n < 100_000 {
            5
        } else if n < 1_000_000 {
            6
        } else if n < 10_000_000 {
            7
        } else if n < 100_000_000 {
            8
        } else {
            9
        }
    }

    /// Round the integer significand `coef` to at most `p` significant
    /// decimal digits under `rm`, mirroring Steps 1–3 of
    /// [`super::round_and_pack_finite`] at fixed `u32` width:
    ///
    /// 1. Drop the low `digits − p` digits, recovering the round digit
    ///    and the sticky bit by a single division with a *literal*
    ///    power-of-ten divisor (the closed form of the production
    ///    `extract_dropped_digits` loop).
    /// 2. Decide via the production [`should_round_up`].
    /// 3. Increment on a round-up and divide back by ten if that crossed
    ///    the `10^p` boundary, bumping the exponent.
    ///
    /// Returns `(rounded_coef, exp_delta)`, `exp_delta ∈ {0, 1}` (the
    /// power-of-ten carry). Quantum padding, overflow / underflow and
    /// packing are deliberately *not* modelled here: they are separate
    /// concerns downstream of the rounding decision and out of the
    /// width-bounded kernel's scope.
    pub(crate) fn round_to_p_digits(coef: u32, p: u32, sign: bool, rm: RoundingMode) -> (u32, i32) {
        let digits = decimal_digits_u32(coef);
        if digits <= p {
            return (coef, 0);
        }
        let drop = digits - p;

        // Split off the dropped tail with a constant divisor chosen by
        // the (small) drop amount. `below = 10^(drop-1)` isolates the
        // most-significant dropped digit (the round digit); everything
        // beneath it is the sticky tail.
        let divisor = pow10_u32(drop);
        let below = pow10_u32(drop - 1);
        let kept = coef / divisor;
        let dropped = coef % divisor;
        let round_digit = (dropped / below) % 10;
        let sticky = dropped % below != 0;

        let last_kept_lsb = kept % 10;
        let mut rounded = kept;
        let mut exp_delta = 0i32;
        if should_round_up(rm, sign, last_kept_lsb, round_digit, sticky) {
            rounded += 1;
            if rounded == pow10_u32(p) {
                rounded /= 10;
                exp_delta = 1;
            }
        }
        (rounded, exp_delta)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        const MODES: [RoundingMode; 5] = [
            RoundingMode::NearestEven,
            RoundingMode::NearestAway,
            RoundingMode::TowardZero,
            RoundingMode::TowardPositive,
            RoundingMode::TowardNegative,
        ];

        #[test]
        fn no_drop_when_within_precision() {
            assert_eq!(
                round_to_p_digits(1_234_567, 7, false, RoundingMode::NearestEven),
                (1_234_567, 0)
            );
            assert_eq!(
                round_to_p_digits(42, 7, true, RoundingMode::TowardNegative),
                (42, 0)
            );
        }

        #[test]
        fn half_even_tie_breaks_to_even() {
            // 1_234_565 with a single dropped "5", nothing below ⇒ exact
            // tie; kept LSB 5 is odd ⇒ round up to 1_234_566.
            assert_eq!(
                round_to_p_digits(12_345_655, 7, false, RoundingMode::NearestEven),
                (1_234_566, 0)
            );
            // 1_234_560 with a dropped "5" ⇒ tie; kept LSB 0 even ⇒
            // stays 1_234_560.
            assert_eq!(
                round_to_p_digits(12_345_605, 7, false, RoundingMode::NearestEven),
                (1_234_560, 0)
            );
        }

        #[test]
        fn directed_modes_follow_sign() {
            // Drop "9": toward +∞ rounds a positive up, toward −∞ leaves
            // it, and the mirror holds for a negative significand.
            assert_eq!(
                round_to_p_digits(12_345_679, 7, false, RoundingMode::TowardPositive),
                (1_234_568, 0)
            );
            assert_eq!(
                round_to_p_digits(12_345_679, 7, false, RoundingMode::TowardNegative),
                (1_234_567, 0)
            );
            assert_eq!(
                round_to_p_digits(12_345_679, 7, true, RoundingMode::TowardNegative),
                (1_234_568, 0)
            );
        }

        #[test]
        fn power_of_ten_carry_bumps_exponent() {
            // 9_999_999 with a dropped "9" rounds up to 10_000_000 →
            // 1_000_000 e+1.
            assert_eq!(
                round_to_p_digits(99_999_999, 7, false, RoundingMode::NearestEven),
                (1_000_000, 1)
            );
        }

        #[test]
        fn truncation_toward_zero_never_increments() {
            assert_eq!(
                round_to_p_digits(99_999_999, 7, false, RoundingMode::TowardZero),
                (9_999_999, 0)
            );
        }

        /// Bounded *exhaustive* concrete check: a dense grid of
        /// significands × both signs × all five modes, cross-checked
        /// against an independently computed reference. This is the
        /// plan's "kernel via bounded-check" carrier — it holds the
        /// empirical weight in `cargo test` regardless of CBMC's mood,
        /// and runs in well under a second.
        #[test]
        fn exhaustive_grid_matches_reference() {
            // Independent reference: digit count by repeated division
            // (a different method to the kernel's comparison ladder),
            // and the §4.3.3 decision transcribed afresh.
            fn ref_digits(mut n: u32) -> u32 {
                if n == 0 {
                    return 1;
                }
                let mut d = 0;
                while n != 0 {
                    n /= 10;
                    d += 1;
                }
                d
            }
            fn ref_up(rm: RoundingMode, sign: bool, lsb: u32, rd: u32, st: bool) -> bool {
                let any = rd != 0 || st;
                match rm {
                    RoundingMode::TowardZero => false,
                    RoundingMode::TowardPositive => any && !sign,
                    RoundingMode::TowardNegative => any && sign,
                    RoundingMode::NearestAway => rd >= 5,
                    RoundingMode::NearestEven => rd > 5 || (rd == 5 && (st || lsb % 2 == 1)),
                }
            }
            fn reference(coef: u32, p: u32, sign: bool, rm: RoundingMode) -> (u32, i32) {
                let digits = ref_digits(coef);
                if digits <= p {
                    return (coef, 0);
                }
                let drop = digits - p;
                let divisor = pow10_u32(drop);
                let below = pow10_u32(drop - 1);
                let kept = coef / divisor;
                let dropped = coef % divisor;
                let rd = (dropped / below) % 10;
                let st = dropped % below != 0;
                let mut r = kept;
                let mut ed = 0;
                if ref_up(rm, sign, kept % 10, rd, st) {
                    r += 1;
                    if r == pow10_u32(p) {
                        r /= 10;
                        ed = 1;
                    }
                }
                (r, ed)
            }

            // Walk a representative grid: every value 0..=20_000 (covers
            // the digit-count ladder up to 5 digits and all carry edges
            // at p ≤ 4), plus the 7-digit boundary band around 10^7.
            for p in [2u32, 4, 7] {
                for coef in (0u32..=20_000).chain(9_999_990..=10_000_010) {
                    for sign in [false, true] {
                        for rm in MODES {
                            assert_eq!(
                                round_to_p_digits(coef, p, sign, rm),
                                reference(coef, p, sign, rm),
                                "coef={coef} p={p} sign={sign} rm={rm:?}"
                            );
                        }
                    }
                }
            }
        }
    }
}
