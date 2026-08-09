//! Delegating shim: the `rSqrt` kernel lives in ferrodec-transcend
//! (ADR-0059 Track D group D3). The public `Decimal128::rsqrt` wrapper
//! and its behaviour tests stay here as the byte-identical regression
//! gate, the same shape `cbrt` carries.

use crate::decimal::Decimal128;
use ferrodec_ieee::{RoundingMode, Status};

impl Decimal128 {
    /// IEEE 754-2019 §9.2 `rSqrt(self)`: the reciprocal square root
    /// `1/√self`, rounded by `rm`.
    ///
    /// ## Accuracy
    ///
    /// Correctly rounded, and **unconditionally** so in the default
    /// two rung build. ADR-0060 derives a uniform Liouville floor for
    /// this operation — every input the classifier declines has a true
    /// value whose relative distance to every grid point and every
    /// nearest mode midpoint exceeds `4.9·10^-105` — and rung 2's
    /// audited budget resolves to `4·10^-108`, clearing it by more than
    /// two orders. There is no exception set to state, in any build.
    ///
    /// The architecture that makes the claim reachable is forced rather
    /// than chosen: the kernel is a direct Newton composition
    /// (`√x`, then its reciprocal, then one division-free polish step)
    /// at the escalation ladder's working precision, because the
    /// `exp(−½·ln self)` route inherits the `|ln x| ≤ 14151`
    /// amplification and cannot clear the floor at any fixed rung. The
    /// derivation, the budget itemization, and the per-format seed
    /// width table live on `ferrodec_transcend::rsqrt` and
    /// `ladder::RSQRT`.
    ///
    /// ## Exact results and ties (§7.5)
    ///
    /// `1/√self` is rational exactly when the stripped input is
    /// `2^A · 5^B` with both exponents even, and the value is then a
    /// pure power of two or of five over a power of ten: `rsqrt(4)` is
    /// `0.5`, `rsqrt(0.04)` is `5`, `rsqrt(6.25)` is `0.4`,
    /// `rsqrt(1E-2k)` is `1E+k` across the whole exponent range. Those
    /// are delivered from the exact coefficient at every rounding
    /// direction with no `INEXACT`. The family also holds real nearest
    /// mode midpoints — powers of five end in 5, so `rsqrt(2^98)` is
    /// the 35-digit `5^49·10^-49` — which the format rounder's own tie
    /// rule resolves; no approximation kernel can, because the true
    /// value IS the boundary. The completeness proof lives on
    /// `ferrodec_transcend`'s `exact::rsqrt_exact_input`.
    ///
    /// ## Special values (IEEE 754-2019 §9.2.1)
    ///
    /// * `rSqrt(+∞)` is `+0`, with no exception.
    /// * `rSqrt(±0)` is `±∞` and signals `DIV_BY_ZERO`; the sign is
    ///   preserved, so `rSqrt(−0)` is `−∞`.
    /// * Every other negative operand, finite or `−∞`, is a domain
    ///   error: a quiet NaN with `INVALID`.
    /// * NaN propagates; a signaling NaN raises `INVALID` and returns
    ///   the quieted payload.
    /// * No finite nonzero operand can overflow or underflow: `rSqrt`
    ///   halves the exponent, and the result of every representable
    ///   input lands strictly inside the normal range
    ///   (`10^3088` at the smallest subnormal, `~3.16·10^-3073` at
    ///   `MAX`).
    #[must_use]
    #[doc(alias = "rSqrt")]
    pub fn rsqrt(self, rm: RoundingMode) -> (Self, Status) {
        ferrodec_transcend::rsqrt::rsqrt_kernel::<Decimal128>(self, rm)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Decimal128 {
        Decimal128::parse_str(s, RoundingMode::NearestEven)
            .unwrap()
            .0
    }

    fn eq(got: Decimal128, want: Decimal128) -> bool {
        got.partial_cmp(want).0 == Some(core::cmp::Ordering::Equal)
    }

    #[test]
    fn rsqrt_specials() {
        let (r, st) = Decimal128::INFINITY.rsqrt(RoundingMode::NearestEven);
        assert!(r.is_zero() && !r.is_sign_negative(), "rSqrt(+inf) = {r}");
        assert_eq!(st, Status::OK);

        let (r, st) = Decimal128::ZERO.rsqrt(RoundingMode::NearestEven);
        assert!(r.is_infinite() && !r.is_sign_negative(), "rSqrt(+0) = {r}");
        assert!(st.div_by_zero());

        let (r, st) = Decimal128::NEG_ZERO.rsqrt(RoundingMode::NearestEven);
        assert!(r.is_infinite() && r.is_sign_negative(), "rSqrt(-0) = {r}");
        assert!(st.div_by_zero());

        let (r, st) = Decimal128::NEG_INFINITY.rsqrt(RoundingMode::NearestEven);
        assert!(r.is_nan() && st.invalid(), "rSqrt(-inf) = {r} {st:?}");

        let (r, st) = parse("-4").rsqrt(RoundingMode::NearestEven);
        assert!(r.is_nan() && st.invalid(), "rSqrt(-4) = {r} {st:?}");

        let (r, _) = Decimal128::NAN.rsqrt(RoundingMode::NearestEven);
        assert!(r.is_nan());
        let (r, st) = Decimal128::SIGNALING_NAN.rsqrt(RoundingMode::NearestEven);
        assert!(r.is_nan() && st.invalid());
    }

    #[test]
    fn rsqrt_exact_family() {
        for (input, want) in [
            ("4", "0.5"),
            ("0.04", "5"),
            ("6.25", "0.4"),
            ("0.25", "2"),
            ("1", "1"),
            ("100", "0.1"),
            ("0.0001", "100"),
        ] {
            let (r, st) = parse(input).rsqrt(RoundingMode::NearestEven);
            assert!(eq(r, parse(want)), "rsqrt({input}) = {r}, want {want}");
            assert!(!st.inexact(), "rsqrt({input}) must not raise INEXACT");
        }
    }

    #[test]
    fn rsqrt_two_is_inexact_and_near_the_reference() {
        let (r, st) = parse("2").rsqrt(RoundingMode::NearestEven);
        assert!(st.inexact());
        assert!(
            eq(r, parse("0.7071067811865475244008443621048490")),
            "rsqrt(2) = {r}"
        );
    }
}
