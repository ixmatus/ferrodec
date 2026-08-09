//! Delegating shim: the `rootn` kernel lives in `ferrodec-transcend`
//! (ADR-0059 Track D group D3). The public `Decimal128::rootn`
//! wrapper and its behaviour tests stay here.

use crate::decimal::Decimal128;
use ferrodec_ieee::{RoundingMode, Status};

impl Decimal128 {
    /// IEEE 754-2019 §9.2 `rootn(self, n)` rounded by `rm`: the `n`-th
    /// root of `self` for an integer `n`.
    ///
    /// Defined on the whole real line for odd `n` and on `[0, ∞)` for
    /// even `n`. `rootn(x, 1)` is `x`, `rootn(x, 2)` is `sqrt(x)`,
    /// `rootn(x, 3)` is `cbrt(x)`, and `rootn(x, −n)` is the
    /// reciprocal of `rootn(x, n)`.
    ///
    /// # Special cases (IEEE 754-2019 §9.2.1)
    ///
    /// * NaN propagates; a signaling NaN raises `INVALID`.
    /// * `rootn(±0, n)` is `±∞` with `DIV_BY_ZERO` for odd `n < 0`,
    ///   and `+∞` with `DIV_BY_ZERO` for even `n < 0`.
    /// * `rootn(±0, n)` is `±0` for odd `n > 0` and `+0` for even
    ///   `n > 0`. The standard's NOTE beside the table calls out the
    ///   consequence: `rootn(−0, 2)` is `+0` while `sqrt(−0)` is `−0`.
    /// * `rootn(+∞, n)` is `+∞` for `n > 0` and `+0` for `n < 0`;
    ///   `rootn(−∞, n)` is `−∞` for odd `n > 0` and `−0` for odd
    ///   `n < 0`.
    /// * A negative operand (finite or `−∞`) with even `n` is a quiet
    ///   NaN with `INVALID`.
    /// * `n = 0` is absent from the standard's table, which leaves the
    ///   case to the implementation; this returns a quiet NaN with
    ///   `INVALID`, matching MPFR's `rootn`.
    ///
    /// # Accuracy
    ///
    /// Correctly rounded on the ADR-0059 escalation ladder.
    /// `|n| ≤ 2` delegates to IEEE 754-2019 §5 basic operations (the
    /// identity, division, and square root), and everything else
    /// classifies exact results and ties from the input before
    /// evaluating `exp(ln|x| / n)` at working precision.
    #[must_use]
    pub fn rootn(self, n: i32, rm: RoundingMode) -> (Self, Status) {
        ferrodec_transcend::rootn::rootn_kernel::<Decimal128>(self, n, rm)
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

    fn equal(a: Decimal128, b: Decimal128) -> bool {
        a.partial_cmp(b).0 == Some(core::cmp::Ordering::Equal)
    }

    #[test]
    fn rootn_matches_cbrt_at_three() {
        for s in ["8", "27", "-64", "0.001", "2", "1E+30", "-5.5"] {
            let x = parse(s);
            let (a, sa) = x.rootn(3, RoundingMode::NearestEven);
            let (b, sb) = x.cbrt(RoundingMode::NearestEven);
            assert!(equal(a, b), "rootn({s}, 3) = {a}, cbrt({s}) = {b}");
            assert_eq!(sa, sb, "rootn({s}, 3) flags differ from cbrt");
        }
    }

    #[test]
    fn rootn_matches_sqrt_at_two() {
        for s in ["4", "2", "1E+30", "0.25", "1E-6000"] {
            let x = parse(s);
            let (a, sa) = x.rootn(2, RoundingMode::NearestEven);
            let (b, sb) = x.sqrt(RoundingMode::NearestEven);
            assert!(equal(a, b), "rootn({s}, 2) = {a}, sqrt({s}) = {b}");
            assert_eq!(sa, sb);
        }
    }

    #[test]
    fn rootn_identity_and_reciprocal() {
        let x = parse("7.5");
        let (r, st) = x.rootn(1, RoundingMode::NearestEven);
        assert!(equal(r, x));
        assert_eq!(st, Status::OK);
        let (r, _) = x.rootn(-1, RoundingMode::NearestEven);
        let (want, _) = Decimal128::ONE.div(x, RoundingMode::NearestEven);
        assert!(equal(r, want));
    }

    #[test]
    fn rootn_exact_perfect_powers() {
        for (s, n, want) in [("8", 3, "2"), ("32", 5, "2"), ("1E+30", 5, "1E+6")] {
            let (r, st) = parse(s).rootn(n, RoundingMode::NearestEven);
            assert!(equal(r, parse(want)), "rootn({s}, {n}) = {r}, want {want}");
            assert!(!st.inexact(), "rootn({s}, {n}) must not raise INEXACT");
        }
    }

    #[test]
    fn rootn_negative_base_odd_n() {
        let (r, _) = parse("-27").rootn(3, RoundingMode::NearestEven);
        assert!(equal(r, parse("-3")));
    }

    #[test]
    fn rootn_negative_base_even_n_is_invalid() {
        let (r, st) = parse("-4").rootn(2, RoundingMode::NearestEven);
        assert!(r.is_nan());
        assert!(st.invalid());
    }

    #[test]
    fn rootn_zero_n_is_invalid() {
        let (r, st) = parse("8").rootn(0, RoundingMode::NearestEven);
        assert!(r.is_nan());
        assert!(st.invalid());
    }
}
