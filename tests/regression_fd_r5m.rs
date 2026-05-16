//! Regression pin for fd-r5m — `cbrt` of a negative argument rounded
//! the wrong way under the two directed modes.
//!
//! Surfaced by the S5 faithful-rounding oracle. `cbrt` evaluates
//! `exp(ln|x|/3)` on the magnitude and re-applies the sign. The
//! magnitude was rounded under the caller's `rm` and *then* negated,
//! but negation reflects the real line about zero, so rounding
//! `|cbrt(x)|` toward `−∞` and negating yields `cbrt(x)` rounded toward
//! `+∞` (and vice versa). For a negative argument the two directed
//! modes therefore rounded by up to one ULP in the wrong direction.
//! The fix reflects the rounding mode (`RoundingMode::for_negation`)
//! before rounding the magnitude.
//!
//! Reproducer: `cbrt(-2)` under `TowardNegative`. The exact value is
//! `−1.2599210498948731647672106072782283505…`; the representable
//! value `≤` it (toward `−∞`) is `−…229`, but the pre-fix kernel
//! returned `−…228` (the toward-zero neighbour). cbrt(2)'s 34-digit
//! directed roundings (independently, via `decimal`):
//! `TowardZero/TowardNegative/Nearest* = …228`, `TowardPositive =
//! …229`; the negative argument mirrors them.

#![cfg(feature = "transcendentals")]

use ferrodec::{Decimal128, RoundingMode};

const MODES: &[RoundingMode] = &[
    RoundingMode::NearestEven,
    RoundingMode::NearestAway,
    RoundingMode::TowardZero,
    RoundingMode::TowardPositive,
    RoundingMode::TowardNegative,
];

fn parse(s: &str) -> Decimal128 {
    Decimal128::parse_str(s, RoundingMode::NearestEven)
        .unwrap()
        .0
}

/// cbrt(2) truncated / floored to 34 significant digits.
const CBRT2_DOWN: &str = "1.259921049894873164767210607278228";
/// cbrt(2) rounded toward +∞ to 34 significant digits.
const CBRT2_UP: &str = "1.259921049894873164767210607278229";

fn bits_eq(a: Decimal128, b: Decimal128) -> bool {
    a.to_bits() == b.to_bits()
}

#[test]
fn cbrt_two_directed_rounding_exact() {
    let x = parse("2");
    let down = parse(CBRT2_DOWN);
    let up = parse(CBRT2_UP);
    for &rm in MODES {
        let (got, status) = x.cbrt(rm);
        assert!(!status.invalid(), "cbrt(2) rm={rm:?} raised INVALID");
        let expect = match rm {
            // True value's 35th digit is 3 (< 5): nearest rounds down.
            RoundingMode::NearestEven
            | RoundingMode::NearestAway
            | RoundingMode::TowardZero
            | RoundingMode::TowardNegative => down,
            RoundingMode::TowardPositive => up,
        };
        assert!(
            bits_eq(got, expect),
            "cbrt(2) rm={rm:?}: got {got:e}, want {expect:e}"
        );
    }
}

#[test]
fn cbrt_neg_two_directed_rounding_exact() {
    // cbrt(-2) = -cbrt(2); the directed result for the negative
    // argument mirrors cbrt(2) under the *reflected* mode.
    let x = parse("-2");
    let down = parse(CBRT2_DOWN); // |·| toward zero
    let up = parse(CBRT2_UP); //   |·| away from zero
    for &rm in MODES {
        let (got, status) = x.cbrt(rm);
        assert!(!status.invalid(), "cbrt(-2) rm={rm:?} raised INVALID");
        let expect = match rm {
            RoundingMode::NearestEven
            | RoundingMode::NearestAway
            | RoundingMode::TowardZero
            // toward +∞ on a negative value ⇒ toward zero ⇒ -|down|
            | RoundingMode::TowardPositive => down.neg(),
            // toward −∞ ⇒ away from zero ⇒ -|up|  (the pre-fix bug
            // returned -|down| here).
            RoundingMode::TowardNegative => up.neg(),
        };
        assert!(
            bits_eq(got, expect),
            "cbrt(-2) rm={rm:?}: got {got:e}, want {expect:e}"
        );
    }
}

/// The negation/round-reflection identity holds for every mode:
/// `cbrt(-x, rm) == -cbrt(x, rm.for_negation())`.
#[test]
fn cbrt_sign_reflection_identity() {
    let x = parse("2");
    let nx = parse("-2");
    for &rm in MODES {
        let (neg_got, _) = nx.cbrt(rm);
        let (pos, _) = x.cbrt(rm.for_negation());
        assert!(
            bits_eq(neg_got, pos.neg()),
            "cbrt(-2) rm={rm:?} != -(cbrt(2) rm={:?})",
            rm.for_negation()
        );
    }
}
