//! `core::ops` operator overloads for [`Decimal128`].
//!
//! Gated on the `ops` feature so the embedded floor still pays
//! nothing by default. When enabled, each operator routes through the
//! corresponding explicit method ([`Decimal128::add`],
//! [`Decimal128::sub`], [`Decimal128::mul`], [`Decimal128::div`],
//! [`Decimal128::rem_near`]) at [`RoundingMode::NearestEven`] and
//! discards the per-call [`Status`](crate::status::Status). The `%`
//! operator routes to the IEEE 754-2019 §5.3.1 nearest-even remainder
//! on this format; the siblings route `%` to GDA truncated
//! ([`ferrodec_decimal64::Decimal64::rem_trunc`] and the decimal32
//! analogue). The per-format choice is documented under ADR-0027.
//!
//! Users who need explicit rounding-mode or status control should keep
//! using the explicit methods. The README's "Why no `core::ops`"
//! section discusses the trade-off in full.

use core::ops::{
    Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Rem, RemAssign, Sub, SubAssign,
};

use crate::decimal::Decimal128;
use crate::status::RoundingMode;

const RM: RoundingMode = RoundingMode::NearestEven;

impl Add for Decimal128 {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self {
        self.add(rhs, RM).0
    }
}

impl Sub for Decimal128 {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self {
        self.sub(rhs, RM).0
    }
}

impl Mul for Decimal128 {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: Self) -> Self {
        self.mul(rhs, RM).0
    }
}

impl Div for Decimal128 {
    type Output = Self;
    #[inline]
    fn div(self, rhs: Self) -> Self {
        self.div(rhs, RM).0
    }
}

impl Rem for Decimal128 {
    type Output = Self;
    /// IEEE 754-2019 §5.3.1 nearest-even remainder. Exact, no rounding
    /// mode. Routes to [`Decimal128::rem_near`]; the sibling formats
    /// route `%` to their GDA truncated `rem_trunc` instead (ADR-0027
    /// records the per-format choice).
    #[inline]
    fn rem(self, rhs: Self) -> Self {
        Decimal128::rem_near(self, rhs).0
    }
}

impl Neg for Decimal128 {
    type Output = Self;
    /// Bitwise sign flip (matches [`Decimal128::neg`], the no-status
    /// variant). Operator users typically want this lighter form;
    /// callers that need IEEE 754 §5.5.1 sNaN handling should call
    /// [`Decimal128::neg_with_status`] explicitly.
    #[inline]
    fn neg(self) -> Self {
        Decimal128::neg(self)
    }
}

impl AddAssign for Decimal128 {
    #[inline]
    fn add_assign(&mut self, rhs: Self) {
        *self = self.add(rhs, RM).0;
    }
}

impl SubAssign for Decimal128 {
    #[inline]
    fn sub_assign(&mut self, rhs: Self) {
        *self = self.sub(rhs, RM).0;
    }
}

impl MulAssign for Decimal128 {
    #[inline]
    fn mul_assign(&mut self, rhs: Self) {
        *self = self.mul(rhs, RM).0;
    }
}

impl DivAssign for Decimal128 {
    #[inline]
    fn div_assign(&mut self, rhs: Self) {
        *self = self.div(rhs, RM).0;
    }
}

impl RemAssign for Decimal128 {
    #[inline]
    fn rem_assign(&mut self, rhs: Self) {
        *self = Decimal128::rem_near(*self, rhs).0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(coef: i128, exp: i32) -> Decimal128 {
        Decimal128::try_new(coef, exp).unwrap()
    }

    #[test]
    fn add_op_matches_method() {
        let a = d(123, -2);
        let b = d(77, -2);
        let s = a + b;
        let want = a.add(b, RM).0;
        assert_eq!(s.to_bits(), want.to_bits());
    }

    #[test]
    fn sub_mul_div_match_methods() {
        let a = d(10, 0);
        let b = d(3, 0);
        assert_eq!((a - b).to_bits(), a.sub(b, RM).0.to_bits());
        assert_eq!((a * b).to_bits(), a.mul(b, RM).0.to_bits());
        assert_eq!((a / b).to_bits(), a.div(b, RM).0.to_bits());
    }

    #[test]
    fn neg_op_flips_sign() {
        let one = Decimal128::ONE;
        let neg = -one;
        assert_eq!(neg.to_bits(), Decimal128::NEG_ONE.to_bits());
    }

    #[test]
    fn add_assign_accumulates() {
        let mut acc = Decimal128::ZERO;
        for v in [d(1, 0), d(2, 0), d(3, 0)] {
            acc += v;
        }
        assert_eq!(acc.to_bits(), d(6, 0).to_bits());
    }

    #[test]
    fn rem_op_routes_through_rem_near() {
        // `%` is the IEEE 754-2019 §5.3.1 nearest-even remainder on
        // Decimal128 (the siblings route `%` to their GDA truncated
        // `rem_trunc` instead; see ADR-0027). Pin the parent's routing
        // contract explicitly so a future refactor that reroutes `%`
        // surfaces here rather than silently changing user math.
        let a = d(10, 0);
        let b = d(3, 0);
        let r = a % b;
        let want = Decimal128::rem_near(a, b).0;
        assert_eq!(r.to_bits(), want.to_bits());

        // 7 % 2 differs between the two rules: rem_near rounds the
        // quotient half-to-even (4) and returns -1; rem_trunc would
        // truncate (3) and return 1. Pin that the operator picks
        // nearest-even on this format.
        let (got, _) = d(7, 0).rem_near(d(2, 0));
        assert_eq!((d(7, 0) % d(2, 0)).to_bits(), got.to_bits());
    }
}
