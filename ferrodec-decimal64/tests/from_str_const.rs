#![cfg(feature = "fmt")]
//! Tests for `Decimal64::from_str_const` and the `dec!` macro.
//!
//! `from_str_const` is a `const` byte scanner that, on the exactly
//! representable subset, mirrors the runtime `parse_str` (same sign,
//! leading-zero, and quantum rules) with rounding removed. The property
//! test pins that equivalence; example tests cover surface forms and the
//! const-context use case; the `#[should_panic]` tests pin each rejection
//! message (the same panics become compile errors in `const` context,
//! covered by the trybuild suite).

use std::hint::black_box;

use ferrodec_decimal64::{dec, Decimal64, RoundingMode};
use proptest::prelude::*;

/// Force runtime evaluation of an otherwise const-foldable call, so a
/// rejected literal panics at run time where `#[should_panic]` can observe
/// the message. In `const` context the same panic is a compile error.
fn rt(s: &str) -> Decimal64 {
    Decimal64::from_str_const(black_box(s))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(4096))]

    /// Scientific form `<sign><coef>e<exp>` is exact by construction (the
    /// coefficient carries at most 16 digits), so the const and runtime
    /// parsers agree bit for bit and the runtime never flags inexact.
    #[test]
    fn const_parser_matches_runtime(
        negative in any::<bool>(),
        coefficient in 0u64..10u64.pow(16),
        exponent in -398i32..=369,
    ) {
        let s = format!("{}{}e{}", if negative { "-" } else { "" }, coefficient, exponent);
        let via_const = Decimal64::from_str_const(&s);
        let (via_runtime, status) =
            Decimal64::parse_str(&s, RoundingMode::NearestEven).unwrap();
        prop_assert!(!status.inexact());
        prop_assert_eq!(via_const.to_bits(), via_runtime.to_bits());
    }
}

#[test]
fn surface_forms_match_runtime() {
    for s in [
        "0",
        "-0",
        "1",
        "-1",
        "10",
        "100",
        "007",
        "0.1",
        "-0.1",
        "0.001",
        ".5",
        "5.",
        "1.2300",
        "123e-2",
        "1E3",
        "1e+3",
        "1e-3",
        "-12.34e5",
        "6.62607015e-34",
        "2.99792458e8",
        "0.00",
        "0e5",
        "-0.0",
        "9999999999999999",
    ] {
        let via_const = Decimal64::from_str_const(s);
        let (via_runtime, status) = Decimal64::parse_str(s, RoundingMode::NearestEven).unwrap();
        assert!(!status.inexact(), "{s:?} should be exact");
        assert_eq!(
            via_const.to_bits(),
            via_runtime.to_bits(),
            "mismatch on {s:?}"
        );
    }
}

#[test]
fn const_context_and_macro_agree() {
    const C_FN: Decimal64 = Decimal64::from_str_const("2.99792458e8");
    const C_MACRO: Decimal64 = dec!("2.99792458e8");
    assert_eq!(C_FN.to_bits(), C_MACRO.to_bits());
    assert_eq!(
        C_FN.to_bits(),
        Decimal64::try_new(299_792_458, 0).unwrap().to_bits()
    );
}

#[test]
#[should_panic(expected = "empty decimal literal")]
fn rejects_empty() {
    let _ = rt("");
}

#[test]
#[should_panic(expected = "sign with no digits")]
fn rejects_sign_only() {
    let _ = rt("-");
}

#[test]
#[should_panic(expected = "more than one decimal point")]
fn rejects_double_point() {
    let _ = rt("1.2.3");
}

#[test]
#[should_panic(expected = "invalid character in literal")]
fn rejects_garbage() {
    let _ = rt("12x");
}

#[test]
#[should_panic(expected = "significant figures")]
fn rejects_too_many_significant_figures() {
    // 17 digits exceeds the 16-digit coefficient.
    let _ = rt("12345678901234567");
}

#[test]
#[should_panic(expected = "exponent out of range")]
fn rejects_exponent_that_would_wrap_i16() {
    let _ = rt("1e40000");
}

#[test]
#[should_panic(expected = "malformed exponent")]
fn rejects_bare_exponent() {
    let _ = rt("1e");
}

#[test]
#[should_panic(expected = "invalid character in exponent")]
fn rejects_exponent_garbage() {
    let _ = rt("1e5x");
}
