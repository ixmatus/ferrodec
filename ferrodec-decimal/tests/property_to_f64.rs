//! Round-trip stability for `f64` → `Decimal` → `f64`.
//!
//! `TryFrom<f64>` is exact (a finite `f64` is a dyadic rational, hence a finite
//! decimal), and [`Decimal::to_f64`] rounds the exact decimal back to the `f64`
//! grid round-to-nearest-even. Because the exact decimal of an `f64` is itself,
//! the nearest `f64` to it is that same `f64`, so the round trip is bit-exact
//! for every finite `f64`, signed zero included. This mirrors the Decimal128
//! `property_binary_float` shape, but with an *exact* intermediate, so the
//! invariant is equality rather than a ULP envelope.

#![cfg(feature = "binary-float")]

use ferrodec_decimal::{Decimal, RoundingMode};
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2048))]

    /// Any finite f64 round-trips bit-exactly through `Decimal`.
    #[test]
    fn f64_to_decimal_to_f64_exact(bits in any::<u64>()) {
        let f = f64::from_bits(bits);
        if !f.is_finite() {
            return Ok(());
        }
        let d = Decimal::try_from(f).unwrap();
        let (back, _) = d.to_f64(RoundingMode::NearestEven);
        if f == 0.0 {
            // Sign-of-zero must also round-trip.
            prop_assert_eq!(back.is_sign_negative(), f.is_sign_negative());
        } else {
            prop_assert_eq!(back.to_bits(), f.to_bits(), "f={}", f);
        }
    }

    /// Any finite f32 round-trips bit-exactly via the f32 → Decimal → f64 → f32
    /// path (every f32 is an exact f64, so the f64 read-out narrows back losslessly).
    #[test]
    fn f32_to_decimal_to_f32_exact(bits in any::<u32>()) {
        let f = f32::from_bits(bits);
        if !f.is_finite() {
            return Ok(());
        }
        let d = Decimal::try_from(f).unwrap();
        let back = d.to_f64(RoundingMode::NearestEven).0 as f32;
        if f == 0.0 {
            prop_assert_eq!(back.is_sign_negative(), f.is_sign_negative());
        } else {
            prop_assert_eq!(back.to_bits(), f.to_bits(), "f={}", f);
        }
    }
}
