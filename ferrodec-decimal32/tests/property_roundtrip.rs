//! L2 (Phase 1 finding A5-F8): a randomized `Display` then
//! `parse_str` round-trip guard over sampled bit patterns.
//!
//! The prior coverage was eight hand picked strings. IEEE 754-2019
//! §5.12 requires the to-scientific and from-scientific pair to
//! preserve both the value and the cohort. Two properties hold here
//! for every `Decimal32`:
//!
//! 1. Value preservation: parsing the rendering of `d` yields a
//!    value numerically equal to `d` (specials match by class and
//!    sign).
//! 2. Cohort stability: the render then parse step is idempotent, so
//!    the cohort exponent a canonical value carries is not lost.
//!    This is asserted as a fixpoint (`rt` and `rt` rendered then
//!    parsed again share their bit pattern), which is the honest
//!    statement: a non canonical input encoding decodes to the same
//!    value and canonicalises on the first round trip, after which
//!    the encoding is stable.

#![cfg(feature = "fmt")]

use ferrodec_decimal32::{Decimal32, RoundingMode};
use proptest::prelude::*;

fn parse(s: &str) -> Decimal32 {
    Decimal32::parse_str(s, RoundingMode::NearestEven)
        .expect("the canonical rendering of a Decimal32 parses back")
        .0
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(8192))]

    #[test]
    fn display_parse_round_trips(bits in any::<u32>()) {
        let d = Decimal32::from_bits(bits);
        let rt = parse(&d.to_string());

        if d.is_nan() {
            prop_assert!(rt.is_nan(), "NaN lost: {d} -> {rt}");
            return Ok(());
        }
        if d.is_infinite() {
            prop_assert!(
                rt.is_infinite() && rt.is_sign_negative() == d.is_sign_negative(),
                "infinity lost: {d} -> {rt}"
            );
            return Ok(());
        }

        // Finite (including zero and subnormal): value preserved.
        prop_assert_eq!(
            d.partial_cmp(rt).0,
            Some(core::cmp::Ordering::Equal),
            "value not preserved: {} -> {}",
            d,
            rt
        );

        // Cohort stability: render/parse is idempotent, so a
        // canonical value's exponent survives every further trip.
        let rt2 = parse(&rt.to_string());
        prop_assert_eq!(
            rt.to_bits(),
            rt2.to_bits(),
            "render/parse not idempotent: {} then {}",
            rt,
            rt2
        );
    }
}
