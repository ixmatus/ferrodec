//! Test-only mock of the [`DecimalFormat`] seam.
//!
//! The escalation predicates consult only `PRECISION` and `BIAS`, so a
//! unit struct with every value-carrying member `unreachable!` lets the
//! boundary tests cover arbitrary format shapes without a real format
//! crate (which would be a cyclic dependency). Const generics let one
//! definition cover the three real shapes plus synthetic ones.

use crate::extended::Extended;
use crate::format::DecimalFormat;
use core::cmp::Ordering;
use ferrodec_ieee::{IeeeDecodedClass as Class, RoundingMode, Status};
use ferrodec_multiword::U256;

#[derive(Clone, Copy, Debug)]
pub(crate) struct MockFmt<const P: u32, const B: i32>;

/// Functional value-carrying mock at the `Decimal128` shape (p = 34,
/// bias = 6176). The argument-reduction pipeline touches exactly
/// `classify`, `to_extended_parts`, and the two width constants, so
/// implementing those three faithfully lets the in-crate rung 1 vs
/// rung 2 reduction differential run on real values without a cyclic
/// dependency on a format crate. Everything else stays unreachable.
///
/// No canonicalization: the test constructs only canonical
/// coefficients (`< 10^34`), and `exp` is the unbiased quantum
/// exponent.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ValueFmt128 {
    pub(crate) coef: u128,
    pub(crate) exp: i32,
    pub(crate) sign: bool,
}

impl DecimalFormat for ValueFmt128 {
    const BIAS: i32 = 6176;
    const PRECISION: u32 = 34;
    const ZERO: Self = Self {
        coef: 0,
        exp: 0,
        sign: false,
    };
    const NEG_ZERO: Self = Self {
        coef: 0,
        exp: 0,
        sign: true,
    };
    const ONE: Self = Self {
        coef: 1,
        exp: 0,
        sign: false,
    };
    const NEG_ONE: Self = Self {
        coef: 1,
        exp: 0,
        sign: true,
    };
    const TEN: Self = Self {
        coef: 10,
        exp: 0,
        sign: false,
    };
    const INFINITY: Self = Self {
        coef: 0,
        exp: 0,
        sign: false,
    };
    const NEG_INFINITY: Self = Self {
        coef: 0,
        exp: 0,
        sign: true,
    };
    const NAN: Self = Self {
        coef: 0,
        exp: 0,
        sign: false,
    };
    const SIGNALING_NAN: Self = Self {
        coef: 0,
        exp: 0,
        sign: false,
    };
    fn classify(self) -> Class {
        if self.coef == 0 {
            Class::Zero {
                sign: self.sign,
                biased_exp: (self.exp + Self::BIAS) as u32,
            }
        } else {
            Class::Finite {
                sign: self.sign,
                biased_exp: (self.exp + Self::BIAS) as u32,
                coefficient: self.coef,
            }
        }
    }
    fn is_nan(self) -> bool {
        unreachable!()
    }
    fn is_zero(self) -> bool {
        self.coef == 0
    }
    fn is_infinite(self) -> bool {
        unreachable!()
    }
    fn is_sign_negative(self) -> bool {
        self.sign
    }
    fn is_signaling_nan(self) -> bool {
        unreachable!()
    }
    fn abs(self) -> Self {
        Self {
            sign: false,
            ..self
        }
    }
    fn neg(self) -> Self {
        Self {
            sign: !self.sign,
            ..self
        }
    }
    fn partial_cmp_fmt(self, _other: Self) -> (Option<Ordering>, Status) {
        unreachable!()
    }
    fn nan_from(self) -> Self {
        unreachable!()
    }
    fn propagate_nan2(self, _other: Self) -> Self {
        unreachable!()
    }
    fn to_extended_parts(self) -> Option<(U256, i32, bool)> {
        Some((U256::from_u128(self.coef), self.exp, self.sign))
    }
    fn round_and_pack_finite(
        _coef: U256,
        _unbiased_exp: i32,
        _q_preferred: i32,
        _sign: bool,
        _pre_sticky: bool,
        _rm: RoundingMode,
        _status: Status,
    ) -> (Self, Status) {
        unreachable!()
    }
    fn recip_seed(self, _rm: RoundingMode) -> (Self, Status) {
        unreachable!()
    }
    fn sqrt_seed(self, _rm: RoundingMode) -> (Self, Status) {
        unreachable!()
    }
    fn div_fmt(self, _other: Self, _rm: RoundingMode) -> (Self, Status) {
        unreachable!()
    }
    fn mul_fmt(self, _other: Self, _rm: RoundingMode) -> (Self, Status) {
        unreachable!()
    }
    fn to_i32_fmt(self, _rm: RoundingMode) -> (i32, Status) {
        unreachable!()
    }
    fn exp_overflow_limit() -> Extended {
        unreachable!()
    }
    fn exp_underflow_limit() -> Extended {
        unreachable!()
    }
}

impl<const P: u32, const B: i32> DecimalFormat for MockFmt<P, B> {
    const BIAS: i32 = B;
    const PRECISION: u32 = P;
    const ZERO: Self = Self;
    const NEG_ZERO: Self = Self;
    const ONE: Self = Self;
    const NEG_ONE: Self = Self;
    const TEN: Self = Self;
    const INFINITY: Self = Self;
    const NEG_INFINITY: Self = Self;
    const NAN: Self = Self;
    const SIGNALING_NAN: Self = Self;
    fn classify(self) -> Class {
        unreachable!()
    }
    fn is_nan(self) -> bool {
        unreachable!()
    }
    fn is_zero(self) -> bool {
        unreachable!()
    }
    fn is_infinite(self) -> bool {
        unreachable!()
    }
    fn is_sign_negative(self) -> bool {
        unreachable!()
    }
    fn is_signaling_nan(self) -> bool {
        unreachable!()
    }
    fn abs(self) -> Self {
        unreachable!()
    }
    fn neg(self) -> Self {
        unreachable!()
    }
    fn partial_cmp_fmt(self, _other: Self) -> (Option<Ordering>, Status) {
        unreachable!()
    }
    fn nan_from(self) -> Self {
        unreachable!()
    }
    fn propagate_nan2(self, _other: Self) -> Self {
        unreachable!()
    }
    fn to_extended_parts(self) -> Option<(U256, i32, bool)> {
        unreachable!()
    }
    fn round_and_pack_finite(
        _coef: U256,
        _unbiased_exp: i32,
        _q_preferred: i32,
        _sign: bool,
        _pre_sticky: bool,
        _rm: RoundingMode,
        _status: Status,
    ) -> (Self, Status) {
        unreachable!()
    }
    fn recip_seed(self, _rm: RoundingMode) -> (Self, Status) {
        unreachable!()
    }
    fn sqrt_seed(self, _rm: RoundingMode) -> (Self, Status) {
        unreachable!()
    }
    fn div_fmt(self, _other: Self, _rm: RoundingMode) -> (Self, Status) {
        unreachable!()
    }
    fn mul_fmt(self, _other: Self, _rm: RoundingMode) -> (Self, Status) {
        unreachable!()
    }
    fn to_i32_fmt(self, _rm: RoundingMode) -> (i32, Status) {
        unreachable!()
    }
    fn exp_overflow_limit() -> Extended {
        unreachable!()
    }
    fn exp_underflow_limit() -> Extended {
        unreachable!()
    }
}
