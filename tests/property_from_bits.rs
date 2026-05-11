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

use ferrodec::{Decimal128, IeeeClass, RoundingMode};
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

    /// `ieee_class` is total and self-consistent over the full u128
    /// space. Slice F.4 closes the M-T1 op-without-proptest gap for
    /// `ieee_class` (added in M9 of 1.14.0; previously covered only
    /// by inline unit tests).
    ///
    /// Pinned invariants:
    /// * `ieee_class(x) ∈ {SignalingNaN, QuietNaN}` iff `x.is_nan()`.
    /// * `ieee_class(x) ∈ {PositiveInfinity, NegativeInfinity}` iff
    ///   `x.is_infinite()`.
    /// * Zero / subnormal / normal classes imply `is_finite()`.
    /// * Sign-prefixed finite variants match `is_sign_negative()`.
    #[test]
    fn ieee_class_total_and_self_consistent(bits in any::<u128>()) {
        let d = Decimal128::from_bits(bits);
        let c = d.ieee_class();
        let is_class_nan = matches!(c, IeeeClass::SignalingNaN | IeeeClass::QuietNaN);
        prop_assert_eq!(is_class_nan, d.is_nan());
        let is_class_inf =
            matches!(c, IeeeClass::PositiveInfinity | IeeeClass::NegativeInfinity);
        prop_assert_eq!(is_class_inf, d.is_infinite());
        let is_class_finite = matches!(
            c,
            IeeeClass::PositiveZero
                | IeeeClass::NegativeZero
                | IeeeClass::PositiveSubnormal
                | IeeeClass::NegativeSubnormal
                | IeeeClass::PositiveNormal
                | IeeeClass::NegativeNormal
        );
        prop_assert_eq!(is_class_finite, d.is_finite());
        // Sign-aware finite classes: the IeeeClass shape collapses
        // NaN sign into the qNaN / sNaN distinction, so this check
        // is finite-only.
        if d.is_finite() {
            let class_is_negative = matches!(
                c,
                IeeeClass::NegativeZero
                    | IeeeClass::NegativeSubnormal
                    | IeeeClass::NegativeNormal
            );
            prop_assert_eq!(class_is_negative, d.is_sign_negative());
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

        /// `from_dpd_bytes(to_dpd_bytes(x))` is bit-equal to
        /// `x.canonicalize()` for every 128-bit input. The DPD codec
        /// is a projection through canonicalize: non-canonical NaN
        /// payloads (≥ 10^33) and non-canonical Form A coefficients
        /// (≥ 10^34) collapse on encode and re-emerge as the
        /// canonical form on decode.
        ///
        /// This is the M11 finding's contract: the agent flagged
        /// that bit-identity round-trip is impossible for
        /// non-canonical NaN payloads, but a *projection* identity
        /// holds and is exactly what the API documents.
        #[test]
        fn dpd_roundtrip_via_canonical(bits in any::<u128>()) {
            let d = Decimal128::from_bits(bits);
            let c = d.canonicalize();
            let r = Decimal128::from_dpd_bytes(d.to_dpd_bytes());
            // Bit-equal against the canonicalised reference: this
            // strictly subsumes "NaN-status agrees" and "numerical
            // value agrees", and it specifically catches NaN payload
            // drift between BID canonicalize and DPD encode.
            prop_assert_eq!(
                r.to_bits(),
                c.to_bits(),
                "round-trip mismatch: input bits {:#034x}, canonicalize {:#034x}, dpd round-trip {:#034x}",
                bits,
                c.to_bits(),
                r.to_bits(),
            );
        }
    }
}
