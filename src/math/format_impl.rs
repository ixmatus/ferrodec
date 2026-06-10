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
use ferrodec_transcend::extended::Extended;
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

    fn mul_fmt(self, other: Self, rm: RoundingMode) -> (Self, Status) {
        self.mul(other, rm)
    }

    fn to_i32_fmt(self, rm: RoundingMode) -> (i32, Status) {
        self.to_i32(rm)
    }

    /// Overflow threshold for `exp(x)`. `e^x` overflows at
    /// `x ≈ ln(MAX) ≈ +14149.4`; values strictly above this
    /// short-circuit through the kernel's saturation gate, which
    /// applies the §7.4 overflow disposition per rounding direction
    /// (`+∞` at the nearest modes and toward `+∞`, `MAX` toward zero
    /// and `−∞`) plus `OVERFLOW`. `Extended::from_u128(14150)`
    /// reproduces the pre-relocation `Extended::EXP_OVERFLOW_LIMIT`
    /// const exactly (`coef = 14150`, `exp = 0`, `sign = false`), so
    /// the Decimal128 magnitude gate is bit-identical.
    fn exp_overflow_limit() -> Extended {
        Extended::from_u128(14150)
    }

    /// Underflow threshold for `exp(x)`. The smallest representable
    /// subnormal is `1 × 10⁻⁶¹⁷⁶`, and round-to-nearest-even maps
    /// any `exp(x) < ½ × MIN_SUBNORMAL` to `+0`. That boundary sits
    /// at `x ≈ ln(0.5 × 10⁻⁶¹⁷⁶) ≈ −14220.85`, so `+14221` is the
    /// first integer past which the saturate short-circuit is safe:
    /// below half the smallest subnormal every rounding direction's
    /// answer is decided (`+0`, or the smallest subnormal toward
    /// `+∞`) and the kernel's saturation gate delivers it per mode.
    /// Setting the underflow threshold at `+14150` (matching the
    /// overflow side) was too tight — it discarded every
    /// subnormal-range result for `x ∈ (−14221, −14150]`, which the
    /// Taylor pipeline is fully capable of producing. The asymmetry
    /// is intrinsic to decimal128's lopsided exponent range
    /// (`E_MAX` = 6144, `MIN_SUBNORMAL` exponent = −6176).
    /// `Extended::from_u128(14221)` reproduces the pre-relocation
    /// `Extended::EXP_UNDERFLOW_LIMIT` const exactly.
    fn exp_underflow_limit() -> Extended {
        Extended::from_u128(14221)
    }
}
