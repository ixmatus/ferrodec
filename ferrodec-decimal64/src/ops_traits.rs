//! `core::ops` trait implementations for [`Decimal64`] (gated on the
//! `ops` feature).
//!
//! The default rounding mode is [`RoundingMode::NearestEven`] and the
//! per-operation `Status` is dropped. Callers that need explicit
//! rounding-mode or status control should keep using the explicit
//! `add` / `sub` / `mul` / `div` / `rem_near` / `rem_trunc` methods.
//!
//! `%` routes to [`Decimal64::rem_trunc`] (the GDA truncated
//! remainder), matching the rule the bare 1.x `rem` method named on
//! this format. The parent crate routes `%` to its nearest-even
//! remainder instead; the per-format choice is documented under
//! ADR-0027. The 1.x bare `rem` spelling was retired in 2.0.
//!
//! See the README's "Why no `core::ops`" section for the design
//! rationale (mirrors ferrodec/Decimal128).

use core::ops::{
    Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Rem, RemAssign, Sub, SubAssign,
};

use crate::decimal::Decimal64;
use ferrodec_ieee::RoundingMode;

const DEFAULT_RM: RoundingMode = RoundingMode::NearestEven;

impl Add for Decimal64 {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self {
        self.add(rhs, DEFAULT_RM).0
    }
}

impl Sub for Decimal64 {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self {
        self.sub(rhs, DEFAULT_RM).0
    }
}

impl Mul for Decimal64 {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: Self) -> Self {
        self.mul(rhs, DEFAULT_RM).0
    }
}

impl Div for Decimal64 {
    type Output = Self;
    #[inline]
    fn div(self, rhs: Self) -> Self {
        self.div(rhs, DEFAULT_RM).0
    }
}

impl Rem for Decimal64 {
    type Output = Self;
    /// GDA / C99 `fmod` truncated remainder. Exact, no rounding mode.
    /// Routes to [`Decimal64::rem_trunc`]; the parent format routes
    /// `%` to its nearest-even `rem_near` instead (ADR-0027 records
    /// the per-format choice).
    #[inline]
    fn rem(self, rhs: Self) -> Self {
        self.rem_trunc(rhs).0
    }
}

impl Neg for Decimal64 {
    type Output = Self;
    #[inline]
    fn neg(self) -> Self {
        Decimal64::neg(self)
    }
}

impl AddAssign for Decimal64 {
    #[inline]
    fn add_assign(&mut self, rhs: Self) {
        *self = self.add(rhs, DEFAULT_RM).0;
    }
}

impl SubAssign for Decimal64 {
    #[inline]
    fn sub_assign(&mut self, rhs: Self) {
        *self = self.sub(rhs, DEFAULT_RM).0;
    }
}

impl MulAssign for Decimal64 {
    #[inline]
    fn mul_assign(&mut self, rhs: Self) {
        *self = self.mul(rhs, DEFAULT_RM).0;
    }
}

impl DivAssign for Decimal64 {
    #[inline]
    fn div_assign(&mut self, rhs: Self) {
        *self = self.div(rhs, DEFAULT_RM).0;
    }
}

impl RemAssign for Decimal64 {
    #[inline]
    fn rem_assign(&mut self, rhs: Self) {
        *self = self.rem_trunc(rhs).0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn from_int(n: i64, exp: i32) -> Decimal64 {
        Decimal64::try_new(n, exp).unwrap()
    }

    #[test]
    fn ops_basic_arithmetic() {
        let a = from_int(2, 0);
        let b = from_int(3, 0);
        assert_eq!((a + b).to_bits(), from_int(5, 0).to_bits());
        assert_eq!((b - a).to_bits(), from_int(1, 0).to_bits());
        assert_eq!((a * b).to_bits(), from_int(6, 0).to_bits());

        let six = from_int(6, 0);
        assert_eq!((six / a).to_bits(), from_int(3, 0).to_bits());
        assert_eq!(
            (from_int(10, 0) % from_int(3, 0)).to_bits(),
            from_int(1, 0).to_bits()
        );
    }

    #[test]
    fn neg_op() {
        assert_eq!((-from_int(5, 0)).to_bits(), from_int(-5, 0).to_bits());
    }

    #[test]
    fn assign_ops() {
        let mut x = from_int(10, 0);
        x += from_int(5, 0);
        assert_eq!(x.to_bits(), from_int(15, 0).to_bits());
        x -= from_int(3, 0);
        assert_eq!(x.to_bits(), from_int(12, 0).to_bits());
        x *= from_int(2, 0);
        assert_eq!(x.to_bits(), from_int(24, 0).to_bits());
        x /= from_int(4, 0);
        assert_eq!(x.to_bits(), from_int(6, 0).to_bits());
        x %= from_int(4, 0);
        assert_eq!(x.to_bits(), from_int(2, 0).to_bits());
    }
}
