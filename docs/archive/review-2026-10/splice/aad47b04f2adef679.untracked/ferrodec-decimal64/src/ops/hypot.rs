//! IEEE 754-2019 §9.2 `hypot` for [`Decimal64`].
//!
//! `hypot(x, y) = sqrt(x² + y²)`, correctly rounded at every rounding
//! direction. Pure delegation onto the shared `ferrodec-transcend`
//! kernel, which resolves every §9.2.1 special value internally and
//! runs the ADR-0059 escalation ladder over the ADR-0060 two band
//! design (an anchor band where the true value hugs the larger
//! operand's grid point, and a kernel band whose exact and tie set is
//! classified from the operands).
//!
//! # Special cases (§9.2.1)
//!
//! * `hypot(±0, ±0) = +0`.
//! * Any `±∞` operand gives `+∞`, *including* against a quiet NaN:
//!   `hypot(±∞, qNaN)` and `hypot(qNaN, ±∞)` are both `+∞`.
//! * A signaling NaN anywhere gives a quiet NaN with `INVALID`, and it
//!   outranks the infinity rule above (§6.2 / §7.2).
//! * A quiet NaN with a finite other operand propagates.
//! * `hypot(x, ±0) = |x|` exactly, no exception raised.
//! * The result is always positive; neither operand's sign reaches it.
//! * `hypot(x, y)` and `hypot(y, x)` deliver identical bits and flags
//!   for every non-NaN operand pair.
//!
//! # Preferred exponent (§9.2.2)
//!
//! `Q(hypot(x, y))` is `min(Q(x), Q(y))`, honoured on every exact
//! delivery. An inexact result carries the full format precision.

use crate::decimal::Decimal64;
use ferrodec_ieee::{RoundingMode, Status};

impl Decimal64 {
    /// IEEE 754-2019 §9.2 `hypot(self, y)`: `sqrt(self² + y²)`,
    /// computed so that neither the intermediate squares nor their sum
    /// can overflow or underflow when the true result is
    /// representable.
    ///
    /// Correctly rounded at every rounding direction, at exact parity
    /// with the `Decimal128` parent. This module's header lists the
    /// §9.2.1 special values; the derivation of the two bands, the
    /// exactness classification, and the error budget live on
    /// `ferrodec_transcend::hypot::hypot_kernel` and `ladder::HYPOT`.
    #[must_use]
    pub fn hypot(self, y: Self, rm: RoundingMode) -> (Self, Status) {
        ferrodec_transcend::hypot::hypot_kernel::<Decimal64>(self, y, rm)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Decimal64 {
        Decimal64::parse_str(s, RoundingMode::NearestEven)
            .unwrap()
            .0
    }

    /// Cohort-insensitive value equality (the IEEE `compare`).
    fn equal(a: Decimal64, b: Decimal64) -> bool {
        a.partial_cmp(b).0 == Some(core::cmp::Ordering::Equal)
    }

    #[test]
    fn pythagorean_triples_are_exact() {
        for (a, b, c) in [("3", "4", "5"), ("5", "12", "13"), ("20", "21", "29")] {
            let (r, st) = parse(a).hypot(parse(b), RoundingMode::NearestEven);
            assert!(equal(r, parse(c)), "hypot({a}, {b})");
            assert_eq!(st, Status::OK, "hypot({a}, {b}) flags");
        }
    }

    #[test]
    fn infinity_beats_quiet_nan() {
        let (r, st) = Decimal64::INFINITY.hypot(Decimal64::NAN, RoundingMode::NearestEven);
        assert!(r.is_infinite() && !r.is_sign_negative());
        assert_eq!(st, Status::OK);
    }

    #[test]
    fn irrational_case_is_inexact() {
        let (r, st) = parse("1").hypot(parse("1"), RoundingMode::NearestEven);
        assert_eq!(st, Status::INEXACT);
        // sqrt(2) to 16 digits.
        assert!(equal(r, parse("1.414213562373095")));
    }
}
