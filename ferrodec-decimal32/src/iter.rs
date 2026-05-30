//! Iterator-friendly impls for [`Decimal32`].
//!
//! [`core::iter::Sum`] and [`core::iter::Product`] let callers write
//! `decimals.iter().sum::<Decimal32>()` and the corresponding
//! `.product()` form. Both use [`RoundingMode::NearestEven`] and drop
//! the per-step [`Status`](crate::Status); callers who want either
//! parameter explicit should fold by hand using [`Decimal32::add`] /
//! [`Decimal32::mul`].
//!
//! No new feature gate: `Sum` and `Product` are useful enough across
//! both embedded and general audiences that they ship in the core
//! surface, matching the `ferrodec` (Decimal128) parent.

use core::iter::{Product, Sum};

use crate::decimal::Decimal32;
use ferrodec_ieee::RoundingMode;

impl Sum<Self> for Decimal32 {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Self::ZERO, |acc, x| acc.add(x, RoundingMode::NearestEven).0)
    }
}

impl<'a> Sum<&'a Self> for Decimal32 {
    fn sum<I: Iterator<Item = &'a Self>>(iter: I) -> Self {
        iter.copied().sum()
    }
}

impl Product<Self> for Decimal32 {
    fn product<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Self::ONE, |acc, x| acc.mul(x, RoundingMode::NearestEven).0)
    }
}

impl<'a> Product<&'a Self> for Decimal32 {
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
            Decimal32::ONE,
            Decimal32::try_new(2, 0).unwrap(),
            Decimal32::try_new(3, 0).unwrap(),
        ];
        let s: Decimal32 = xs.iter().sum();
        let want = Decimal32::try_new(6, 0).unwrap();
        assert_eq!(s.to_bits(), want.to_bits());
    }

    #[test]
    fn sum_empty_is_zero() {
        let s: Decimal32 = core::iter::empty::<Decimal32>().sum();
        assert!(s.is_zero());
    }

    #[test]
    fn product_three_integers() {
        let xs = [
            Decimal32::try_new(2, 0).unwrap(),
            Decimal32::try_new(3, 0).unwrap(),
            Decimal32::try_new(4, 0).unwrap(),
        ];
        let p: Decimal32 = xs.into_iter().product();
        let want = Decimal32::try_new(24, 0).unwrap();
        assert_eq!(p.to_bits(), want.to_bits());
    }

    #[test]
    fn product_empty_is_one() {
        let p: Decimal32 = core::iter::empty::<Decimal32>().product();
        assert_eq!(p.to_bits(), Decimal32::ONE.to_bits());
    }

    #[test]
    fn sum_propagates_nan() {
        let xs = [
            Decimal32::ONE,
            Decimal32::NAN,
            Decimal32::try_new(2, 0).unwrap(),
        ];
        let s: Decimal32 = xs.into_iter().sum();
        assert!(s.is_nan());
    }
}
