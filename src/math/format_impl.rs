//! `impl DecimalFormat for Decimal128` (P0a.2 c3).
//!
//! Every method is a thin forward to `Decimal128`'s already-verified
//! inherent surface. The trait carries no arithmetic of its own, so
//! the shared `ferrodec-transcend` kernel instantiated at
//! `F = Decimal128` is byte-identical to the pre-extraction
//! hand-written kernel. That property is what keeps the extraction
//! behaviour-neutral for the formally-verified Decimal128 parent.

use crate::decimal::Decimal128;
use ferrodec_ieee::{RoundingMode, Status};
use ferrodec_transcend::DecimalFormat;

impl DecimalFormat for Decimal128 {
    const BIAS: i32 = crate::bid::BIAS as i32;
    const PRECISION: u32 = crate::bid::PRECISION;

    const ZERO: Self = Decimal128::ZERO;
    const NEG_ZERO: Self = Decimal128::NEG_ZERO;
    const ONE: Self = Decimal128::ONE;
    const NEG_ONE: Self = Decimal128::NEG_ONE;
    const TEN: Self = Decimal128::TEN;
    const INFINITY: Self = Decimal128::INFINITY;
    const NEG_INFINITY: Self = Decimal128::NEG_INFINITY;
    const NAN: Self = Decimal128::NAN;
    const SIGNALING_NAN: Self = Decimal128::SIGNALING_NAN;

    fn classify(self) -> ferrodec_ieee::IeeeDecodedClass {
        crate::bid::classify_bits(self.to_bits())
    }

    fn is_nan(self) -> bool {
        Decimal128::is_nan(self)
    }

    fn is_zero(self) -> bool {
        Decimal128::is_zero(self)
    }

    fn is_infinite(self) -> bool {
        Decimal128::is_infinite(self)
    }

    fn is_sign_negative(self) -> bool {
        Decimal128::is_sign_negative(self)
    }

    fn is_signaling_nan(self) -> bool {
        Decimal128::is_signaling_nan(self)
    }

    fn abs(self) -> Self {
        Decimal128::abs(self)
    }

    fn neg(self) -> Self {
        Decimal128::neg(self)
    }

    fn partial_cmp_fmt(self, other: Self) -> (Option<core::cmp::Ordering>, Status) {
        self.partial_cmp(other)
    }

    fn nan_from(self) -> Self {
        crate::ops::nan_from(self)
    }

    fn propagate_nan2(self, other: Self) -> Self {
        crate::ops::propagate_nan2(self, other)
    }

    fn to_extended_parts(self) -> (ferrodec_multiword::U256, i32, bool) {
        match crate::bid::classify_bits(self.to_bits()) {
            crate::bid::Class::Zero { sign, biased_exp } => (
                ferrodec_multiword::U256::ZERO,
                biased_exp as i32 - crate::bid::BIAS as i32,
                sign,
            ),
            crate::bid::Class::Finite {
                sign,
                biased_exp,
                coefficient,
            } => (
                ferrodec_multiword::U256::from_u128(coefficient),
                biased_exp as i32 - crate::bid::BIAS as i32,
                sign,
            ),
            _ => panic!("Decimal128::to_extended_parts: NaN / Inf not representable"),
        }
    }

    fn round_and_pack_finite(
        coef: ferrodec_multiword::U256,
        unbiased_exp: i32,
        q_preferred: i32,
        sign: bool,
        pre_sticky: bool,
        rm: RoundingMode,
        status: Status,
    ) -> (Self, Status) {
        crate::ops::round_and_pack_finite(
            coef,
            unbiased_exp,
            q_preferred,
            sign,
            pre_sticky,
            rm,
            status,
        )
    }

    fn recip_seed(self, rm: RoundingMode) -> (Self, Status) {
        Decimal128::ONE.div(self, rm)
    }

    fn sqrt_seed(self, rm: RoundingMode) -> (Self, Status) {
        self.sqrt(rm)
    }

    fn div_fmt(self, other: Self, rm: RoundingMode) -> (Self, Status) {
        self.div(other, rm)
    }
}
