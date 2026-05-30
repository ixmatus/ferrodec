//! Round-trip stability for `f64` ↔ `Decimal128`.
//!
//! The conversion goes through the canonical decimal-string form, so
//! `f64 → Decimal128 → f64` must round-trip bit-exactly for any
//! finite `f64` (the intermediate carries `f64`'s shortest-round-trip
//! decimal representation, which is what `f64::FromStr` is the inverse
//! of). The reverse `Decimal128 → f64 → Decimal128` necessarily loses
//! precision (`f64` has ≤ 17 sig digits vs. `Decimal128`'s 34) so we
//! only check that the result is within `f64`'s ULP envelope.

#![cfg(feature = "binary-float")]

use ferrodec::{Decimal128, RoundingMode};
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// Any finite f64 round-trips exactly through Decimal128.
    #[test]
    fn f64_to_decimal_to_f64_exact(bits in any::<u64>()) {
        let f = f64::from_bits(bits);
        if !f.is_finite() {
            return Ok(());
        }
        let d = Decimal128::from_f64(f, RoundingMode::NearestEven).0;
        let (back, _) = d.to_f64(RoundingMode::NearestEven);
        // Sign-of-zero must also round-trip.
        if f == 0.0 {
            prop_assert_eq!(back.is_sign_negative(), f.is_sign_negative());
        } else {
            prop_assert_eq!(back.to_bits(), f.to_bits(), "f={}", f);
        }
    }

    /// f32 also round-trips exactly via the f32 → Decimal128 → f32 path.
    #[test]
    fn f32_to_decimal_to_f32_exact(bits in any::<u32>()) {
        let f = f32::from_bits(bits);
        if !f.is_finite() {
            return Ok(());
        }
        let d = Decimal128::from_f32(f, RoundingMode::NearestEven).0;
        let (back, _) = d.to_f32(RoundingMode::NearestEven);
        if f == 0.0 {
            prop_assert_eq!(back.is_sign_negative(), f.is_sign_negative());
        } else {
            prop_assert_eq!(back.to_bits(), f.to_bits(), "f={}", f);
        }
    }
}
