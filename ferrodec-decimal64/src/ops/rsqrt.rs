//! IEEE 754-2019 §9.2 `rSqrt` for [`Decimal64`]: the reciprocal square
//! root `1/√x` (ADR-0059 Track D group D3, under the ADR-0060 phase
//! gate).
//!
//! Pure delegation onto the shared `ferrodec-transcend` kernel, which
//! resolves every §9.2.1 special value internally and runs the ADR-0059
//! escalation ladder from this operation's first release. The kernel is
//! a direct Newton composition rather than `exp(−½·ln x)`, an
//! architecture ADR-0060 forces: the exp/ln route's error budget cannot
//! clear the operation's proven `4.9·10^-105` Liouville floor, and the
//! Newton kernel's can.
//!
//! # Special cases (§9.2.1)
//!
//! * `rSqrt(+∞) = +0`, no exception.
//! * `rSqrt(±0) = ±∞`, signalling `DIV_BY_ZERO`; the sign is preserved.
//! * Every other negative operand, finite or `−∞`, is a domain error:
//!   quiet NaN with INVALID.
//! * NaN propagates (sNaN raises INVALID).
//! * No finite nonzero operand overflows or underflows: the operation
//!   halves the exponent, so every result of a representable
//!   `Decimal64` input lands strictly inside the normal range
//!   (`10^199` at the smallest subnormal, `~3.16·10^-193` at `MAX`).

use crate::decimal::Decimal64;
use ferrodec_ieee::{RoundingMode, Status};

impl Decimal64 {
    /// IEEE 754-2019 §9.2 `rSqrt(self)`: the reciprocal square root
    /// `1/√self`, rounded by `rm`.
    ///
    /// Correctly rounded, and unconditionally so in the default two
    /// rung build: ADR-0060's uniform Liouville floor for this
    /// operation (`4.9·10^-105` relative, derived at `Decimal128` and
    /// strictly easier here) sits more than two orders past what rung
    /// 2's audited budget resolves. Exact results and nearest mode ties
    /// are classified from the input alone — `rsqrt(4) = 0.5`,
    /// `rsqrt(0.04) = 5`, `rsqrt(2^48) = 5^24·10^-24` is a real
    /// 17-digit midpoint — so §7.5's ban on `INEXACT` for exact results
    /// holds in every rounding direction. The derivations live on
    /// `ferrodec_transcend::rsqrt`, `exact::rsqrt_exact_input`, and
    /// `ladder::RSQRT`; this module's header lists the §9.2.1 special
    /// values.
    #[must_use]
    #[doc(alias = "rSqrt")]
    pub fn rsqrt(self, rm: RoundingMode) -> (Self, Status) {
        ferrodec_transcend::rsqrt::rsqrt_kernel::<Decimal64>(self, rm)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::cmp::Ordering;

    fn parse(s: &str) -> Decimal64 {
        Decimal64::parse_str(s, RoundingMode::NearestEven)
            .unwrap()
            .0
    }

    fn eq(got: Decimal64, want: Decimal64) -> bool {
        got.partial_cmp(want).0 == Some(Ordering::Equal)
    }

    #[test]
    fn rsqrt_specials() {
        let (r, st) = Decimal64::INFINITY.rsqrt(RoundingMode::NearestEven);
        assert!(r.is_zero() && !r.is_sign_negative());
        assert!(st.is_ok());

        let (r, st) = Decimal64::ZERO.rsqrt(RoundingMode::NearestEven);
        assert!(r.is_infinite() && !r.is_sign_negative());
        assert!(st.div_by_zero());

        let (r, st) = Decimal64::NEG_ZERO.rsqrt(RoundingMode::NearestEven);
        assert!(r.is_infinite() && r.is_sign_negative());
        assert!(st.div_by_zero());

        let (r, st) = Decimal64::NEG_INFINITY.rsqrt(RoundingMode::NearestEven);
        assert!(r.is_nan() && st.invalid());

        let (r, st) = parse("-4").rsqrt(RoundingMode::NearestEven);
        assert!(r.is_nan() && st.invalid());

        let (r, _) = Decimal64::NAN.rsqrt(RoundingMode::NearestEven);
        assert!(r.is_nan());
        let (r, st) = Decimal64::SIGNALING_NAN.rsqrt(RoundingMode::NearestEven);
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
        ] {
            let (r, st) = parse(input).rsqrt(RoundingMode::NearestEven);
            assert!(eq(r, parse(want)), "rsqrt({input}) = {r}, want {want}");
            assert!(!st.inexact(), "rsqrt({input}) must not raise INEXACT");
        }
    }

    #[test]
    fn rsqrt_two_is_correctly_rounded() {
        let (r, st) = parse("2").rsqrt(RoundingMode::NearestEven);
        assert!(st.inexact());
        assert!(eq(r, parse("0.7071067811865475")), "rsqrt(2) = {r}");
    }
}
