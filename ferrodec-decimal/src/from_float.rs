//! Lossless `TryFrom<f64>` / `TryFrom<f32>` (the `binary-float` feature).
//!
//! The conversion is exact. A finite binary float is a dyadic rational, which
//! is always a finite decimal, so the result is the float's precise value with
//! no rounding and no context. This deliberately differs from the fixed-width
//! ferrodec formats, which must round an `f64` to their width: an
//! arbitrary-precision value need not, so it does not, and `0.1f64` converts to
//! the exact `0.1000000000000000055511151231257827021181583404541015625`, not
//! the shortest decimal `0.1`. NaN and the infinities are not finite decimals
//! and are rejected. See ADR-0041.

use crate::Decimal;
use ferrodec_multiword::DecBig;

/// The error from [`TryFrom<f64>`] / [`TryFrom<f32>`] for [`Decimal`] when the
/// float is not a finite number.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DecimalFromFloatError {
    /// The input was NaN.
    NotANumber,
    /// The input was an infinity.
    Infinite,
}

impl core::fmt::Display for DecimalFromFloatError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NotANumber => f.write_str("cannot convert NaN to Decimal"),
            Self::Infinite => f.write_str("cannot convert an infinity to Decimal"),
        }
    }
}

impl core::error::Error for DecimalFromFloatError {}

impl TryFrom<f64> for Decimal {
    type Error = DecimalFromFloatError;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        if value.is_nan() {
            return Err(DecimalFromFloatError::NotANumber);
        }
        if value.is_infinite() {
            return Err(DecimalFromFloatError::Infinite);
        }
        Ok(f64_to_decimal_exact(value))
    }
}

impl TryFrom<f32> for Decimal {
    type Error = DecimalFromFloatError;

    fn try_from(value: f32) -> Result<Self, Self::Error> {
        // Every `f32` is exactly representable as an `f64`, so widening first
        // loses nothing and reuses the one exact path.
        Decimal::try_from(f64::from(value))
    }
}

/// The exact decimal value of a finite `f64`. The float decomposes into
/// `significand * 2^e2`; for `e2 >= 0` that integer is the coefficient at
/// exponent zero, and for `e2 < 0` the identity `2^-k = 5^k * 10^-k` makes the
/// coefficient `significand * 5^k` at exponent `-k`.
fn f64_to_decimal_exact(value: f64) -> Decimal {
    let bits = value.to_bits();
    let sign = (bits >> 63) == 1;
    let raw_exp = ((bits >> 52) & 0x7ff) as i32;
    let frac = bits & 0x000f_ffff_ffff_ffff;
    if raw_exp == 0 && frac == 0 {
        return Decimal::finite(sign, DecBig::zero(), 0);
    }
    // Restore the implicit leading bit for a normal value; a subnormal has none.
    // The unbiased power of two is `raw_exp - 1023 - 52` (normal) or `-1074`
    // (subnormal).
    let (significand, e2) = if raw_exp == 0 {
        (frac, -1074)
    } else {
        (frac | (1 << 52), raw_exp - 1075)
    };
    if e2 >= 0 {
        // An integer: the coefficient is `significand * 2^e2` at exponent zero.
        let coeff = DecBig::from_u64(significand).mul(&pow(2, e2 as u32));
        return Decimal::finite(sign, coeff, 0);
    }
    // Below the binary point: `significand / 2^k`. Cancel the powers of two the
    // significand and the denominator share (this is what makes 1.0 convert to
    // the cohort `1`, not `1.000...0`), then apply `2^-k = 5^k * 10^-k` to the
    // reduced denominator.
    let k = e2.unsigned_abs();
    let shared = significand.trailing_zeros().min(k);
    let reduced_sig = significand >> shared;
    let reduced_k = k - shared;
    let coeff = DecBig::from_u64(reduced_sig).mul(&pow(5, reduced_k));
    Decimal::finite(sign, coeff, -(reduced_k as i32))
}

/// `base^exp` as a [`DecBig`], by binary exponentiation.
fn pow(base: u32, exp: u32) -> DecBig {
    let mut acc = DecBig::from_u32(1);
    let mut b = DecBig::from_u32(base);
    let mut e = exp;
    while e > 0 {
        if e & 1 == 1 {
            acc = acc.mul(&b);
        }
        e >>= 1;
        if e > 0 {
            b = b.mul(&b);
        }
    }
    acc
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Decimal {
        Decimal::parse_str(s).unwrap()
    }

    #[test]
    fn dyadic_values_are_exact() {
        assert_eq!(Decimal::try_from(1.0_f64).unwrap(), parse("1"));
        assert_eq!(Decimal::try_from(-2.5_f64).unwrap(), parse("-2.5"));
        assert_eq!(Decimal::try_from(0.25_f64).unwrap(), parse("0.25"));
        // Signed zero is preserved.
        assert_eq!(Decimal::try_from(0.0_f64).unwrap(), parse("0"));
        assert!(Decimal::try_from(-0.0_f64).unwrap().is_negative());
        assert!(Decimal::try_from(-0.0_f64).unwrap().is_zero());
    }

    #[test]
    fn conversion_is_lossless_not_shortest() {
        // 0.1 is not dyadic, so the exact value is the long form (the same value
        // CPython's decimal.Decimal(0.1) yields), not the shortest decimal 0.1.
        let d = Decimal::try_from(0.1_f64).unwrap();
        assert_eq!(
            d,
            parse("0.1000000000000000055511151231257827021181583404541015625")
        );
        assert_ne!(d, parse("0.1"));
    }

    #[test]
    fn large_and_subnormal_round_trip_exactly() {
        // A large power of two is an exact integer coefficient at exponent zero.
        assert_eq!(
            Decimal::try_from(2.0_f64.powi(60)).unwrap(),
            parse("1152921504606846976")
        );
        // The least positive subnormal f64 is 2^-1074, an exact 5^1074 decimal.
        let tiny = Decimal::try_from(f64::from_bits(1)).unwrap();
        assert_eq!(tiny.digits(), Some(751));
    }

    #[test]
    fn rejects_non_finite() {
        assert_eq!(
            Decimal::try_from(f64::NAN),
            Err(DecimalFromFloatError::NotANumber)
        );
        assert_eq!(
            Decimal::try_from(f64::INFINITY),
            Err(DecimalFromFloatError::Infinite)
        );
        assert_eq!(
            Decimal::try_from(f32::NEG_INFINITY),
            Err(DecimalFromFloatError::Infinite)
        );
        assert_eq!(
            Decimal::try_from(f32::NAN),
            Err(DecimalFromFloatError::NotANumber)
        );
    }

    #[test]
    fn f32_routes_exactly_through_f64() {
        assert_eq!(Decimal::try_from(0.5_f32).unwrap(), parse("0.5"));
        assert_eq!(Decimal::try_from(2.0_f32).unwrap(), parse("2"));
    }
}
