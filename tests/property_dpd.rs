#![cfg(all(feature = "dpd", feature = "fmt"))]
//! Property tests for the DPD interchange surface
//! (`Decimal128::to_dpd_bytes` / `from_dpd_bytes`).
//!
//! Three invariants:
//!
//! 1. **Canonical round-trip**: every `Decimal128` constructed from a
//!    parsed string round-trips bit-equal through DPD. Parsed strings
//!    cover all canonical encodings exhaustively over the proptest
//!    sample space (every 34-digit coefficient + every representable
//!    exponent + every cohort).
//!
//! 2. **Totality + canonicalization fixpoint**: every 128-bit pattern
//!    decodes via `from_dpd_bytes` without panicking. The result, once
//!    re-encoded, decodes again to the same value — so DPD-side
//!    canonicalization is a projection (idempotent under
//!    decode→encode→decode).
//!
//! 3. **Cohort preservation**: round-tripping a finite value through
//!    DPD preserves its quantum exponent, not just its numerical
//!    value. `1.0` and `1.00` are distinct in IEEE 754; they must stay
//!    distinct through the codec.

use ferrodec::{Decimal128, RoundingMode};
use proptest::prelude::*;

/// Generate a finite Decimal128 from a parsed string. The pattern
/// covers the canonical surface: any coefficient up to 34 digits, any
/// representable exponent, both signs, with optional decimal-point
/// scientific notation.
fn finite_string() -> impl Strategy<Value = String> {
    "[-+]?[0-9]{1,34}(\\.[0-9]{1,6})?(E[-+]?[0-9]{1,4})?"
}

proptest! {
    /// Every canonical finite value round-trips bit-equal.
    #[test]
    fn finite_string_roundtrip(s in finite_string()) {
        let parsed = Decimal128::parse_str(&s, RoundingMode::NearestEven);
        prop_assume!(parsed.is_ok());
        let (d, _status) = parsed.unwrap();
        prop_assume!(d.is_finite());
        let bytes = d.to_dpd_bytes();
        let recovered = Decimal128::from_dpd_bytes(bytes);
        prop_assert_eq!(
            recovered.to_bits(),
            d.to_bits(),
            "round-trip mismatch for input {:?}", s,
        );
    }

    /// Cohort preservation: round-tripping `x` through DPD leaves the
    /// quantum unchanged (`x.same_quantum(recovered)`). Catches subtle
    /// bugs where the encoder silently normalises trailing zeros.
    #[test]
    fn cohort_preservation(s in finite_string()) {
        let parsed = Decimal128::parse_str(&s, RoundingMode::NearestEven);
        prop_assume!(parsed.is_ok());
        let (d, _status) = parsed.unwrap();
        prop_assume!(d.is_finite());
        let recovered = Decimal128::from_dpd_bytes(d.to_dpd_bytes());
        prop_assert!(d.same_quantum(recovered), "quantum drift detected");
    }

    /// `from_dpd_bytes` is total on `[u8; 16]`. No input panics; every
    /// output is a `Decimal128` whose canonical-class is well-defined.
    #[test]
    fn from_dpd_bytes_total(bits in any::<u128>()) {
        let d = Decimal128::from_dpd_bytes(bits.to_be_bytes());
        // The class predicate must match exactly one of the disjoint
        // categories — `is_nan`, `is_infinite`, `is_zero`, or
        // `is_finite` — and `is_finite` excludes NaN/Inf by definition.
        let categories = [d.is_nan(), d.is_infinite(), d.is_finite()];
        let active: usize = categories.iter().map(|&b| usize::from(b)).sum();
        prop_assert_eq!(active, 1, "exactly one of NaN/Inf/Finite must hold");
    }

    /// DPD-side canonicalization is a projection: decode → encode →
    /// decode is bit-equal to the first decode. Equivalently,
    /// re-encoding a value already obtained via `from_dpd_bytes`
    /// produces a DPD pattern that decodes to the same value.
    ///
    /// This is the property check that gives `dqCanonical.decTest`
    /// its semantic content for our codec — non-canonical input
    /// declets share a value with the canonical declet, and the
    /// encoder always emits the canonical form.
    #[test]
    fn decode_encode_decode_idempotent(bits in any::<u128>()) {
        let first = Decimal128::from_dpd_bytes(bits.to_be_bytes());
        let canonical_bytes = first.to_dpd_bytes();
        let second = Decimal128::from_dpd_bytes(canonical_bytes);
        prop_assert_eq!(
            first.to_bits(),
            second.to_bits(),
            "canonicalization is not idempotent",
        );
    }

    /// Sanity: the BID `from_bits` / `to_bits` round-trip is bit-exact
    /// (existing invariant), and the DPD round-trip preserves it for
    /// canonical inputs. This is the cross-encoding analog of
    /// `from_bits` ∘ `to_bits` = id.
    #[test]
    fn finite_bid_dpd_bid_identity_via_construction(
        coef in 0u128..10u128.pow(34),
        biased_exp in 0u32..=12287,
        sign in any::<bool>(),
    ) {
        // Construct a Decimal128 directly from `try_new_unsigned`
        // when possible, falling back to `from_bits`. We need the
        // value to be canonical (coef < 10^34, biased_exp valid),
        // which the strategy guarantees.
        let exp = biased_exp as i32 - 6176;
        let parsed = Decimal128::try_new_unsigned(coef, exp);
        prop_assume!(parsed.is_ok());
        let positive = parsed.unwrap();
        let d = if sign {
            // Negate by flipping the sign bit directly — there's no
            // public negate method that doesn't go through ops.
            Decimal128::from_bits(positive.to_bits() | (1u128 << 127))
        } else {
            positive
        };
        let recovered = Decimal128::from_dpd_bytes(d.to_dpd_bytes());
        prop_assert_eq!(recovered.to_bits(), d.to_bits());
    }
}
