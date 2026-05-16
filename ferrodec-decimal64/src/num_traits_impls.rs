//! `num-traits` adapter for [`Decimal64`] (gated on the `num-traits`
//! feature, which transitively enables `ops` + `binary-float`).
//!
//! Implements the foundational traits generic-numeric crates expect:
//! [`Zero`], [`One`], [`Bounded`], [`Num`], [`Signed`],
//! [`FromPrimitive`], [`ToPrimitive`]. All conversions default to
//! [`RoundingMode::NearestEven`] when a rounding decision is needed
//! and drop the per-operation [`Status`](ferrodec_ieee::Status).
//!
//! The integer `to_*` paths delegate to the exact decimal
//! conversions in [`crate::convert`] (scaling the coefficient in
//! `u128`, no `f64` intermediate), so every Decimal64-representable
//! integer converts without precision loss. They return `None` for
//! NaN / Infinity / out-of-range (anything the spec flags `INVALID`);
//! an in-range value that merely rounds returns `Some`.

use num_traits::{Bounded, FromPrimitive, Num, One, Signed, ToPrimitive, Zero};

use crate::convert::ParseDecimalError;
use crate::decimal::Decimal64;
use ferrodec_ieee::RoundingMode;

const RM: RoundingMode = RoundingMode::NearestEven;

impl Zero for Decimal64 {
    fn zero() -> Self {
        Self::ZERO
    }
    fn is_zero(&self) -> bool {
        Decimal64::is_zero(*self)
    }
}

impl One for Decimal64 {
    fn one() -> Self {
        Self::ONE
    }
    fn is_one(&self) -> bool {
        // Bitwise equality on the canonical ONE cohort. Other cohorts
        // representing the value 1 (e.g. 10E-1) return false; matches
        // num_traits's expectation that `is_one` is "the natural one".
        self.to_bits() == Self::ONE.to_bits()
    }
}

impl Bounded for Decimal64 {
    fn min_value() -> Self {
        Self::MIN
    }
    fn max_value() -> Self {
        Self::MAX
    }
}

impl Num for Decimal64 {
    type FromStrRadixErr = FromStrRadixError;

    fn from_str_radix(s: &str, radix: u32) -> Result<Self, Self::FromStrRadixErr> {
        if radix != 10 {
            return Err(FromStrRadixError::UnsupportedRadix(radix));
        }
        Self::parse_str(s, RM)
            .map(|(d, _)| d)
            .map_err(FromStrRadixError::Parse)
    }
}

/// Error returned by `<Decimal64 as num_traits::Num>::from_str_radix`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FromStrRadixError {
    /// `radix` was not `10`.
    UnsupportedRadix(u32),
    /// The string did not parse as a Decimal64 literal.
    Parse(ParseDecimalError),
}

impl core::fmt::Display for FromStrRadixError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnsupportedRadix(r) => {
                write!(f, "unsupported radix {r} (Decimal64 is base 10)")
            }
            Self::Parse(e) => write!(f, "{e}"),
        }
    }
}

impl core::error::Error for FromStrRadixError {}

impl Signed for Decimal64 {
    fn abs(&self) -> Self {
        Decimal64::abs(*self)
    }
    fn abs_sub(&self, other: &Self) -> Self {
        let (diff, _) = self.sub(*other, RM);
        if diff.is_sign_negative() {
            Self::ZERO
        } else {
            diff
        }
    }
    fn signum(&self) -> Self {
        if self.is_nan() {
            return Self::NAN;
        }
        if self.is_zero() {
            return *self; // preserve sign
        }
        if self.is_sign_negative() {
            Self::NEG_ONE
        } else {
            Self::ONE
        }
    }
    fn is_positive(&self) -> bool {
        !self.is_zero() && !self.is_sign_negative() && !self.is_nan()
    }
    fn is_negative(&self) -> bool {
        !self.is_zero() && self.is_sign_negative() && !self.is_nan()
    }
}

impl FromPrimitive for Decimal64 {
    fn from_i64(n: i64) -> Option<Self> {
        // Decimal64 holds 16 digits, so any i64 with |n| < 10¹⁶ fits
        // exactly via try_new. Larger magnitudes go through the f64
        // round-trip for Decimal64-appropriate rounding.
        if let Ok(d) = Self::try_new(n, 0) {
            return Some(d);
        }
        #[allow(clippy::cast_precision_loss)]
        let (d, _) = Self::from_f64(n as f64, RM);
        Some(d)
    }
    fn from_u64(n: u64) -> Option<Self> {
        // u64 values that round-trip through i64 < 10¹⁶ fit exactly;
        // larger ones round via f64.
        if let Ok(signed) = i64::try_from(n) {
            if let Ok(d) = Self::try_new(signed, 0) {
                return Some(d);
            }
        }
        #[allow(clippy::cast_precision_loss)]
        let (d, _) = Self::from_f64(n as f64, RM);
        Some(d)
    }
    fn from_f64(n: f64) -> Option<Self> {
        if n.is_nan() {
            return None;
        }
        let (d, _) = Self::from_f64(n, RM);
        Some(d)
    }
    fn from_f32(n: f32) -> Option<Self> {
        if n.is_nan() {
            return None;
        }
        let (d, _) = Self::from_f64(f64::from(n), RM);
        Some(d)
    }
}

impl ToPrimitive for Decimal64 {
    fn to_i64(&self) -> Option<i64> {
        // Exact decimal path (M5). `INVALID` covers NaN, Infinity,
        // and out-of-range; INEXACT (a mere rounding) still yields a
        // value.
        let (n, s) = Decimal64::to_i64(*self, RM);
        (!s.invalid()).then_some(n)
    }
    fn to_u64(&self) -> Option<u64> {
        let (n, s) = Decimal64::to_u64(*self, RM);
        (!s.invalid()).then_some(n)
    }
    fn to_i128(&self) -> Option<i128> {
        let (n, s) = Decimal64::to_i128(*self, RM);
        (!s.invalid()).then_some(n)
    }
    fn to_u128(&self) -> Option<u128> {
        let (n, s) = Decimal64::to_u128(*self, RM);
        (!s.invalid()).then_some(n)
    }
    fn to_f64(&self) -> Option<f64> {
        Some(Decimal64::to_f64(*self, RoundingMode::NearestEven).0)
    }
    fn to_f32(&self) -> Option<f32> {
        // Direct decimal → f32 (M4): the old `to_f64(..) as f32`
        // double-rounded across f32 half-ULP boundaries.
        Some(Decimal64::to_f32(*self, RoundingMode::NearestEven).0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_one_bounded() {
        assert!(<Decimal64 as Zero>::zero().is_zero());
        assert!(<Decimal64 as One>::one().is_one());
        assert_eq!(
            <Decimal64 as Bounded>::min_value().to_bits(),
            Decimal64::MIN.to_bits()
        );
        assert_eq!(
            <Decimal64 as Bounded>::max_value().to_bits(),
            Decimal64::MAX.to_bits()
        );
    }

    #[test]
    fn num_from_str_radix() {
        let d = <Decimal64 as Num>::from_str_radix("1.23", 10).unwrap();
        let want = Decimal64::try_new(123, -2).unwrap();
        assert_eq!(d.to_bits(), want.to_bits());

        let err = <Decimal64 as Num>::from_str_radix("1010", 2).unwrap_err();
        assert!(matches!(err, FromStrRadixError::UnsupportedRadix(2)));
    }

    #[test]
    fn signed_predicates() {
        assert!(<Decimal64 as Signed>::is_positive(&Decimal64::ONE));
        assert!(<Decimal64 as Signed>::is_negative(&Decimal64::NEG_ONE));
        assert!(!<Decimal64 as Signed>::is_positive(&Decimal64::ZERO));
        assert!(!<Decimal64 as Signed>::is_negative(&Decimal64::ZERO));
        assert!(!<Decimal64 as Signed>::is_positive(&Decimal64::NAN));
    }

    #[test]
    fn signum_basic() {
        assert_eq!(
            <Decimal64 as Signed>::signum(&Decimal64::ONE).to_bits(),
            Decimal64::ONE.to_bits()
        );
        assert_eq!(
            <Decimal64 as Signed>::signum(&Decimal64::NEG_ONE).to_bits(),
            Decimal64::NEG_ONE.to_bits()
        );
        let r = <Decimal64 as Signed>::signum(&Decimal64::NAN);
        assert!(r.is_nan());
    }

    #[test]
    fn abs_sub_positive_difference() {
        let three = Decimal64::try_new(3, 0).unwrap();
        let five = Decimal64::try_new(5, 0).unwrap();
        // 5 - 3 = 2; 3 - 5 should clamp to 0.
        let r = <Decimal64 as Signed>::abs_sub(&five, &three);
        assert_eq!(r.to_bits(), Decimal64::try_new(2, 0).unwrap().to_bits());
        let r = <Decimal64 as Signed>::abs_sub(&three, &five);
        assert!(r.is_zero());
    }

    #[test]
    fn from_primitive_basics() {
        assert_eq!(
            <Decimal64 as FromPrimitive>::from_i64(42)
                .unwrap()
                .to_f64(RoundingMode::NearestEven)
                .0,
            42.0
        );
        assert_eq!(
            <Decimal64 as FromPrimitive>::from_u64(42)
                .unwrap()
                .to_f64(RoundingMode::NearestEven)
                .0,
            42.0
        );
        assert_eq!(
            <Decimal64 as FromPrimitive>::from_f64(2.5)
                .unwrap()
                .to_f64(RoundingMode::NearestEven)
                .0,
            2.5
        );
        assert!(<Decimal64 as FromPrimitive>::from_f64(f64::NAN).is_none());
    }

    #[test]
    fn to_primitive_basics() {
        let d = Decimal64::try_new(42, 0).unwrap();
        assert_eq!(<Decimal64 as ToPrimitive>::to_i64(&d), Some(42));
        assert_eq!(<Decimal64 as ToPrimitive>::to_u64(&d), Some(42));

        let neg = Decimal64::try_new(-3, 0).unwrap();
        assert_eq!(<Decimal64 as ToPrimitive>::to_i64(&neg), Some(-3));
        assert_eq!(<Decimal64 as ToPrimitive>::to_u64(&neg), None);

        assert_eq!(<Decimal64 as ToPrimitive>::to_f64(&d), Some(42.0));
        assert_eq!(<Decimal64 as ToPrimitive>::to_i64(&Decimal64::NAN), None);
        assert_eq!(
            <Decimal64 as ToPrimitive>::to_i64(&Decimal64::INFINITY),
            None
        );
    }
}
