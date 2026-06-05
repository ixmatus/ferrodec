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
    pack_finite, pack_infinity, BiasedExp, Coefficient, BIAS, BIASED_EXP_MAX, COEFFICIENT_LIMIT,
    E_MIN, PRECISION,
};
use crate::decimal::Decimal32;
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
        // q_clamped is in the representable unbiased range by construction.
        let biased_exp = BiasedExp::try_from_unbiased(q_clamped)
            .expect("q_clamped in [-BIAS, BIASED_EXP_MAX - BIAS]");
        if q_clamped != q {
            // §7.4 Clamped (informational): the zero is exact at every
            // exponent, but its preferred quantum fell outside the format
            // range and was clamped into it (fd-61r / ADR-0048).
            status |= Status::CLAMPED;
        }
        return (
            Decimal32::from_bits(pack_finite(sign, biased_exp, Coefficient::ZERO)),
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
    // `fma(3.142290e-17, -2.033196e-78, 5.38890e-95)` produced a
    // coefficient ending `2` instead of `1`). Drop to the wider of the
    // PRECISION and subnormal-quantum requirements in ONE rounding,
    // the sibling analogue of the parent `Decimal128` fd-42l fix. This
    // leaves `finalise_finite`'s `biased < 0` arm unreachable for
    // non-zero results.
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
        // IEEE 754-2019 §6.3: the down-shift target is
        // MAX(q_preferred, qmin); padding toward a quantum below qmin
        // is not representable. (fd-dc6, matching the parent fd-42l.)
        let down_target = q_preferred.max(qmin);
        if exp_after > down_target {
            // Pad with trailing zeros up to PRECISION digits.
            let max_shift = PRECISION as i32 - kept_digits as i32;
            let want_shift = exp_after - down_target;
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

    finalise_finite(
        rounded,
        exp_after,
        sign,
        rm,
        status,
        q_preferred.min(unbiased_exp),
    )
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
/// Finalise a rounded `(coef, unbiased_exp, sign)` by checking
/// overflow / underflow and packing.
fn finalise_finite(
    coef: u64,
    unbiased_exp: i32,
    sign: bool,
    rm: RoundingMode,
    mut status: Status,
    q_ideal: i32,
) -> (Decimal32, Status) {
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
        if q_ideal < -bias || q_ideal > biased_exp_max - bias {
            // §7.4 Clamped (informational): the zero's preferred exponent
            // fell outside the format range and was clamped in. `q_ideal`
            // is checked rather than the delivered `biased` exponent because
            // a subnormal underflow that rounds to zero has already had its
            // exponent pulled up to qmin upstream (fd-61r / ADR-0048).
            status |= Status::CLAMPED;
        }
        return (
            Decimal32::from_bits(pack_finite(sign, biased_exp, Coefficient::ZERO)),
            status,
        );
    }

    if biased > biased_exp_max {
        // Try IEEE 754-2019 §6.3 exponent clamping: if the adjusted
        // exponent is within the format's range, pad the coefficient
        // with trailing zeros to bring biased down to BIASED_EXP_MAX.
        // This is the "Clamped" condition — informational only, no
        // OVERFLOW raised. The smallest case: `1E+96` packs as
        // `1000000E+90` (coef = 10^6, biased_exp = BIASED_EXP_MAX).
        let shift_needed = (biased - biased_exp_max) as u32;
        let digits = digit_count_u64(coef);
        if (digits + shift_needed) as i32 <= PRECISION as i32
            && (shift_needed as usize) < POW10_U64.len()
            && coef != 0
        {
            let shifted = coef * POW10_U64[shift_needed as usize];
            if shifted < u64::from(COEFFICIENT_LIMIT) {
                let shifted_coef =
                    Coefficient::try_new(shifted as u32).expect("shifted < COEFFICIENT_LIMIT");
                // §7.4 Clamped (informational): the preferred quantum
                // exceeded the format range and was pulled down to qmax, the
                // coefficient absorbing the difference as trailing zeros. The
                // value is exact (fd-61r / ADR-0048; `1E+96` packs as
                // `1000000E+90`).
                status |= Status::CLAMPED;
                return (
                    Decimal32::from_bits(pack_finite(sign, BiasedExp::MAX, shifted_coef)),
                    status,
                );
            }
        }
        // Genuine overflow: rounded magnitude exceeds MAX. Per
        // IEEE 754-2019 §7.4, the result depends on the rounding mode.
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
            pack_finite(sign, BiasedExp::MAX, Coefficient::MAX)
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
        // A `biased < 0` result is subnormal by construction. IEEE
        // 754-2019 §7.5 signals UNDERFLOW when a subnormal result is
        // inexact, where inexactness can arise either from this arm's
        // own digit drop *or* from an earlier precision rounding the
        // caller already recorded in `status` (the fd-9fi sibling-FMA
        // exact-oracle sweep surfaced `fma(-5.738903e-42,
        // 5.487024e-55, -0e-101)`: the 14→7 precision drop set
        // INEXACT, then this arm's 1-digit shift was exact, so the old
        // `round_digit != 0 || sticky` test mistook it for "no
        // underflow"). Port of the decimal64 fd-99f / M1 rule, which
        // decimal32 lacked.
        if round_digit != 0 || sticky {
            status |= Status::INEXACT;
        }
        if status.inexact() {
            status |= Status::UNDERFLOW;
        }
        // After rounding the subnormal could cross back over to normal
        // if it gained a digit (e.g. 9_999_999 + ulp = 10_000_000): in
        // that case re-pack at biased_exp = 1.
        if final_coef >= u64::from(COEFFICIENT_LIMIT) {
            let bumped = final_coef / 10;
            // bumped < COEFFICIENT_LIMIT by construction (final_coef < 10 * COEFFICIENT_LIMIT).
            let bumped_coef =
                Coefficient::try_new(bumped as u32).expect("bumped < COEFFICIENT_LIMIT");
            return (
                Decimal32::from_bits(pack_finite(
                    sign,
                    BiasedExp::try_from_biased(1).unwrap(),
                    bumped_coef,
                )),
                status,
            );
        }
        let final_coefficient =
            Coefficient::try_new(final_coef as u32).expect("final_coef < COEFFICIENT_LIMIT");
        return (
            Decimal32::from_bits(pack_finite(sign, BiasedExp::MIN, final_coefficient)),
            status,
        );
    }

    // biased ∈ [0, biased_exp_max] from the if-arms above, coef < COEFFICIENT_LIMIT.
    //
    // IEEE 754-2019 §7.5: a result that is representable (biased ≥ 0)
    // but tiny is still subnormal when its adjusted exponent falls
    // below E_MIN. The deeply-subnormal `biased < 0` arm above already
    // raises UNDERFLOW; this catches the representable subnormal that
    // the `biased < 0` test misses (the fd-9fi sibling-FMA exact-oracle
    // sweep surfaced `fma(-5.738903e-42, 5.487024e-55, -0e-101)` as
    // Inexact only, want Underflow Inexact). Underflow is signalled
    // only together with inexactness (an exact subnormal is `Subnormal`
    // but not `Underflow`), so it gates on the INEXACT the rounding
    // step already accumulated. Port of the decimal64 fd-99f / M1 rule
    // (`finalise_finite`), which decimal32 lacked.
    let adjusted_exp = unbiased_exp + digit_count_u64(coef) as i32 - 1;
    if adjusted_exp < E_MIN && status.inexact() {
        status |= Status::UNDERFLOW;
    }

    let biased_exp =
        BiasedExp::try_from_biased(biased as u32).expect("biased in [0, BIASED_EXP_MAX]");
    let coefficient = Coefficient::try_new(coef as u32).expect("coef < COEFFICIENT_LIMIT");
    (
        Decimal32::from_bits(pack_finite(sign, biased_exp, coefficient)),
        status,
    )
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
    fn round_seven_digits_exact() {
        let (d, s) = round_and_pack_finite(
            9_999_999,
            0,
            0,
            false,
            false,
            RoundingMode::NearestEven,
            Status::OK,
        );
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
    fn round_clamp_at_emax_nearest() {
        // 1E+96 has adjusted exponent `e + digits − 1 = 96 + 1 − 1 = 96
        // = E_MAX`, so the value is in range; biased = 96 + 101 = 197 >
        // BIASED_EXP_MAX = 191. IEEE 754-2019 §6.3 says: pad the
        // coefficient with trailing zeros to bring the biased exponent
        // down to BIASED_EXP_MAX — the "Clamped" condition. No
        // OVERFLOW raised; the result is exact.
        let (d, s) = round_and_pack_finite(
            1,
            96,
            96,
            false,
            false,
            RoundingMode::NearestEven,
            Status::OK,
        );
        // Expect 1000000E+90: coef = 10^6, biased_exp = BIASED_EXP_MAX
        // = 191. The local `pack` helper takes the unbiased quantum
        // (90 = 191 − BIAS = 191 − 101).
        assert_eq!(
            d.to_bits(),
            pack(false, BIASED_EXP_MAX as i32 - BIAS as i32, 1_000_000).to_bits()
        );
        assert!(!s.overflow());
        assert!(!s.inexact());
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
            200,
            200,
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
        // 1E+97: adjusted = 97 > E_MAX = 96, so this is genuine
        // overflow. Even after attempted clamping the digit budget is
        // insufficient (would need 1 followed by 7 zeros = 8 digits >
        // PRECISION = 7).
        let (d, s) = round_and_pack_finite(
            1,
            97,
            97,
            false,
            false,
            RoundingMode::NearestEven,
            Status::OK,
        );
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
