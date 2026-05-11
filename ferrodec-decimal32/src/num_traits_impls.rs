//! `num-traits` adapter for [`Decimal32`] (gated on the `num-traits`
//! feature, which transitively enables `ops` + `binary-float`).
//!
//! Implements the foundational traits generic-numeric crates expect:
//! [`Zero`], [`One`], [`Bounded`], [`Num`], [`Signed`],
//! [`FromPrimitive`], [`ToPrimitive`]. All conversions default to
//! [`RoundingMode::NearestEven`] when a rounding decision is needed
//! and drop the per-operation [`Status`](ferrodec_ieee::Status).
//!
//! Decimal32's 7-digit precision is narrower than `i64` / `u64`, so
//! the integer-conversion paths through `to_*` may lose precision.
//! `to_i64` / `to_u64` route via `f64` (already cheap and exact for
//! Decimal32-representable integers); the `to_*` paths return `None`
//! for NaN / Infinity / out-of-range.

use num_traits::{Bounded, FromPrimitive, Num, One, Signed, ToPrimitive, Zero};

use crate::convert::ParseDecimalError;
use crate::decimal::Decimal32;
use ferrodec_ieee::RoundingMode;

const RM: RoundingMode = RoundingMode::NearestEven;

impl Zero for Decimal32 {
    fn zero() -> Self {
        Self::ZERO
    }
    fn is_zero(&self) -> bool {
        Decimal32::is_zero(*self)
    }
}

impl One for Decimal32 {
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

impl Bounded for Decimal32 {
    fn min_value() -> Self {
        Self::MIN
    }
    fn max_value() -> Self {
        Self::MAX
    }
}

impl Num for Decimal32 {
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

/// Error returned by `<Decimal32 as num_traits::Num>::from_str_radix`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FromStrRadixError {
    /// `radix` was not `10`.
    UnsupportedRadix(u32),
    /// The string did not parse as a Decimal32 literal.
    Parse(ParseDecimalError),
}

impl core::fmt::Display for FromStrRadixError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnsupportedRadix(r) => {
                write!(f, "unsupported radix {r} (Decimal32 is base 10)")
            }
            Self::Parse(e) => write!(f, "{e}"),
        }
    }
}

impl core::error::Error for FromStrRadixError {}

impl Signed for Decimal32 {
    fn abs(&self) -> Self {
        Decimal32::abs(*self)
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

impl FromPrimitive for Decimal32 {
    fn from_i64(n: i64) -> Option<Self> {
        // Decimal32 holds 7 digits. Values with |n| < 10⁷ fit exactly
        // via try_new; larger magnitudes round via the f64 round-trip
        // (lossless for |n| < 2⁵³, ≤ 1 ULP at the f64 scale beyond
        // that — well below Decimal32's 7-digit envelope).
        if let Ok(coef) = i32::try_from(n) {
            if let Ok(d) = Self::try_new(coef, 0) {
                return Some(d);
            }
        }
        #[allow(clippy::cast_precision_loss)]
        let (d, _) = Self::from_f64(n as f64, RM);
        Some(d)
    }
    fn from_u64(n: u64) -> Option<Self> {
        // Same shape as from_i64; exact via try_new_unsigned for
        // n < 10⁷, f64 round-trip otherwise.
        if let Ok(coef) = u32::try_from(n) {
            if let Ok(d) = Self::try_new_unsigned(coef, 0) {
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

impl ToPrimitive for Decimal32 {
    fn to_i64(&self) -> Option<i64> {
        if self.is_nan() || self.is_infinite() {
            return None;
        }
        let f = Decimal32::to_f64(*self);
        #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
        let rounded = libm_round(f);
        if !(i64::MIN as f64..=i64::MAX as f64).contains(&rounded) {
            return None;
        }
        #[allow(clippy::cast_possible_truncation)]
        Some(rounded as i64)
    }
    fn to_u64(&self) -> Option<u64> {
        if self.is_nan() || self.is_infinite() {
            return None;
        }
        let f = Decimal32::to_f64(*self);
        let rounded = libm_round(f);
        if rounded < 0.0 {
            return None;
        }
        #[allow(clippy::cast_precision_loss)]
        let max = u64::MAX as f64;
        if rounded > max {
            return None;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        Some(rounded as u64)
    }
    fn to_f64(&self) -> Option<f64> {
        Some(Decimal32::to_f64(*self))
    }
    #[allow(clippy::cast_possible_truncation)]
    fn to_f32(&self) -> Option<f32> {
        Some(Decimal32::to_f64(*self) as f32)
    }
}

/// Round `f` to the nearest integer (toward even on ties), without
/// requiring `std::f64::round`. Uses `libm` when available; otherwise
/// a pure-Rust implementation via `floor` + half compare.
fn libm_round(f: f64) -> f64 {
    if !f.is_finite() {
        return f;
    }
    // libm doesn't expose `roundeven` everywhere; use floor + 0.5
    // adjustment with banker's-rounding tie-break.
    let floor = libm::floor(f);
    let frac = f - floor;
    if frac > 0.5 {
        floor + 1.0
    } else if frac < 0.5 {
        floor
    } else {
        // Halfway: round to even.
        #[allow(clippy::float_cmp)]
        if libm::floor(floor / 2.0) * 2.0 == floor {
            floor
        } else {
            floor + 1.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_one_bounded() {
        assert!(<Decimal32 as Zero>::zero().is_zero());
        assert!(<Decimal32 as One>::one().is_one());
        assert_eq!(
            <Decimal32 as Bounded>::min_value().to_bits(),
            Decimal32::MIN.to_bits()
        );
        assert_eq!(
            <Decimal32 as Bounded>::max_value().to_bits(),
            Decimal32::MAX.to_bits()
        );
    }

    #[test]
    fn num_from_str_radix() {
        let d = <Decimal32 as Num>::from_str_radix("1.23", 10).unwrap();
        let want = Decimal32::try_new(123, -2).unwrap();
        assert_eq!(d.to_bits(), want.to_bits());

        let err = <Decimal32 as Num>::from_str_radix("1010", 2).unwrap_err();
        assert!(matches!(err, FromStrRadixError::UnsupportedRadix(2)));
    }

    #[test]
    fn signed_predicates() {
        assert!(<Decimal32 as Signed>::is_positive(&Decimal32::ONE));
        assert!(<Decimal32 as Signed>::is_negative(&Decimal32::NEG_ONE));
        assert!(!<Decimal32 as Signed>::is_positive(&Decimal32::ZERO));
        assert!(!<Decimal32 as Signed>::is_negative(&Decimal32::ZERO));
        assert!(!<Decimal32 as Signed>::is_positive(&Decimal32::NAN));
    }

    #[test]
    fn signum_basic() {
        assert_eq!(
            <Decimal32 as Signed>::signum(&Decimal32::ONE).to_bits(),
            Decimal32::ONE.to_bits()
        );
        assert_eq!(
            <Decimal32 as Signed>::signum(&Decimal32::NEG_ONE).to_bits(),
            Decimal32::NEG_ONE.to_bits()
        );
        let r = <Decimal32 as Signed>::signum(&Decimal32::NAN);
        assert!(r.is_nan());
    }

    #[test]
    fn abs_sub_positive_difference() {
        let three = Decimal32::try_new(3, 0).unwrap();
        let five = Decimal32::try_new(5, 0).unwrap();
        // 5 - 3 = 2; 3 - 5 should clamp to 0.
        let r = <Decimal32 as Signed>::abs_sub(&five, &three);
        assert_eq!(r.to_bits(), Decimal32::try_new(2, 0).unwrap().to_bits());
        let r = <Decimal32 as Signed>::abs_sub(&three, &five);
        assert!(r.is_zero());
    }

    #[test]
    fn from_primitive_basics() {
        assert_eq!(
            <Decimal32 as FromPrimitive>::from_i64(42).unwrap().to_f64(),
            42.0
        );
        assert_eq!(
            <Decimal32 as FromPrimitive>::from_u64(42).unwrap().to_f64(),
            42.0
        );
        assert_eq!(
            <Decimal32 as FromPrimitive>::from_f64(2.5)
                .unwrap()
                .to_f64(),
            2.5
        );
        assert!(<Decimal32 as FromPrimitive>::from_f64(f64::NAN).is_none());
    }

    #[test]
    fn from_primitive_accepts_values_above_2_pow_53() {
        // Pre-fix: `from_i64`/`from_u64` rejected any |n| > 2⁵³ on
        // the (mistaken) grounds that the f64 intermediate would be
        // lossy. Decimal32 has 7 digits — rounding is the correct
        // behaviour, not refusal.
        let big_i: i64 = (1i64 << 60) + 7;
        let r = <Decimal32 as FromPrimitive>::from_i64(big_i);
        assert!(
            r.is_some(),
            "from_i64 should round large values, not reject"
        );
        let d = r.unwrap();
        assert!(d.is_finite() && !d.is_zero());

        let big_u: u64 = 1u64 << 60;
        let r = <Decimal32 as FromPrimitive>::from_u64(big_u);
        assert!(
            r.is_some(),
            "from_u64 should round large values, not reject"
        );
        let d = r.unwrap();
        assert!(d.is_finite() && !d.is_zero());

        // Small values still take the exact try_new path.
        let small = <Decimal32 as FromPrimitive>::from_i64(-9999).unwrap();
        assert_eq!(
            small.to_bits(),
            Decimal32::try_new(-9999, 0).unwrap().to_bits()
        );
    }

    #[test]
    fn to_primitive_basics() {
        let d = Decimal32::try_new(42, 0).unwrap();
        assert_eq!(<Decimal32 as ToPrimitive>::to_i64(&d), Some(42));
        assert_eq!(<Decimal32 as ToPrimitive>::to_u64(&d), Some(42));

        let neg = Decimal32::try_new(-3, 0).unwrap();
        assert_eq!(<Decimal32 as ToPrimitive>::to_i64(&neg), Some(-3));
        assert_eq!(<Decimal32 as ToPrimitive>::to_u64(&neg), None);

        assert_eq!(<Decimal32 as ToPrimitive>::to_f64(&d), Some(42.0));
        assert_eq!(<Decimal32 as ToPrimitive>::to_i64(&Decimal32::NAN), None);
        assert_eq!(
            <Decimal32 as ToPrimitive>::to_i64(&Decimal32::INFINITY),
            None
        );
    }
}
