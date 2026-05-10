#![cfg(feature = "serde")]
//! Round-trip and explicit-format tests for `Decimal32`'s serde
//! integration.

use ferrodec_decimal32::{serde_bid, Decimal32, RoundingMode};
use serde::{Deserialize, Serialize};

fn parse(s: &str) -> Decimal32 {
    Decimal32::parse_str(s, RoundingMode::NearestEven)
        .unwrap()
        .0
}

#[test]
fn json_round_trip_strings() {
    for s in &["1", "1.5", "-1.5", "0.001", "1.234E+10", "Infinity", "-Infinity", "NaN"] {
        let d = parse(s);
        let json = serde_json::to_string(&d).unwrap();
        let back: Decimal32 = serde_json::from_str(&json).unwrap();
        if d.is_nan() {
            assert!(back.is_nan(), "NaN round-trip lost NaN-ness for {s:?}");
        } else {
            assert_eq!(back.to_bits(), d.to_bits(), "round-trip mismatch for {s:?}");
        }
    }
}

#[test]
fn json_default_format_is_string() {
    let d = parse("1.5");
    let json = serde_json::to_string(&d).unwrap();
    assert_eq!(json, "\"1.5\"");
}

#[test]
fn serde_bid_round_trip_via_json() {
    #[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
    struct Wrapper {
        #[serde(with = "serde_bid")]
        value: Decimal32,
    }
    let w = Wrapper {
        value: parse("12.34"),
    };
    // serde_json serialises u32 as a JSON number, but JSON's
    // visit_any path on the deserializer surfaces visit_u32 / visit_u64
    // depending on the backend.
    let json = serde_json::to_string(&w).unwrap();
    let back: Wrapper = serde_json::from_str(&json).unwrap();
    assert_eq!(back, w);
}

#[test]
fn serde_bid_string_fallback() {
    #[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
    struct Wrapper {
        #[serde(with = "serde_bid")]
        value: Decimal32,
    }
    // Hand-crafted JSON with a string field — exercises the
    // visit_str fallback in serde_bid::deserialize.
    let json = r#"{"value":"3.14"}"#;
    let back: Wrapper = serde_json::from_str(json).unwrap();
    assert_eq!(back.value.to_bits(), parse("3.14").to_bits());
}
