//! Iterator-friendly impls for [`Decimal128`].
//!
//! [`core::iter::Sum`] and [`core::iter::Product`] let callers write
//! `decimals.iter().sum::<Decimal128>()` and the corresponding
//! `.product()` form. Both use [`RoundingMode::NearestEven`] and drop
//! the per-step [`Status`](crate::status::Status); callers who want
//! either parameter explicit should fold by hand using
//! [`Decimal128::add`] / [`Decimal128::mul`].
//!
//! No new feature gate — `Sum` and `Product` are useful enough across
//! both embedded and general audiences that they ship in the core
//! surface.

use core::iter::{Product, Sum};

use crate::decimal::Decimal128;
use crate::status::RoundingMode;

impl Sum<Self> for Decimal128 {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Self::ZERO, |acc, x| acc.add(x, RoundingMode::NearestEven).0)
    }
}

impl<'a> Sum<&'a Self> for Decimal128 {
    fn sum<I: Iterator<Item = &'a Self>>(iter: I) -> Self {
        iter.copied().sum()
    }
}

impl Product<Self> for Decimal128 {
    fn product<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Self::ONE, |acc, x| acc.mul(x, RoundingMode::NearestEven).0)
    }
}

impl<'a> Product<&'a Self> for Decimal128 {
    fn product<I: Iterator<Item = &'a Self>>(iter: I) -> Self {
        iter.copied().product()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sum_three_integers() {
        let xs = [
            Decimal128::ONE,
            Decimal128::try_new(2, 0).unwrap(),
            Decimal128::try_new(3, 0).unwrap(),
        ];
        let s: Decimal128 = xs.iter().sum();
        let want = Decimal128::try_new(6, 0).unwrap();
        assert_eq!(s.to_bits(), want.to_bits());
    }

    #[test]
    fn sum_empty_is_zero() {
        let s: Decimal128 = core::iter::empty::<Decimal128>().sum();
        assert!(s.is_zero());
    }

    #[test]
    fn product_three_integers() {
        let xs = [
            Decimal128::try_new(2, 0).unwrap(),
            Decimal128::try_new(3, 0).unwrap(),
            Decimal128::try_new(4, 0).unwrap(),
        ];
        let p: Decimal128 = xs.into_iter().product();
        let want = Decimal128::try_new(24, 0).unwrap();
        assert_eq!(p.to_bits(), want.to_bits());
    }

    #[test]
    fn product_empty_is_one() {
        let p: Decimal128 = core::iter::empty::<Decimal128>().product();
        assert_eq!(p.to_bits(), Decimal128::ONE.to_bits());
    }

    #[test]
    fn sum_propagates_nan() {
        let xs = [
            Decimal128::ONE,
            Decimal128::NAN,
            Decimal128::try_new(2, 0).unwrap(),
        ];
        let s: Decimal128 = xs.into_iter().sum();
        assert!(s.is_nan());
    }
}
