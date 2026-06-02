//! Parsing and formatting: known General Decimal Arithmetic to-scientific
//! strings, error cases, and the parse-then-format round-trip.

#![cfg(feature = "fmt")]

use ferrodec_decimal::{Decimal, ParseDecimalError};
use ferrodec_multiword::DecBig;
use proptest::prelude::*;

fn fin(sign: bool, coeff: u128, exp: i32) -> Decimal {
    Decimal::finite(sign, DecBig::from_u128(coeff), exp)
}

#[test]
fn to_scientific_known_strings() {
    let cases: &[(Decimal, &str)] = &[
        (fin(false, 123, 0), "123"),
        (fin(false, 1230, -3), "1.230"),
        (fin(false, 50, -2), "0.50"),
        (fin(false, 123, -3), "0.123"),
        (fin(false, 123, -5), "0.00123"),
        (fin(false, 123, -8), "0.00000123"),
        (fin(false, 123, -9), "1.23E-7"),
        (fin(false, 7, 4), "7E+4"),
        (fin(false, 0, 0), "0"),
        (fin(false, 0, -2), "0.00"),
        (fin(false, 0, 2), "0E+2"),
        (fin(true, 123, -1), "-12.3"),
        (fin(true, 0, 0), "-0"),
        (Decimal::infinity(false), "Infinity"),
        (Decimal::infinity(true), "-Infinity"),
        (Decimal::quiet_nan(false, DecBig::zero()), "NaN"),
        (Decimal::quiet_nan(true, DecBig::zero()), "-NaN"),
        (
            Decimal::signaling_nan(false, DecBig::from_u32(123)),
            "sNaN123",
        ),
        (Decimal::quiet_nan(false, DecBig::from_u32(45)), "NaN45"),
    ];
    for (d, expected) in cases {
        assert_eq!(&d.to_string(), expected, "formatting {d:?}");
    }
}

#[test]
fn parse_known_strings() {
    assert_eq!(Decimal::parse_str("1.230").unwrap(), fin(false, 1230, -3));
    assert_eq!(Decimal::parse_str("0.50").unwrap(), fin(false, 50, -2));
    assert_eq!(Decimal::parse_str("+7E4").unwrap(), fin(false, 7, 4));
    assert_eq!(Decimal::parse_str("1.23E-7").unwrap(), fin(false, 123, -9));
    assert_eq!(Decimal::parse_str("-12.3").unwrap(), fin(true, 123, -1));
    assert_eq!(Decimal::parse_str(".5").unwrap(), fin(false, 5, -1));
    assert_eq!(Decimal::parse_str("1.").unwrap(), fin(false, 1, 0));
    // Case-insensitive specials with payloads.
    assert_eq!(Decimal::parse_str("inf").unwrap(), Decimal::infinity(false));
    assert_eq!(
        Decimal::parse_str("-Infinity").unwrap(),
        Decimal::infinity(true)
    );
    assert_eq!(
        Decimal::parse_str("snan99").unwrap(),
        Decimal::signaling_nan(false, DecBig::from_u32(99))
    );
}

#[test]
fn parse_error_cases() {
    assert_eq!(Decimal::parse_str(""), Err(ParseDecimalError::Empty));
    for bad in ["1.2.3", ".", "1e", "1e+", "abc", "+", "-", "1.2e3.4", "1 2"] {
        assert_eq!(
            Decimal::parse_str(bad),
            Err(ParseDecimalError::InvalidSyntax),
            "expected InvalidSyntax for {bad:?}"
        );
    }
    // An exponent far outside i32.
    assert_eq!(
        Decimal::parse_str("1E99999999999"),
        Err(ParseDecimalError::ExponentOverflow)
    );
}

proptest! {
    /// to-scientific is a faithful, reversible encoding of `(sign, coeff, exp)`:
    /// parsing the formatted string recovers the exact same value.
    #[test]
    fn finite_format_parse_roundtrip(sign: bool, coeff in 0u128..=u128::MAX, exp in -2000i32..2000) {
        let d = fin(sign, coeff, exp);
        let s = d.to_string();
        let parsed = Decimal::parse_str(&s).expect("formatted string parses");
        prop_assert_eq!(parsed, d);
    }

    /// Special values round-trip through their canonical strings.
    #[test]
    fn special_format_parse_roundtrip(sign: bool, signaling: bool, payload in 0u128..1_000_000) {
        let p = DecBig::from_u128(payload);
        let d = if signaling {
            Decimal::signaling_nan(sign, p)
        } else {
            Decimal::quiet_nan(sign, p)
        };
        prop_assert_eq!(Decimal::parse_str(&d.to_string()).unwrap(), d);

        let inf = Decimal::infinity(sign);
        prop_assert_eq!(Decimal::parse_str(&inf.to_string()).unwrap(), inf);
    }
}
