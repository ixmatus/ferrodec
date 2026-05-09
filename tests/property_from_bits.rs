#![cfg(feature = "fmt")]
//! Property tests for `Decimal128::from_bits` over the full 128-bit
//! input space.
//!
//! Motivation: the 6-agent correctness review (May 2026) found two
//! HIGH bugs reachable only through `from_bits` with non-canonical
//! Form-A coefficients (`coef ≥ 10^34`):
//!
//! * `to_dpd_bytes` panicked in debug / corrupted output in release.
//! * Arithmetic kernels treated the poisoned coefficient as real,
//!   producing results ~3.8% wrong.
//!
//! The root-cause fix lives in `bid::classify_bits` (canonicalise
//! Form A on decode, ADR-0010 commit `f0b6a16`). These tests pin the
//! contract surface so a future regression cannot reintroduce the
//! same shape: every `u128` decoded as a `Decimal128` must produce a
//! safe, IEEE-conformant value at every public method.

use ferrodec::{Decimal128, RoundingMode};
use proptest::prelude::*;

proptest! {
    /// Every 128-bit input is total: classify, predicates, and abs/neg
    /// produce well-defined results without panicking.
    #[test]
    fn from_bits_total_classify(bits in any::<u128>()) {
        let d = Decimal128::from_bits(bits);
        let categories = [d.is_nan(), d.is_infinite(), d.is_finite()];
        let active: usize = categories.iter().map(|&b| usize::from(b)).sum();
        prop_assert_eq!(active, 1, "exactly one of NaN/Inf/Finite must hold");
        // is_zero implies is_finite (zero is a finite value class).
        if d.is_zero() {
            prop_assert!(d.is_finite());
        }
        // abs / neg are total, no panic.
        let _ = d.abs();
        let _ = d.neg();
    }

    /// `is_canonical` and `canonicalize` form a fixpoint: a canonical
    /// value canonicalises to itself bit-equal; a non-canonical value
    /// canonicalises to something that *is* canonical.
    #[test]
    fn canonicalize_is_a_projection(bits in any::<u128>()) {
        let d = Decimal128::from_bits(bits);
        let c = d.canonicalize();
        prop_assert!(c.is_canonical(), "canonicalize must produce a canonical value");
        // Idempotent: canonicalising twice equals canonicalising once.
        prop_assert_eq!(
            c.canonicalize().to_bits(),
            c.to_bits(),
            "canonicalize must be idempotent",
        );
        // If d is already canonical, canonicalize is a no-op (bit-equal).
        if d.is_canonical() {
            prop_assert_eq!(
                c.to_bits(),
                d.to_bits(),
                "canonical input should pass through unchanged",
            );
        }
    }

    /// Arithmetic on a non-canonical input must agree (numerically)
    /// with arithmetic on its canonicalised form. This is the H4 fix
    /// surface: before commit f0b6a16 the kernels would treat a
    /// non-canonical Form-A coefficient as real, producing different
    /// results than the canonical form.
    #[test]
    fn add_one_agrees_with_canonicalize(bits in any::<u128>()) {
        let d = Decimal128::from_bits(bits);
        let c = d.canonicalize();
        let one = Decimal128::ONE;
        let (r_d, _) = d.add(one, RoundingMode::NearestEven);
        let (r_c, _) = c.add(one, RoundingMode::NearestEven);
        // Both NaN, or both equal numerically.
        if r_d.is_nan() {
            prop_assert!(r_c.is_nan());
        } else {
            let (cmp, _) = r_d.partial_cmp(r_c);
            prop_assert_eq!(
                cmp,
                Some(core::cmp::Ordering::Equal),
                "non-canonical input should add the same as its canonicalised form",
            );
        }
    }
}

#[cfg(feature = "dpd")]
mod dpd_total {
    use super::*;

    proptest! {
        /// `to_dpd_bytes` is total over the full input space —
        /// regression test for the H3 panic shape (non-canonical
        /// Form-A coefficient ≥ 10^34 reached the codec's
        /// `assert!(leading_digit < 10)`). The fix canonicalises on
        /// decode in `bid::classify_bits`, so this property has been
        /// safe by construction since commit f0b6a16; the test pins
        /// the contract so a future refactor can't quietly break it.
        #[test]
        fn to_dpd_bytes_total(bits in any::<u128>()) {
            let d = Decimal128::from_bits(bits);
            // Just calling it must not panic.
            let _ = d.to_dpd_bytes();
        }

        /// Round-trip through DPD always yields a value numerically
        /// equal to (or NaN-equivalent to) the canonicalised input.
        /// Not bit-equal because non-canonical inputs canonicalise
        /// before encode.
        #[test]
        fn dpd_roundtrip_via_canonical(bits in any::<u128>()) {
            let d = Decimal128::from_bits(bits);
            let c = d.canonicalize();
            let r = Decimal128::from_dpd_bytes(d.to_dpd_bytes());
            if c.is_nan() {
                prop_assert!(r.is_nan());
            } else {
                let (cmp, _) = c.partial_cmp(r);
                prop_assert_eq!(cmp, Some(core::cmp::Ordering::Equal));
            }
        }
    }
}
