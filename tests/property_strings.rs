//! Property tests for the `&str` parser and `Display` formatter.
//!
//! Two roundtrip directions:
//!
//! 1. `Decimal128 → format → parse → Decimal128` should yield a value
//!    numerically equal to the original (cohort/bit pattern may shift
//!    if the formatter normalises trailing zeros, but we don't, so for
//!    most inputs the round-trip is bit-equal).
//! 2. `parse(s) → format → parse` should produce the same `Decimal128`
//!    twice over.

use proptest::prelude::*;

use ferrodec::{Decimal128, RoundingMode};

const BIAS_U32: u32 = 6176;

fn decimal_finite(sign: bool, biased_exp: u32, coef: u128) -> Decimal128 {
    debug_assert!(coef < 1u128 << 113);
    debug_assert!(biased_exp <= 12287);
    let s = (sign as u128) << 127;
    let exp_high2 = ((biased_exp >> 12) & 0b11) as u128;
    let coef_high3 = (coef >> 110) & 0b111;
    let type_bits = (exp_high2 << 3) | coef_high3;
    let ec = (biased_exp & 0xFFF) as u128;
    let t = coef & ((1u128 << 110) - 1);
    let bits = s | (type_bits << 122) | (ec << 110) | t;
    Decimal128::from_bits(bits)
}

fn arbitrary_finite() -> impl Strategy<Value = Decimal128> {
    (
        any::<bool>(),
        prop_oneof![
            0u32..=64u32,
            (BIAS_U32 - 100)..=(BIAS_U32 + 100),
            (12287u32 - 64)..=12287u32,
        ],
        prop_oneof![
            1u128..=1_000,
            1u128..=10_000_000_000,
            1u128..=10u128.pow(20),
            1u128..=(10u128.pow(34) - 1),
        ],
    )
        .prop_map(|(s, e, c)| decimal_finite(s, e, c))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2048))]

    /// `format → parse` round-trips numerically for any finite operand.
    #[test]
    fn format_parse_roundtrip(d in arbitrary_finite()) {
        let s = format!("{d}");
        let (parsed, _) = Decimal128::parse_str(&s, RoundingMode::default())
            .unwrap_or_else(|_| panic!("parse {s:?}"));
        let (cmp, _) = parsed.partial_cmp(d);
        prop_assert_eq!(
            cmp,
            Some(core::cmp::Ordering::Equal),
            "{:?} -> {:?} -> {:?}", d, s, parsed
        );
    }

    /// `parse → format → parse` is idempotent: the second parse yields
    /// the same `Decimal128` as the first.
    #[test]
    fn parse_format_parse_idempotent(d in arbitrary_finite()) {
        let s1 = format!("{d}");
        let (p1, _) = Decimal128::parse_str(&s1, RoundingMode::default()).unwrap();
        let s2 = format!("{p1}");
        prop_assert_eq!(s1, s2.clone(), "format not stable: {:?} -> {:?}", p1, s2);
        let (p2, _) = Decimal128::parse_str(&s2, RoundingMode::default()).unwrap();
        prop_assert_eq!(p1.to_bits(), p2.to_bits());
    }

    /// Special tokens round-trip exactly.
    #[test]
    fn specials_roundtrip(_: u8) {
        for &(s, expected) in &[
            ("NaN", Decimal128::NAN),
            ("Infinity", Decimal128::INFINITY),
            ("-Infinity", Decimal128::NEG_INFINITY),
        ] {
            let (parsed, _) = Decimal128::parse_str(s, RoundingMode::default()).unwrap();
            // For NaN, bit-exact since payload is preserved.
            if expected.is_nan() {
                prop_assert!(parsed.is_nan());
            } else {
                prop_assert_eq!(parsed.to_bits(), expected.to_bits());
            }
            let formatted = format!("{parsed}");
            prop_assert_eq!(formatted, s);
        }
    }
}
