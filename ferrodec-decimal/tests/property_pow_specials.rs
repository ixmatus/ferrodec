//! The [`Decimal::power`] special-case table (IEEE 754-2019 section 9.2.1 /
//! General Decimal Arithmetic), asserted explicitly and cross-checked against
//! `power.decTest` and libmpdec. The notable departures from IEEE `pow` are
//! that `power(0, 0)` and `power(NaN, 0)` are not `1`, and that `power(0, -n)`
//! raises no `Division_by_zero` flag.

#![cfg(feature = "fmt")]

use ferrodec_decimal::{Context, Decimal, Rounding};

fn ctx(prec: u32) -> Context {
    Context::new(
        core::num::NonZeroU32::new(prec).unwrap(),
        999,
        -999,
        Rounding::HalfEven,
    )
}

/// `(rendered result, raised Invalid_operation)`.
fn pow(x: &str, y: &str, c: &Context) -> (String, bool) {
    let (r, s) = Decimal::parse_str(x)
        .unwrap()
        .power(&Decimal::parse_str(y).unwrap(), c);
    (r.to_string(), s.invalid())
}

#[test]
fn nan_propagation() {
    let c = ctx(9);
    // Quiet NaN propagates with no flag (no IEEE pow(NaN, 0) = 1 exception).
    for (x, y) in [
        ("NaN", "0"),
        ("0", "NaN"),
        ("1", "NaN"),
        ("NaN", "NaN"),
        ("NaN", "Infinity"),
    ] {
        assert_eq!(pow(x, y, &c), ("NaN".to_string(), false), "power({x}, {y})");
    }
    // Signaling NaN raises Invalid.
    assert_eq!(pow("sNaN", "1", &c), ("NaN".to_string(), true));
    assert_eq!(pow("1", "sNaN", &c), ("NaN".to_string(), true));
}

#[test]
fn exponent_zero() {
    let c = ctx(9);
    // power(0, 0) and signed variants are Invalid.
    for (x, y) in [("0", "0"), ("-0", "0"), ("0", "-0"), ("-0", "-0")] {
        assert_eq!(pow(x, y, &c), ("NaN".to_string(), true), "power({x}, {y})");
    }
    // Any other base to the zero power is 1 (exact).
    for x in ["0.1", "3", "-3", "1", "-1", "Infinity", "-Infinity"] {
        assert_eq!(pow(x, "0", &c), ("1".to_string(), false), "power({x}, 0)");
    }
}

#[test]
fn base_zero() {
    let c = ctx(9);
    // Signed zero / infinity by the sign of y and the parity of an odd integer
    // y over -0; no Division_by_zero flag for the negative exponents.
    let cases = [
        ("0", "1", "0"),
        ("0", "-1", "Infinity"),
        ("-0", "1", "-0"),
        ("-0", "-1", "-Infinity"),
        ("0", "2", "0"),
        ("-0", "2", "0"),
        ("0", "3", "0"),
        ("-0", "3", "-0"),
        ("0", "-2", "Infinity"),
        ("-0", "-2", "Infinity"),
        ("-0", "-3", "-Infinity"),
    ];
    for (x, y, want) in cases {
        assert_eq!(pow(x, y, &c), (want.to_string(), false), "power({x}, {y})");
    }
}

#[test]
fn base_one() {
    let c = ctx(9);
    // power(1, integer) = 1 exact; power(1, non-integer or infinite) = rounded 1.
    assert_eq!(pow("1", "5", &c), ("1".to_string(), false));
    assert_eq!(pow("1", "-1000", &c), ("1".to_string(), false));
    assert_eq!(pow("1", "1.01", &c), ("1.00000000".to_string(), false));
    assert_eq!(pow("1", "Infinity", &c), ("1.00000000".to_string(), false));
    assert_eq!(pow("1", "-Infinity", &c), ("1.00000000".to_string(), false));
    // The value is exactly one, so a round-away mode must not push it up: the
    // result is `1.00`, never `2.00`.
    let ceil = Context::new(
        core::num::NonZeroU32::new(3).unwrap(),
        999,
        -999,
        Rounding::Ceiling,
    );
    assert_eq!(pow("1", "1.01", &ceil), ("1.00".to_string(), false));
    assert_eq!(pow("1", "12.3", &ceil), ("1.00".to_string(), false));
    let up = Context::new(
        core::num::NonZeroU32::new(3).unwrap(),
        999,
        -999,
        Rounding::Up,
    );
    assert_eq!(pow("1", "1.01", &up), ("1.00".to_string(), false));
}

#[test]
fn infinite_operands() {
    let c = ctx(9);
    let cases = [
        // +Inf base by sign of y.
        ("Infinity", "-Infinity", "0", false),
        ("Infinity", "-1", "0", false),
        ("Infinity", "-0.5", "0", false),
        ("Infinity", "0.5", "Infinity", false),
        ("Infinity", "1", "Infinity", false),
        ("Infinity", "Infinity", "Infinity", false),
        // -Inf base: integer y by parity, non-integer / infinite y invalid.
        ("-Infinity", "1", "-Infinity", false),
        ("-Infinity", "2", "Infinity", false),
        ("-Infinity", "3", "-Infinity", false),
        ("-Infinity", "-1", "-0", false),
        ("-Infinity", "-2", "0", false),
        ("-Infinity", "0.5", "NaN", true),
        ("-Infinity", "Infinity", "NaN", true),
        ("-Infinity", "-Infinity", "NaN", true),
        // Finite base, infinite exponent, by magnitude versus one.
        ("0.5", "Infinity", "0", false),
        ("0.5", "-Infinity", "Infinity", false),
        ("2", "Infinity", "Infinity", false),
        ("2", "-Infinity", "0", false),
        // Negative finite base with infinite exponent is invalid.
        ("-0.5", "Infinity", "NaN", true),
        ("-2", "Infinity", "NaN", true),
    ];
    for (x, y, want, inval) in cases {
        assert_eq!(pow(x, y, &c), (want.to_string(), inval), "power({x}, {y})");
    }
}

#[test]
fn negative_base_integer_only() {
    let c = ctx(9);
    // Integer exponent: sign by parity, magnitude |x|^y.
    assert_eq!(pow("-2", "3", &c), ("-8".to_string(), false));
    assert_eq!(pow("-2", "-3", &c), ("-0.125".to_string(), false));
    assert_eq!(pow("-2", "4", &c), ("16".to_string(), false));
    assert_eq!(pow("-3", "3", &c), ("-27".to_string(), false));
    assert_eq!(pow("-10", "9", &c), ("-1.00000000E+9".to_string(), false));
    // Non-integer exponent over a negative base is Invalid.
    assert_eq!(pow("-2", "0.5", &c), ("NaN".to_string(), true));
    assert_eq!(pow("-2", "2.5", &c), ("NaN".to_string(), true));
}

#[test]
fn integer_exponent_exact_cohorts() {
    let c = ctx(9);
    // Exact where representable; rounded (Inexact) only when it exceeds the
    // precision or does not terminate.
    assert_eq!(pow("2", "3", &c).0, "8");
    assert_eq!(pow("2", "-1", &c).0, "0.5");
    assert_eq!(pow("2", "-2", &c).0, "0.25");
    assert_eq!(pow("10", "3", &c).0, "1000");
    assert_eq!(pow("1.5", "2", &c).0, "2.25");
    assert_eq!(pow("0.5", "3", &c).0, "0.125");
    assert_eq!(pow("12", "2", &c).0, "144");
    assert_eq!(pow("2", "30", &c).0, "1.07374182E+9");
    assert_eq!(pow("7", "-2", &c).0, "0.0204081633");

    // The exact ones carry no Inexact; the rounded ones do.
    let exact = Decimal::parse_str("2")
        .unwrap()
        .power(&Decimal::parse_str("3").unwrap(), &c);
    assert!(!exact.1.inexact());
    let rounded = Decimal::parse_str("2")
        .unwrap()
        .power(&Decimal::parse_str("30").unwrap(), &c);
    assert!(rounded.1.inexact());
}
