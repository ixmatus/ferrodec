//! IEEE 754-2019 rounding-decision function shared across the
//! ferrodec family.
//!
//! Every precision's `round_and_pack_finite` calls this to decide
//! whether the kept-coefficient should be bumped by `+1` given the
//! rounding mode, the result's sign, the next dropped digit, and the
//! sticky bit accumulated from the digits further down. The decision
//! is purely a function of these five inputs (plus the `RoundingMode`
//! enum), so it is precision-agnostic and lives here next to
//! [`RoundingMode`](crate::RoundingMode).

use crate::RoundingMode;

/// Decide whether to bump the kept coefficient by +1.
///
/// Inputs:
///
/// * `rm` — the active rounding mode.
/// * `sign` — the result's sign (`true` if negative).
/// * `last_kept_lsb` — the *low-order decimal digit* of the kept
///   coefficient (i.e. `kept % 10`). Used for the
///   [`RoundingMode::NearestEven`] tie-break.
/// * `round_digit` — the next decimal digit *immediately below* the
///   kept coefficient. In `[0, 9]`.
/// * `sticky` — `true` iff at least one *further* digit beneath
///   `round_digit` is non-zero.
///
/// Behaviour follows IEEE 754-2019 §4.3.3:
///
/// | Mode | Round up iff |
/// | --- | --- |
/// | `NearestEven` | `round_digit > 5`, or `round_digit == 5 &&` (`sticky` or `last_kept_lsb` is odd) |
/// | `NearestAway` | `round_digit >= 5` |
/// | `TowardZero` | never |
/// | `TowardPositive` | result is positive and some dropped digit is non-zero |
/// | `TowardNegative` | result is negative and some dropped digit is non-zero |
///
/// Returns `false` immediately when the dropped digits are all zero:
/// the result is then exact and no mode rounds away from an exact
/// value.
#[must_use]
pub const fn should_round_up(
    rm: RoundingMode,
    sign: bool,
    last_kept_lsb: u32,
    round_digit: u32,
    sticky: bool,
) -> bool {
    // Exact result: never round up.
    let dropped_nonzero = round_digit != 0 || sticky;
    if !dropped_nonzero {
        return false;
    }
    match rm {
        RoundingMode::TowardZero => false,
        RoundingMode::TowardPositive => !sign,
        RoundingMode::TowardNegative => sign,
        RoundingMode::NearestAway => round_digit >= 5,
        RoundingMode::NearestEven => {
            if round_digit < 5 {
                false
            } else if round_digit > 5 {
                true
            } else {
                // Halfway: round to even. Sticky means the dropped
                // digits are *above* the halfway mark, so round up.
                sticky || (last_kept_lsb & 1) == 1
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_never_rounds_up() {
        for rm in [
            RoundingMode::NearestEven,
            RoundingMode::NearestAway,
            RoundingMode::TowardZero,
            RoundingMode::TowardPositive,
            RoundingMode::TowardNegative,
        ] {
            assert!(!should_round_up(rm, false, 7, 0, false));
            assert!(!should_round_up(rm, true, 7, 0, false));
        }
    }

    #[test]
    fn nearest_even_tie_break() {
        // round_digit == 5, sticky == false: tie. Round-to-even.
        // Last kept LSB 4 (even) → no round-up.
        assert!(!should_round_up(
            RoundingMode::NearestEven,
            false,
            4,
            5,
            false
        ));
        // Last kept LSB 5 (odd) → round up.
        assert!(should_round_up(
            RoundingMode::NearestEven,
            false,
            5,
            5,
            false
        ));
        // sticky breaks the tie above halfway → round up.
        assert!(should_round_up(
            RoundingMode::NearestEven,
            false,
            4,
            5,
            true
        ));
    }

    #[test]
    fn nearest_away_at_halfway() {
        assert!(should_round_up(
            RoundingMode::NearestAway,
            false,
            4,
            5,
            false
        ));
        // Just below halfway: no round up.
        assert!(!should_round_up(
            RoundingMode::NearestAway,
            false,
            4,
            4,
            true
        ));
    }

    #[test]
    fn directional_modes_respect_sign() {
        // TowardPositive: positive result with non-zero dropped → up.
        assert!(should_round_up(
            RoundingMode::TowardPositive,
            false,
            0,
            1,
            false
        ));
        // TowardPositive: negative result never rounds up.
        assert!(!should_round_up(
            RoundingMode::TowardPositive,
            true,
            0,
            9,
            true
        ));
        // TowardNegative: mirror of the above.
        assert!(!should_round_up(
            RoundingMode::TowardNegative,
            false,
            0,
            9,
            true
        ));
        assert!(should_round_up(
            RoundingMode::TowardNegative,
            true,
            0,
            1,
            false
        ));
    }

    #[test]
    fn toward_zero_never_rounds_up() {
        assert!(!should_round_up(
            RoundingMode::TowardZero,
            false,
            0,
            9,
            true
        ));
        assert!(!should_round_up(
            RoundingMode::TowardZero,
            true,
            0,
            9,
            true
        ));
    }
}
