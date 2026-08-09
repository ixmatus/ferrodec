//! Delegating shim for the IEEE 754-2019 §9.2 `hypot` kernel in
//! `ferrodec-transcend` (ADR-0060 Track D D3). The public
//! `Decimal128::hypot` wrapper and its behaviour tests stay here as the
//! byte-identical regression gate.

use crate::decimal::Decimal128;
use ferrodec_ieee::{RoundingMode, Status};

impl Decimal128 {
    /// IEEE 754-2019 §9.2 `hypot(self, y)`: `sqrt(self² + y²)`,
    /// computed so that neither the intermediate squares nor their sum
    /// can overflow or underflow when the true result is representable.
    ///
    /// Correctly rounded at every rounding direction. Pure delegation
    /// onto the shared kernel, which resolves every §9.2.1 special
    /// value internally and runs the ADR-0059 escalation ladder; the
    /// derivation of its two bands, its exactness classification, and
    /// its error budget live on `ferrodec_transcend::hypot::hypot_kernel`,
    /// `ferrodec_transcend`'s `exact::hypot_exact_or_tie`, and
    /// `ladder::HYPOT`.
    ///
    /// ## Special values (IEEE 754-2019 §9.2.1)
    ///
    /// * `hypot(±0, ±0) = +0`.
    /// * Any `±∞` operand gives `+∞`, *including* against a quiet NaN:
    ///   `hypot(±∞, qNaN)` and `hypot(qNaN, ±∞)` are both `+∞`.
    /// * A signaling NaN anywhere gives a quiet NaN with `INVALID`, and
    ///   outranks the infinity rule above (§6.2 / §7.2).
    /// * A quiet NaN with a finite other operand propagates.
    /// * `hypot(x, ±0) = |x|` exactly, no exception raised.
    /// * The result is always positive; neither operand's sign reaches
    ///   it.
    /// * `hypot(x, y)` and `hypot(y, x)` deliver identical bits and
    ///   flags for every non-NaN operand pair.
    ///
    /// ## Preferred exponent (IEEE 754-2019 §9.2.2)
    ///
    /// `Q(hypot(x, y))` is `min(Q(x), Q(y))`, honoured on every exact
    /// delivery (the zeros, `hypot(x, ±0)`, and the Pythagorean pairs
    /// the exactness classifier decides). An inexact result carries the
    /// full format precision.
    #[must_use]
    pub fn hypot(self, y: Self, rm: RoundingMode) -> (Self, Status) {
        ferrodec_transcend::hypot::hypot_kernel::<Decimal128>(self, y, rm)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate alloc;

    fn parse(s: &str) -> Decimal128 {
        Decimal128::parse_str(s, RoundingMode::NearestEven)
            .unwrap()
            .0
    }

    #[test]
    fn pythagorean_triples_are_exact() {
        for (a, b, c) in [
            ("3", "4", "5"),
            ("5", "12", "13"),
            ("8", "15", "17"),
            ("20", "21", "29"),
        ] {
            let (r, st) = parse(a).hypot(parse(b), RoundingMode::NearestEven);
            assert_eq!(alloc::format!("{r}"), c, "hypot({a}, {b})");
            assert_eq!(st, Status::OK, "hypot({a}, {b}) flags");
        }
    }

    #[test]
    fn zero_operand_returns_the_magnitude() {
        let (r, st) = parse("-7").hypot(Decimal128::ZERO, RoundingMode::NearestEven);
        assert_eq!(alloc::format!("{r}"), "7");
        assert_eq!(st, Status::OK);
    }

    #[test]
    fn infinity_beats_quiet_nan() {
        let (r, st) = Decimal128::INFINITY.hypot(Decimal128::NAN, RoundingMode::NearestEven);
        assert!(r.is_infinite() && !r.is_sign_negative());
        assert_eq!(st, Status::OK);
        let (r, st) = Decimal128::NAN.hypot(Decimal128::NEG_INFINITY, RoundingMode::NearestEven);
        assert!(r.is_infinite() && !r.is_sign_negative());
        assert_eq!(st, Status::OK);
    }

    #[test]
    fn irrational_case_is_inexact() {
        let (r, st) = parse("1").hypot(parse("1"), RoundingMode::NearestEven);
        assert_eq!(st, Status::INEXACT);
        // sqrt(2) to 34 digits.
        assert_eq!(alloc::format!("{r}"), "1.414213562373095048801688724209698");
    }
}
