//! `impl DecimalFormat for Decimal64` (fd-r0l P1-A).
//!
//! Every method is a thin forward to `Decimal64`'s already-verified
//! inherent surface, exactly as `ferrodec`'s `math/format_impl.rs`
//! does for `Decimal128`. The trait carries no arithmetic of its own,
//! so the shared `ferrodec-transcend` kernel instantiated at
//! `F = Decimal64` reuses one verified implementation rather than the
//! pre-fd-r0l lossy `f64` / `libm` detour.
//!
//! The one place this is *not* a pure forward is
//! [`round_and_pack_finite`](DecimalFormat::round_and_pack_finite):
//! the kernel hands a `U256` coefficient up to `Extended`'s 50-digit
//! working width, whereas `Decimal64`'s verified rounder accepts a
//! `u128` (≤ 38 digits). The adapter narrows `U256 → u128` by the
//! same base-10 digit-drop-with-sticky reduction `Decimal64`'s own
//! `round_and_pack_into_u64` performs for its `u128 → u64`
//! compression: it makes no rounding decision (sticky preservation
//! only) and then forwards to the verified rounder.

use crate::bid::{classify_bits, pack_quiet_nan, BIAS};
use crate::decimal::Decimal64;
use crate::ops::addsub::round_and_pack_into_u64;
use ferrodec_ieee::{IeeeDecodedClass, RoundingMode, Status};
use ferrodec_multiword::U256;
use ferrodec_transcend::extended::Extended;
use ferrodec_transcend::DecimalFormat;

/// Map `Decimal64`'s local `bid::Class` (u64 coefficient / payload) to
/// the family-shared [`IeeeDecodedClass`] (u128 fields) the kernel
/// consumes. Field values are preserved exactly; only the integer
/// width widens. `bid::Class` is left untouched (c1 did not move
/// `Decimal64`'s local enum).
#[inline]
fn to_ieee_class(c: crate::bid::Class) -> IeeeDecodedClass {
    match c {
        crate::bid::Class::Finite {
            sign,
            biased_exp,
            coefficient,
        } => IeeeDecodedClass::Finite {
            sign,
            biased_exp,
            coefficient: u128::from(coefficient),
        },
        crate::bid::Class::Zero { sign, biased_exp } => IeeeDecodedClass::Zero { sign, biased_exp },
        crate::bid::Class::Infinity { sign } => IeeeDecodedClass::Infinity { sign },
        crate::bid::Class::QuietNaN { sign, payload } => IeeeDecodedClass::QuietNaN {
            sign,
            payload: u128::from(payload),
        },
        crate::bid::Class::SignalingNaN { sign, payload } => IeeeDecodedClass::SignalingNaN {
            sign,
            payload: u128::from(payload),
        },
    }
}

/// Build the canonical quiet NaN for a NaN datum, preserving its sign
/// and trailing-significand payload. Mirrors the inline pattern in
/// `ops/exp.rs`'s `exp_special_cases` (and `addsub.rs`'s
/// `handle_specials`) so NaN payload behaviour is byte-identical to
/// `Decimal64`'s existing kernels.
#[inline]
fn quiet_nan_of(d: Decimal64) -> Decimal64 {
    match classify_bits(d.to_bits()) {
        crate::bid::Class::QuietNaN { sign, payload }
        | crate::bid::Class::SignalingNaN { sign, payload } => {
            Decimal64::from_bits(pack_quiet_nan(sign, payload))
        }
        // Caller guarantees `d.is_nan()`; the non-NaN arms are
        // unreachable in practice. Returning a canonical quiet NaN
        // keeps the function total without a panic.
        _ => Decimal64::NAN,
    }
}

impl DecimalFormat for Decimal64 {
    const BIAS: i32 = crate::bid::BIAS as i32;
    const PRECISION: u32 = crate::bid::PRECISION;

    const ZERO: Self = Decimal64::ZERO;
    const NEG_ZERO: Self = Decimal64::NEG_ZERO;
    const ONE: Self = Decimal64::ONE;
    const NEG_ONE: Self = Decimal64::NEG_ONE;
    const TEN: Self = Decimal64::TEN;
    const INFINITY: Self = Decimal64::INFINITY;
    const NEG_INFINITY: Self = Decimal64::NEG_INFINITY;
    const NAN: Self = Decimal64::NAN;
    const SIGNALING_NAN: Self = Decimal64::SIGNALING_NAN;

    fn classify(self) -> IeeeDecodedClass {
        to_ieee_class(classify_bits(self.to_bits()))
    }

    fn is_nan(self) -> bool {
        Decimal64::is_nan(self)
    }

    fn is_zero(self) -> bool {
        Decimal64::is_zero(self)
    }

    fn is_infinite(self) -> bool {
        Decimal64::is_infinite(self)
    }

    fn is_sign_negative(self) -> bool {
        Decimal64::is_sign_negative(self)
    }

    fn is_signaling_nan(self) -> bool {
        Decimal64::is_signaling_nan(self)
    }

    fn abs(self) -> Self {
        Decimal64::abs(self)
    }

    fn neg(self) -> Self {
        Decimal64::neg(self)
    }

    fn partial_cmp_fmt(self, other: Self) -> (Option<core::cmp::Ordering>, Status) {
        self.partial_cmp(other)
    }

    fn nan_from(self) -> Self {
        quiet_nan_of(self)
    }

    fn propagate_nan2(self, other: Self) -> Self {
        if self.is_nan() {
            quiet_nan_of(self)
        } else {
            quiet_nan_of(other)
        }
    }

    fn to_extended_parts(self) -> Option<(U256, i32, bool)> {
        match classify_bits(self.to_bits()) {
            crate::bid::Class::Zero { sign, biased_exp } => {
                Some((U256::ZERO, biased_exp as i32 - BIAS as i32, sign))
            }
            crate::bid::Class::Finite {
                sign,
                biased_exp,
                coefficient,
            } => Some((
                U256::from_u128(u128::from(coefficient)),
                biased_exp as i32 - BIAS as i32,
                sign,
            )),
            // NaN / Inf are not representable in the Extended kernel form.
            _ => None,
        }
    }

    fn round_and_pack_finite(
        coef: U256,
        unbiased_exp: i32,
        q_preferred: i32,
        sign: bool,
        pre_sticky: bool,
        rm: RoundingMode,
        status: Status,
    ) -> (Self, Status) {
        // The kernel's `Extended` coefficient is ≤ 50 digits (U256);
        // `Decimal64`'s verified rounder
        // (`round_and_pack_into_u64`) accepts a `u128` (≤ 38 digits)
        // and itself performs the final `u128 → u64` digit-drop with
        // sticky tracking before delegating to `round_and_pack_finite`.
        // Reduce `U256 → u128` here by the identical base-10 sticky
        // reduction so no rounding decision is made outside the
        // verified rounder: drop low digits while the value exceeds
        // the u128 envelope, OR-ing any non-zero dropped digit into
        // the sticky and shifting the exponent up to compensate.
        //
        // `status` (always `Status::OK` from `Extended::to_format`'s
        // single call site) is intentionally not forwarded:
        // `round_and_pack_into_u64` opens a fresh `Status::OK` exactly
        // as the `Decimal128` forward does (it calls
        // `crate::ops::round_and_pack_finite(..., status)` where the
        // kernel only ever passes `Status::OK`). The kernels OR
        // `INEXACT` in at their own boundary.
        let _ = status;

        let mut c = coef;
        let mut sticky = pre_sticky;
        let mut shift: i32 = 0;
        // `c.hi != 0` ⇔ the value needs more than 128 bits ⇒ more
        // than a `u128` can hold. Drop one base-10 digit per step;
        // bounded by U256's ≤ 78-digit envelope.
        while c.hi != 0 {
            let (q, r) = c.div_rem10();
            if r != 0 {
                sticky = true;
            }
            c = q;
            shift += 1;
        }
        // `c.hi == 0`, so `c.lo` is the full u128 coefficient.
        round_and_pack_into_u64(c.lo, unbiased_exp + shift, q_preferred, sign, sticky, rm)
    }

    fn recip_seed(self, rm: RoundingMode) -> (Self, Status) {
        Decimal64::ONE.div(self, rm)
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

    /// Overflow threshold for `exp(x)`. `Decimal64` has `E_MAX = 384`,
    /// so `MAX ≈ 10^385`; `e^x` overflows once `x > ln(10^385) =
    /// 385 · ln(10) ≈ +886.49`. The first integer strictly past that
    /// boundary is `887`, so any `|x| > 887` short-circuits through
    /// the kernel's saturation gate (§7.4 disposition per rounding
    /// direction, plus `OVERFLOW`). `Extended::from_u128(887)` is `coef = 887`,
    /// `exp = 0`, `sign = false` — the analogue of `Decimal128`'s
    /// `14150` figure, scaled to `Decimal64`'s exponent envelope.
    fn exp_overflow_limit() -> Extended {
        Extended::from_u128(887)
    }

    /// Underflow threshold for `exp(x)`. `Decimal64`'s smallest
    /// representable subnormal is `10^(E_MIN − (PRECISION − 1)) =
    /// 10^(−383 − 15) = 10⁻³⁹⁸`, and round-to-nearest-even maps any
    /// `exp(x) < ½ × MIN_SUBNORMAL` to `+0`. That boundary sits at
    /// `x ≈ ln(½ × 10⁻³⁹⁸) = ln(0.5) + (−398)·ln(10) ≈ −0.693 −
    /// 916.29 ≈ −916.98`. Mirroring `Decimal128`'s underflow
    /// derivation (`⌈|x|⌉ + 1`): `⌈916.98⌉ = 917`, `+1 ⇒ 918` is the
    /// first integer past which the saturate short-circuit is safe.
    /// The asymmetry with the overflow side (`887`) is intrinsic to
    /// `Decimal64`'s lopsided exponent range (`E_MAX = 384`,
    /// `MIN_SUBNORMAL` exponent = `−398`), exactly as for
    /// `Decimal128`. Inputs in `(−918, −887]` produce representable
    /// subnormals — the Taylor pipeline handles them and they must
    /// NOT short-circuit to zero.
    fn exp_underflow_limit() -> Extended {
        Extended::from_u128(918)
    }
}
