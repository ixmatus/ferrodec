//! The arithmetic context and rounding modes.
//!
//! A [`Context`] carries the working precision, the adjusted-exponent bounds,
//! the rounding mode, and the clamp flag. It is passed by reference to each
//! operation, which returns a per-operation `Status` rather than mutating
//! global state, following ADR-0002 and ADR-0003.

use core::num::NonZeroU32;
use ferrodec_ieee::{should_round_up, RoundingMode};

/// The eight General Decimal Arithmetic rounding modes.
///
/// Five coincide with the IEEE 754 directions the fixed-width ferrodec formats
/// already implement and delegate to the Kani-proven
/// [`ferrodec_ieee::should_round_up`]. The other three (`HalfDown`, `Up`,
/// `ZeroFiveUp`) are the ones the fixed formats deliberately decline
/// (ADR-0005) and are decided here.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Rounding {
    /// Round to nearest; ties to even. The General Decimal Arithmetic default.
    #[default]
    HalfEven,
    /// Round to nearest; ties away from zero.
    HalfUp,
    /// Round to nearest; ties toward zero.
    HalfDown,
    /// Round toward zero (truncate).
    Down,
    /// Round away from zero.
    Up,
    /// Round toward positive infinity.
    Ceiling,
    /// Round toward negative infinity.
    Floor,
    /// Round toward zero unless the discarded digits would carry the kept
    /// coefficient's least digit to `0` or `5`, in which case round away.
    ZeroFiveUp,
}

impl Rounding {
    /// Decide whether to increment the kept coefficient by one. This is the
    /// General Decimal Arithmetic analogue of the IEEE-only
    /// [`ferrodec_ieee::should_round_up`], extended to the eight modes; the
    /// rounding core routes every rounding decision through it.
    ///
    /// Inputs match [`ferrodec_ieee::should_round_up`]: the result `sign`, the
    /// low decimal digit `last_kept` of the kept coefficient, the next dropped
    /// digit `round_digit` in `[0, 9]`, and the `sticky` bit (any further
    /// dropped digit non-zero).
    #[must_use]
    pub fn round_up(self, sign: bool, last_kept: u32, round_digit: u32, sticky: bool) -> bool {
        let any_dropped = round_digit != 0 || sticky;
        match self {
            Rounding::HalfEven => should_round_up(
                RoundingMode::NearestEven,
                sign,
                last_kept,
                round_digit,
                sticky,
            ),
            Rounding::HalfUp => should_round_up(
                RoundingMode::NearestAway,
                sign,
                last_kept,
                round_digit,
                sticky,
            ),
            Rounding::Down => should_round_up(
                RoundingMode::TowardZero,
                sign,
                last_kept,
                round_digit,
                sticky,
            ),
            Rounding::Ceiling => should_round_up(
                RoundingMode::TowardPositive,
                sign,
                last_kept,
                round_digit,
                sticky,
            ),
            Rounding::Floor => should_round_up(
                RoundingMode::TowardNegative,
                sign,
                last_kept,
                round_digit,
                sticky,
            ),
            // Ties toward zero: round up only when the tail is strictly past
            // the halfway point.
            Rounding::HalfDown => round_digit > 5 || (round_digit == 5 && sticky),
            // Away from zero on any non-zero discarded tail.
            Rounding::Up => any_dropped,
            // Round half-five up: only bumps when the kept last digit is 0 or 5.
            Rounding::ZeroFiveUp => any_dropped && (last_kept == 0 || last_kept == 5),
        }
    }
}

/// The arithmetic context.
///
/// `precision` is the working precision in decimal digits; the
/// [`NonZeroU32`] type makes a zero precision unrepresentable rather than
/// documented-away (ADR-0054). `emax` and `emin` bound the *adjusted*
/// exponent of a finite result (the adjusted exponent is
/// `exponent + digits - 1`). `clamp` enables IEEE-style exponent clamping
/// of the result's quantum into the representable range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Context {
    /// Working precision in decimal digits.
    pub precision: NonZeroU32,
    /// Maximum adjusted exponent.
    pub emax: i32,
    /// Minimum adjusted exponent.
    pub emin: i32,
    /// Active rounding mode.
    pub rounding: Rounding,
    /// Whether to clamp the result's quantum to the representable range.
    pub clamp: bool,
}

impl Context {
    /// Build a context from a precision, an adjusted-exponent range, and a
    /// rounding mode, with clamping off.
    ///
    /// # Examples
    ///
    /// ```
    /// use core::num::NonZeroU32;
    /// use ferrodec_decimal::{Context, Rounding};
    ///
    /// const P34: NonZeroU32 = NonZeroU32::new(34).unwrap();
    /// let ctx = Context::new(P34, 6144, -6143, Rounding::HalfEven);
    /// assert_eq!(ctx.precision.get(), 34);
    /// ```
    #[must_use]
    pub const fn new(precision: NonZeroU32, emax: i32, emin: i32, rounding: Rounding) -> Self {
        Self {
            precision,
            emax,
            emin,
            rounding,
            clamp: false,
        }
    }

    /// Return a copy with the given rounding mode.
    #[must_use]
    pub const fn with_rounding(mut self, rounding: Rounding) -> Self {
        self.rounding = rounding;
        self
    }

    /// Return a copy with clamping enabled or disabled.
    #[must_use]
    pub const fn with_clamp(mut self, clamp: bool) -> Self {
        self.clamp = clamp;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_up_shared_modes_match_ieee() {
        // Spot-check that the shared modes delegate faithfully: half-even at a
        // tie with an even kept digit does not round up; with an odd one it
        // does.
        assert!(!Rounding::HalfEven.round_up(false, 4, 5, false));
        assert!(Rounding::HalfEven.round_up(false, 5, 5, false));
        // Down never rounds up; Ceiling rounds a positive inexact up.
        assert!(!Rounding::Down.round_up(false, 9, 9, true));
        assert!(Rounding::Ceiling.round_up(false, 0, 1, false));
        assert!(!Rounding::Ceiling.round_up(true, 0, 9, true));
    }

    #[test]
    fn round_up_gda_only_modes() {
        // HalfDown: exact half (round_digit 5, no sticky) rounds toward zero.
        assert!(!Rounding::HalfDown.round_up(false, 0, 5, false));
        assert!(Rounding::HalfDown.round_up(false, 0, 5, true));
        assert!(Rounding::HalfDown.round_up(false, 0, 6, false));
        // Up: any non-zero discarded tail rounds away.
        assert!(Rounding::Up.round_up(false, 0, 1, false));
        assert!(!Rounding::Up.round_up(false, 7, 0, false));
        // ZeroFiveUp: only when the kept last digit is 0 or 5.
        assert!(Rounding::ZeroFiveUp.round_up(false, 0, 1, false));
        assert!(Rounding::ZeroFiveUp.round_up(false, 5, 1, false));
        assert!(!Rounding::ZeroFiveUp.round_up(false, 4, 9, true));
    }

    #[test]
    fn context_builders() {
        let ctx = Context::new(
            core::num::NonZeroU32::new(34).unwrap(),
            6144,
            -6143,
            Rounding::HalfEven,
        )
        .with_rounding(Rounding::Down)
        .with_clamp(true);
        assert_eq!(ctx.precision.get(), 34);
        assert_eq!(ctx.rounding, Rounding::Down);
        assert!(ctx.clamp);
    }
}
