//! IEEE 754 exception flags and rounding modes.
//!
//! Operations return `(result, Status)` rather than mutating any global or
//! thread-local state. Callers accumulate status across a sequence of
//! operations with [`Status::merge`] or `|=`.

use core::ops::{BitOr, BitOrAssign};

/// IEEE 754 exception flags raised by an operation.
///
/// The five flags are packed in a single byte. A freshly constructed
/// [`Status`] has no flags set ([`Status::OK`]).
///
/// Flags follow IEEE 754-2019 §7:
/// - `INVALID`     — operation has no useful definition (e.g. `0/0`, `Inf-Inf`, sNaN input)
/// - `DIV_BY_ZERO` — finite, non-zero numerator divided by zero
/// - `OVERFLOW`    — rounded result exceeds the largest finite magnitude
/// - `UNDERFLOW`   — non-zero result smaller than the smallest normal magnitude (after rounding)
/// - `INEXACT`     — rounded result differs from the infinitely precise result
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
#[repr(transparent)]
pub struct Status(u8);

impl Status {
    const F_INVALID: u8 = 0b0000_0001;
    const F_DIV_BY_ZERO: u8 = 0b0000_0010;
    const F_OVERFLOW: u8 = 0b0000_0100;
    const F_UNDERFLOW: u8 = 0b0000_1000;
    const F_INEXACT: u8 = 0b0001_0000;

    const ALL: u8 = Self::F_INVALID
        | Self::F_DIV_BY_ZERO
        | Self::F_OVERFLOW
        | Self::F_UNDERFLOW
        | Self::F_INEXACT;

    /// No flags raised.
    pub const OK: Self = Self(0);
    /// Operation has no useful definition (e.g. `0/0`, `Inf - Inf`, sNaN operand).
    pub const INVALID: Self = Self(Self::F_INVALID);
    /// Finite, non-zero numerator divided by zero.
    pub const DIV_BY_ZERO: Self = Self(Self::F_DIV_BY_ZERO);
    /// Rounded result exceeds the largest finite magnitude.
    pub const OVERFLOW: Self = Self(Self::F_OVERFLOW);
    /// Non-zero result is smaller than the smallest normal magnitude.
    pub const UNDERFLOW: Self = Self(Self::F_UNDERFLOW);
    /// Rounded result differs from the infinitely precise result.
    pub const INEXACT: Self = Self(Self::F_INEXACT);

    /// Construct a `Status` from raw flag bits, masking unknown bits.
    #[inline]
    #[must_use]
    pub const fn from_bits_truncate(bits: u8) -> Self {
        Self(bits & Self::ALL)
    }

    /// Return the underlying flag byte.
    #[inline]
    #[must_use]
    pub const fn bits(self) -> u8 {
        self.0
    }

    /// `true` when no flags are raised.
    #[inline]
    #[must_use]
    pub const fn is_ok(self) -> bool {
        self.0 == 0
    }

    /// `true` when the `INVALID` flag is raised.
    #[inline]
    #[must_use]
    pub const fn invalid(self) -> bool {
        self.0 & Self::F_INVALID != 0
    }

    /// `true` when the `DIV_BY_ZERO` flag is raised.
    #[inline]
    #[must_use]
    pub const fn div_by_zero(self) -> bool {
        self.0 & Self::F_DIV_BY_ZERO != 0
    }

    /// `true` when the `OVERFLOW` flag is raised.
    #[inline]
    #[must_use]
    pub const fn overflow(self) -> bool {
        self.0 & Self::F_OVERFLOW != 0
    }

    /// `true` when the `UNDERFLOW` flag is raised.
    #[inline]
    #[must_use]
    pub const fn underflow(self) -> bool {
        self.0 & Self::F_UNDERFLOW != 0
    }

    /// `true` when the `INEXACT` flag is raised.
    #[inline]
    #[must_use]
    pub const fn inexact(self) -> bool {
        self.0 & Self::F_INEXACT != 0
    }

    /// Union of two flag sets.
    #[inline]
    #[must_use]
    pub const fn merge(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

impl BitOr for Status {
    type Output = Self;
    #[inline]
    fn bitor(self, rhs: Self) -> Self {
        self.merge(rhs)
    }
}

impl BitOrAssign for Status {
    #[inline]
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

/// IEEE 754 rounding-direction attribute.
///
/// The default is [`RoundingMode::NearestEven`] — IEEE 754 §4.3.3
/// `roundTiesToEven`, the only direction that round-trips cleanly through
/// arithmetic.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum RoundingMode {
    /// Round to nearest, ties to even (IEEE 754 default, banker's rounding).
    #[default]
    NearestEven,
    /// Round to nearest, ties away from zero.
    NearestAway,
    /// Round toward zero (truncate).
    TowardZero,
    /// Round toward positive infinity (ceiling).
    TowardPositive,
    /// Round toward negative infinity (floor).
    TowardNegative,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ok_is_zero() {
        let s = Status::OK;
        assert!(s.is_ok());
        assert!(!s.invalid());
        assert!(!s.div_by_zero());
        assert!(!s.overflow());
        assert!(!s.underflow());
        assert!(!s.inexact());
        assert_eq!(s.bits(), 0);
    }

    #[test]
    fn each_flag_is_disjoint() {
        let flags = [
            (Status::INVALID, Status::F_INVALID),
            (Status::DIV_BY_ZERO, Status::F_DIV_BY_ZERO),
            (Status::OVERFLOW, Status::F_OVERFLOW),
            (Status::UNDERFLOW, Status::F_UNDERFLOW),
            (Status::INEXACT, Status::F_INEXACT),
        ];
        for (i, (a, _)) in flags.iter().enumerate() {
            for (j, (b, _)) in flags.iter().enumerate() {
                if i != j {
                    assert_eq!(a.bits() & b.bits(), 0, "flags {i} and {j} overlap");
                }
            }
        }
    }

    #[test]
    fn merge_unions_flags() {
        let a = Status::INVALID | Status::INEXACT;
        let b = Status::OVERFLOW | Status::INEXACT;
        let m = a.merge(b);
        assert!(m.invalid());
        assert!(m.overflow());
        assert!(m.inexact());
        assert!(!m.div_by_zero());
        assert!(!m.underflow());
    }

    #[test]
    fn bitor_assign_accumulates() {
        let mut s = Status::OK;
        s |= Status::INEXACT;
        s |= Status::OVERFLOW;
        assert!(s.inexact());
        assert!(s.overflow());
    }

    #[test]
    fn from_bits_truncate_drops_unknown_bits() {
        let s = Status::from_bits_truncate(0xFF);
        assert_eq!(s.bits(), Status::ALL);
    }

    #[test]
    fn rounding_mode_default_is_nearest_even() {
        assert_eq!(RoundingMode::default(), RoundingMode::NearestEven);
    }
}
