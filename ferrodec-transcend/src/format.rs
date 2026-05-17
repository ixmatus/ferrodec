//! The [`DecimalFormat`] seam contract.
//!
//! Implemented once per sibling decimal format (Decimal128 in the core
//! `ferrodec` crate today; Decimal64 / Decimal32 later). Every method
//! is a thin forward to the format's already-verified inherent
//! surface: the trait carries no arithmetic of its own, so a concrete
//! instantiation of the shared kernel is byte-identical to a
//! hand-written per-format kernel. That property is what makes the
//! `ferrodec-transcend` extraction behaviour-neutral for the
//! formally-verified Decimal128 parent.

use core::cmp::Ordering;
use ferrodec_ieee::{IeeeDecodedClass as Class, RoundingMode, Status};
use ferrodec_multiword::U256;

/// The contract between the shared transcendental kernel and a
/// concrete IEEE 754-2019 decimal format.
///
/// ## The three generic boundaries
///
/// The kernel touches the format type only at:
///
/// * [`to_extended_parts`](DecimalFormat::to_extended_parts) — decode
///   a finite / zero datum into `(coefficient, unbiased exponent,
///   sign)` for the `Extended` intermediate. NaN / Inf are filtered
///   by the caller before this is reached.
/// * [`round_and_pack_finite`](DecimalFormat::round_and_pack_finite)
///   — round the kernel's `Extended` result back to the format. This
///   is a callback into the format's *existing verified* rounding
///   routine; the trait does not reimplement it.
/// * [`recip_seed`](DecimalFormat::recip_seed) /
///   [`sqrt_seed`](DecimalFormat::sqrt_seed) — the Newton seed for
///   `Extended::recip` / `Extended::sqrt`. Reached in production by
///   `tan`, the inverse-trig kernels, the hyperbolic kernels, and
///   `pow`. Seeding at the format's own precision is what keeps each
///   instantiation byte-identical to its pre-extraction kernel.
///
/// Everything else the kernel needs from the format (classification,
/// the named constants, NaN propagation, ordering) is loop-free and
/// cheap, so it is exposed directly rather than threaded through an
/// intermediate.
pub trait DecimalFormat: Copy + Sized {
    /// Exponent bias added to the unbiased quantum for storage
    /// (Decimal128 = 6176).
    const BIAS: i32;
    /// Working precision in decimal digits (Decimal128 = 34).
    const PRECISION: u32;

    /// `+0` with the canonical zero cohort.
    const ZERO: Self;
    /// `-0`.
    const NEG_ZERO: Self;
    /// `1`.
    const ONE: Self;
    /// `-1`.
    const NEG_ONE: Self;
    /// `10`.
    const TEN: Self;
    /// `+∞`.
    const INFINITY: Self;
    /// `-∞`.
    const NEG_INFINITY: Self;
    /// A canonical quiet NaN.
    const NAN: Self;
    /// A canonical signaling NaN.
    const SIGNALING_NAN: Self;

    /// Decode the bit pattern into its [`Class`].
    fn classify(self) -> Class;
    /// `true` iff this is any NaN (quiet or signaling).
    fn is_nan(self) -> bool;
    /// `true` iff this is a (signed) zero.
    fn is_zero(self) -> bool;
    /// `true` iff this is ±∞.
    fn is_infinite(self) -> bool;
    /// `true` iff the sign bit is set (including on NaN / zero).
    fn is_sign_negative(self) -> bool;
    /// `true` iff this is a signaling NaN.
    fn is_signaling_nan(self) -> bool;
    /// Absolute value (clears the sign bit; NaN stays NaN).
    #[must_use]
    fn abs(self) -> Self;
    /// Negation (flips the sign bit; NaN stays NaN). Used by the odd
    /// transcendental kernels (`cbrt`) that evaluate on `|x|` and
    /// re-apply the sign.
    #[must_use]
    fn neg(self) -> Self;
    /// IEEE 754 partial comparison. `None` iff either operand is NaN;
    /// the `Status` carries `INVALID` on a signaling-NaN operand.
    fn partial_cmp_fmt(self, other: Self) -> (Option<Ordering>, Status);

    /// Build this format's quiet-NaN result from a NaN input,
    /// preserving the format's payload rule. Caller guarantees
    /// `self.is_nan()`.
    #[must_use]
    fn nan_from(self) -> Self;
    /// Quiet-NaN result for a binary op given at least one NaN
    /// operand (first-NaN-wins payload).
    #[must_use]
    fn propagate_nan2(self, other: Self) -> Self;

    /// **Boundary 1 — into the kernel.** Decompose a finite or zero
    /// datum into `(coefficient, unbiased_exponent, sign)`. The
    /// coefficient is widened to [`U256`] (the `Extended` working
    /// width); the exponent is unbiased (no [`BIAS`](Self::BIAS)
    /// offset). Caller must have filtered NaN / Inf.
    fn to_extended_parts(self) -> (U256, i32, bool);

    /// **Boundary 2 — out of the kernel.** Round an `Extended`
    /// result `(coef · 10^unbiased_exp, sign)` back to the format,
    /// honouring the IEEE 754 §6.3 preferred quantum `q_preferred`,
    /// the `pre_sticky` low-order residue, the rounding mode, and the
    /// accumulated `status`. This forwards to the format's existing
    /// verified `round_and_pack_finite`; the trait adds no rounding
    /// logic of its own.
    fn round_and_pack_finite(
        coef: U256,
        unbiased_exp: i32,
        q_preferred: i32,
        sign: bool,
        pre_sticky: bool,
        rm: RoundingMode,
        status: Status,
    ) -> (Self, Status);

    /// **Boundary 3a — Newton seed.** `1 / self` rounded at the
    /// format's own precision. Used to seed `Extended::recip`.
    fn recip_seed(self, rm: RoundingMode) -> (Self, Status);
    /// **Boundary 3b — Newton seed.** `sqrt(self)` rounded at the
    /// format's own precision. Used to seed `Extended::sqrt`.
    fn sqrt_seed(self, rm: RoundingMode) -> (Self, Status);
    /// `self / other` rounded at the format's precision. Distinct
    /// from the Newton seed: this is a real result-domain divide
    /// (e.g. `pow`'s final `ONE / result` inversion).
    fn div_fmt(self, other: Self, rm: RoundingMode) -> (Self, Status);
    /// `self * other` rounded at the format's precision. Used by
    /// `pow`'s square-and-multiply integer fast path, which only
    /// commits its result when no intermediate multiply rounded.
    fn mul_fmt(self, other: Self, rm: RoundingMode) -> (Self, Status);
    /// Round to the nearest `i32` under `rm`, reporting `INVALID`
    /// (via the `Status`) when the value does not fit. Used by
    /// `pow` to recover the integer exponent for the fast path.
    fn to_i32_fmt(self, rm: RoundingMode) -> (i32, Status);
}
