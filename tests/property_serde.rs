//! Round-trip tests for the `serde` feature.
//!
//! The default `Serialize` / `Deserialize` route through the
//! canonical decimal string — that survives every format and stays
//! human-readable. Round-trip equivalence is *numeric*, not bitwise,
//! since `Display` does not always preserve the source cohort.
//!
//! The `serde_bid` helper module is bit-exact when the underlying
//! format can carry a u128 losslessly; we exercise it via
//! `serde_test::Token`, which is format-agnostic.

#![cfg(feature = "serde")]

use ferrodec::{Decimal128, RoundingMode};
use serde_test::{assert_de_tokens, assert_tokens, Configure, Token};

mod common;
use common::parse;

// Default representation -----------------------------------------------------

fn nums_equal(a: Decimal128, b: Decimal128) -> bool {
    if a.is_nan() {
        return b.is_nan();
    }
    matches!(a.partial_cmp(b).0, Some(core::cmp::Ordering::Equal))
}

#[test]
fn json_string_round_trip() {
    // Numeric (not bitwise) round-trip: Display chooses the cohort it
    // wants, and the parser may produce a different cohort from the
    // same numeric value. That's a property of Display + parse, not
    // serde, and is the right behaviour for a human-readable format.
    let cases = [
        Decimal128::ZERO,
        Decimal128::ONE,
        Decimal128::NEG_ONE,
        Decimal128::TEN,
        parse("1.23"),
        parse("-0.0001"),
        parse("1.234E+20"),
        Decimal128::INFINITY,
        Decimal128::NEG_INFINITY,
        Decimal128::NAN,
    ];
    for d in cases {
        let json = serde_json::to_string(&d).expect("serialize");
        let back: Decimal128 = serde_json::from_str(&json).expect("deserialize");
        assert!(
            nums_equal(d, back),
            "input {d:?} → {json} (parsed back as {back:?})"
        );
    }
}

#[test]
fn json_cohort_preserved_for_simple_inputs() {
    // For inputs whose Display-then-parse round-trip is cohort-stable
    // (the common case: small decimals like "1.23"), we get a
    // bit-exact round-trip too.
    let stable = [parse("1.23"), parse("0"), parse("-0.0001"), parse("100")];
    for d in stable {
        let json = serde_json::to_string(&d).unwrap();
        let back: Decimal128 = serde_json::from_str(&json).unwrap();
        assert_eq!(back.to_bits(), d.to_bits(), "input {d:?} → {json}");
    }
}

#[test]
fn json_serialized_form_is_decimal_string() {
    let d = parse("3.14");
    let json = serde_json::to_string(&d).unwrap();
    assert_eq!(json, "\"3.14\"");
}

#[test]
fn deserialize_rejects_garbage() {
    let bad: Result<Decimal128, _> = serde_json::from_str("\"not-a-number\"");
    assert!(bad.is_err());
}

// `serde_bid` helper module --------------------------------------------------

#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq)]
struct WithBid {
    #[serde(with = "ferrodec::serde_bid")]
    value: Decimal128,
}

#[test]
fn serde_bid_string_fallback_round_trips_via_json() {
    // A hand-written JSON string routes through the bid module's
    // human-readable path: `deserialize_any` accepts the string and
    // parses it via `Decimal128::parse_str`. (serde_bid *serialize*
    // always writes the raw u128, so JSON's own serialized form is a
    // number, not this string; this test exercises only the string
    // *input* path — fd-aqs.13 review.)
    let row = WithBid {
        value: parse("3.14"),
    };
    let from_str_json = "{\"value\":\"3.14\"}";
    let back: WithBid = serde_json::from_str(from_str_json).expect("string-form deserialize");
    assert_eq!(back.value.to_bits(), row.value.to_bits());
}

#[test]
fn serde_bid_accepts_decimal_string() {
    // The string-input fallback applies to *human-readable* formats
    // (fd-aqs.13: the bid module now branches on `is_human_readable`).
    // `.readable()` puts serde_test in human-readable mode, matching JSON
    // / YAML, so the `deserialize_any` path accepts the string token.
    let row = WithBid {
        value: parse("3.14"),
    };
    assert_de_tokens(
        &row.readable(),
        &[
            Token::Struct {
                name: "WithBid",
                len: 1,
            },
            Token::Str("value"),
            Token::Str("3.14"),
            Token::StructEnd,
        ],
    );
}

#[test]
fn serde_bid_round_trips_in_bincode() {
    // fd-aqs.13: bincode is non-self-describing, so it rejected the
    // former `deserialize_any` at runtime. With the `is_human_readable`
    // branch the bid module serializes/deserializes the raw BID u128,
    // which bincode round-trips bit-exactly (NaN / Inf included, since
    // the transport is the encoding, not the value).
    for s in ["3.14", "-2.5", "1E+100", "0", "-0", "NaN", "Infinity"] {
        let row = WithBid { value: parse(s) };
        let bytes = bincode::serialize(&row).expect("bincode serialize");
        let back: WithBid = bincode::deserialize(&bytes).expect("bincode deserialize");
        assert_eq!(
            back.value.to_bits(),
            row.value.to_bits(),
            "bincode round-trip {s}"
        );
    }
}

// `assert_tokens` and the unused-import suppression below would fight
// each other; silence them at file scope.
#[allow(dead_code)]
const _: fn(Decimal128, Decimal128) -> bool = nums_equal;
#[allow(dead_code)]
const _: RoundingMode = RoundingMode::NearestEven;
#[allow(dead_code)]
fn _silence_unused_assert_tokens() {
    let _ = assert_tokens::<WithBid>;
}
