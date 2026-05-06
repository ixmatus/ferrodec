//! `num-traits` adapter for [`Decimal128`].
//!
//! Gated on the `num-traits` feature, which transitively enables
//! `ops` (since [`num_traits::Num`] requires `Add + Sub + Mul + Div +
//! Rem` from `core::ops`). Implements the foundational traits
//! generic-numeric crates expect:
//!
//! * [`Zero`] / [`One`] — identity elements for `+` / `*`.
//! * [`Bounded`] — `min_value` / `max_value` for the format.
//! * [`Num`] — composes the above with the operator traits.
//! * [`Signed`] — sign predicates and `signum` / `abs`.
//! * [`FromPrimitive`] / [`ToPrimitive`] — best-effort conversions
//!   to and from `u8` / `i64` / `f64` etc.
//!
//! All conversions default to [`RoundingMode::NearestEven`] when a
//! rounding decision is required; [`Status`](crate::status::Status)
//! is dropped. Callers who need explicit control should keep using
//! the explicit ferrodec methods.

use num_traits::{Bounded, FromPrimitive, Num, One, Signed, ToPrimitive, Zero};

use crate::convert::ParseDecimalError;
use crate::decimal::Decimal128;
use crate::status::RoundingMode;

const RM: RoundingMode = RoundingMode::NearestEven;

impl Zero for Decimal128 {
    fn zero() -> Self {
        Self::ZERO
    }
    fn is_zero(&self) -> bool {
        Decimal128::is_zero(*self)
    }
}

impl One for Decimal128 {
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

impl Bounded for Decimal128 {
    fn min_value() -> Self {
        Self::MIN
    }
    fn max_value() -> Self {
        Self::MAX
    }
}

impl Num for Decimal128 {
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

/// Error returned by `<Decimal128 as num_traits::Num>::from_str_radix`.
///
/// Decimal128 is a base-10 format; parsing in any other radix is
/// unsupported. For radix 10, decimal-literal parse errors fall
/// through from the underlying [`ParseDecimalError`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FromStrRadixError {
    /// `radix` was not `10`.
    UnsupportedRadix(u32),
    /// The string did not parse as a Decimal128 literal.
    Parse(ParseDecimalError),
}

impl core::fmt::Display for FromStrRadixError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnsupportedRadix(r) => {
                write!(f, "unsupported radix {r} (Decimal128 is base 10)")
            }
            Self::Parse(e) => write!(f, "{e}"),
        }
    }
}

impl core::error::Error for FromStrRadixError {}

impl Signed for Decimal128 {
    fn abs(&self) -> Self {
        Decimal128::abs(*self)
    }
    fn abs_sub(&self, other: &Self) -> Self {
        // `abs_sub(x, y) = max(x - y, 0)`, the historical "positive
        // difference". Implements via the explicit ops to avoid
        // requiring partial_cmp on NaN.
        let (diff, _) = self.sub(*other, RM);
        if diff.is_sign_negative() {
            Self::ZERO
        } else {
            diff
        }
    }
    fn signum(&self) -> Self {
        Decimal128::signum(*self)
    }
    fn is_positive(&self) -> bool {
        !self.is_zero() && !self.is_sign_negative() && !self.is_nan()
    }
    fn is_negative(&self) -> bool {
        !self.is_zero() && self.is_sign_negative() && !self.is_nan()
    }
}

impl FromPrimitive for Decimal128 {
    fn from_i64(n: i64) -> Option<Self> {
        Some(Self::from_i64(n))
    }
    fn from_u64(n: u64) -> Option<Self> {
        Some(Self::from_u64(n))
    }
    fn from_i128(n: i128) -> Option<Self> {
        Some(Self::from_i128(n, RM).0)
    }
    fn from_u128(n: u128) -> Option<Self> {
        Some(Self::from_u128(n, RM).0)
    }
    // f32 / f64 conversions live behind the `binary-float` feature.
    // When that's off, the trait's default impls (returning None) are
    // the right behaviour: ferrodec doesn't know how to round a binary
    // float without the feature's machinery.
    #[cfg(feature = "binary-float")]
    fn from_f64(n: f64) -> Option<Self> {
        if n.is_nan() {
            return None;
        }
        Some(Self::from_f64(n))
    }
    #[cfg(feature = "binary-float")]
    fn from_f32(n: f32) -> Option<Self> {
        if n.is_nan() {
            return None;
        }
        Some(Self::from_f32(n))
    }
}

impl ToPrimitive for Decimal128 {
    fn to_i64(&self) -> Option<i64> {
        let (v, st) = Decimal128::to_i64(*self, RM);
        if st.invalid() {
            None
        } else {
            Some(v)
        }
    }
    fn to_u64(&self) -> Option<u64> {
        let (v, st) = Decimal128::to_u64(*self, RM);
        if st.invalid() {
            None
        } else {
            Some(v)
        }
    }
    fn to_i128(&self) -> Option<i128> {
        let (v, st) = Decimal128::to_i128(*self, RM);
        if st.invalid() {
            None
        } else {
            Some(v)
        }
    }
    fn to_u128(&self) -> Option<u128> {
        let (v, st) = Decimal128::to_u128(*self, RM);
        if st.invalid() {
            None
        } else {
            Some(v)
        }
    }
    // f32 / f64 conversions are gated on `binary-float`. Same
    // rationale as `FromPrimitive::from_f*` above.
    #[cfg(feature = "binary-float")]
    fn to_f64(&self) -> Option<f64> {
        // Always succeeds: f64 has its own NaN/Inf representations.
        let (v, _) = Decimal128::to_f64(*self, RM);
        Some(v)
    }
    #[cfg(feature = "binary-float")]
    fn to_f32(&self) -> Option<f32> {
        let (v, _) = Decimal128::to_f32(*self, RM);
        Some(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_and_one() {
        assert!(<Decimal128 as Zero>::zero().is_zero());
        assert!(<Decimal128 as One>::one().is_one());
    }

    #[test]
    fn bounded() {
        assert_eq!(
            <Decimal128 as Bounded>::min_value().to_bits(),
            Decimal128::MIN.to_bits()
        );
        assert_eq!(
            <Decimal128 as Bounded>::max_value().to_bits(),
            Decimal128::MAX.to_bits()
        );
    }

    #[test]
    fn num_from_str_radix_10() {
        let d = <Decimal128 as Num>::from_str_radix("1.23", 10).unwrap();
        let want = Decimal128::try_new(123, -2).unwrap();
        assert_eq!(d.to_bits(), want.to_bits());
    }

    #[test]
    fn num_from_str_radix_other_rejected() {
        let err = <Decimal128 as Num>::from_str_radix("1010", 2).unwrap_err();
        assert!(matches!(err, FromStrRadixError::UnsupportedRadix(2)));
    }

    #[test]
    fn signed_predicates() {
        let one = Decimal128::ONE;
        let neg_one = Decimal128::NEG_ONE;
        assert!(Signed::is_positive(&one));
        assert!(!Signed::is_positive(&neg_one));
        assert!(Signed::is_negative(&neg_one));
        assert!(!Signed::is_negative(&one));
        assert!(!Signed::is_positive(&Decimal128::ZERO));
        assert!(!Signed::is_negative(&Decimal128::ZERO));
        assert!(!Signed::is_positive(&Decimal128::NAN));
    }

    #[test]
    fn from_primitive_round_trip_i64() {
        let d = <Decimal128 as FromPrimitive>::from_i64(-12345).unwrap();
        let back = <Decimal128 as ToPrimitive>::to_i64(&d).unwrap();
        assert_eq!(back, -12345);
    }

    #[test]
    fn to_primitive_nan_to_i64_is_none() {
        assert_eq!(<Decimal128 as ToPrimitive>::to_i64(&Decimal128::NAN), None);
    }

    #[test]
    fn num_addsub_via_operator_traits() {
        // Smoke: Num requires the operator traits, so this composes
        // cleanly under the `num-traits` feature.
        fn double<T: Num + Copy>(x: T) -> T {
            x + x
        }
        let two = double(Decimal128::ONE);
        assert_eq!(two.to_bits(), Decimal128::try_new(2, 0).unwrap().to_bits());
    }
}
