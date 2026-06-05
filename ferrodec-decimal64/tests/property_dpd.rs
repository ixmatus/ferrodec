#![cfg(all(feature = "dpd", feature = "fmt"))]
//! Property tests for the DPD interchange surface
//! (`Decimal64::to_dpd_bytes` / `from_dpd_bytes`), mirroring the
//! decimal128 codec's proptest suite (ADR-0009).
//!
//! Invariants:
//!
//! 1. **Canonical round-trip**: every `Decimal64` parsed from a string
//!    round-trips bit-equal through DPD.
//! 2. **Cohort preservation**: the round-trip preserves the quantum
//!    exponent, so `1.0` and `1.00` stay distinct.
//! 3. **Totality**: every 64-bit pattern decodes without panicking to a
//!    well-classified value.
//! 4. **Canonicalization is a projection**: decode → encode → decode is
//!    bit-equal to the first decode (the semantic content of
//!    `ddCanonical.decTest` for the codec).

use ferrodec_decimal64::{Decimal64, RoundingMode};
use proptest::prelude::*;

/// A finite Decimal64 string: any coefficient up to 16 digits, any
/// representable exponent, both signs, optional scientific notation.
fn finite_string() -> impl Strategy<Value = String> {
    "[-+]?[0-9]{1,16}(\\.[0-9]{1,6})?(E[-+]?[0-9]{1,3})?"
}

proptest! {
    /// Every canonical finite value round-trips bit-equal.
    #[test]
    fn finite_string_roundtrip(s in finite_string()) {
        let parsed = Decimal64::parse_str(&s, RoundingMode::NearestEven);
        prop_assume!(parsed.is_ok());
        let (d, _status) = parsed.unwrap();
        prop_assume!(d.is_finite());
        let recovered = Decimal64::from_dpd_bytes(d.to_dpd_bytes());
        prop_assert_eq!(
            recovered.to_bits(),
            d.to_bits(),
            "round-trip mismatch for input {:?}", s,
        );
    }

    /// Cohort preservation: the round-trip leaves the quantum unchanged.
    #[test]
    fn cohort_preservation(s in finite_string()) {
        let parsed = Decimal64::parse_str(&s, RoundingMode::NearestEven);
        prop_assume!(parsed.is_ok());
        let (d, _status) = parsed.unwrap();
        prop_assume!(d.is_finite());
        let recovered = Decimal64::from_dpd_bytes(d.to_dpd_bytes());
        prop_assert!(d.same_quantum(recovered), "quantum drift detected");
    }

    /// `from_dpd_bytes` is total on `[u8; 8]`: exactly one of
    /// NaN / Inf / Finite holds for every input.
    #[test]
    fn from_dpd_bytes_total(bits in any::<u64>()) {
        let d = Decimal64::from_dpd_bytes(bits.to_be_bytes());
        let categories = [d.is_nan(), d.is_infinite(), d.is_finite()];
        let active: usize = categories.iter().map(|&b| usize::from(b)).sum();
        prop_assert_eq!(active, 1, "exactly one of NaN/Inf/Finite must hold");
    }

    /// DPD-side canonicalization is a projection: decode → encode →
    /// decode equals the first decode.
    #[test]
    fn decode_encode_decode_idempotent(bits in any::<u64>()) {
        let first = Decimal64::from_dpd_bytes(bits.to_be_bytes());
        let second = Decimal64::from_dpd_bytes(first.to_dpd_bytes());
        prop_assert_eq!(
            first.to_bits(),
            second.to_bits(),
            "canonicalization is not idempotent",
        );
    }

    /// Cross-encoding analog of `from_bits ∘ to_bits = id`: a value
    /// constructed canonically survives the BID → DPD → BID round-trip
    /// bit-equal.
    #[test]
    fn finite_bid_dpd_bid_identity_via_construction(
        coef in 0u64..10u64.pow(16),
        biased_exp in 0u32..=767,
        sign in any::<bool>(),
    ) {
        let exp = biased_exp as i32 - 398;
        let parsed = Decimal64::try_new_unsigned(coef, exp);
        prop_assume!(parsed.is_ok());
        let positive = parsed.unwrap();
        let d = if sign {
            Decimal64::from_bits(positive.to_bits() | (1u64 << 63))
        } else {
            positive
        };
        let recovered = Decimal64::from_dpd_bytes(d.to_dpd_bytes());
        prop_assert_eq!(recovered.to_bits(), d.to_bits());
    }
}
