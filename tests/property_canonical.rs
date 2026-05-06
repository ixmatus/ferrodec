//! Proptest coverage for `is_canonical` / `canonicalize`.
//!
//! These are bit-manipulation routines, also Kani-proven in
//! `src/verify/canonical.rs`. The proptest pass confirms the same
//! invariants on randomly-generated inputs across a wider sample
//! than the `#[cfg(test)] mod tests` set in `src/classify.rs`.

use ferrodec::Decimal128;
use proptest::prelude::*;

proptest! {
    /// `is_canonical(x)` ⇔ `canonicalize(x)` is bit-identical to `x`.
    /// This is the defining contract of the canonical predicate; if
    /// either side ever drifts the other catches it.
    #[test]
    fn is_canonical_iff_canonicalize_fixpoint(bits in any::<u128>()) {
        let d = Decimal128::from_bits(bits);
        let same = d.canonicalize().to_bits() == d.to_bits();
        prop_assert_eq!(d.is_canonical(), same);
    }

    /// `canonicalize` is idempotent — calling it twice returns the
    /// same bits as calling it once. The Kani version proves this for
    /// the entire `u128` domain; this run hits a thousand random
    /// patterns per cargo test invocation as a sanity check.
    #[test]
    fn canonicalize_is_idempotent(bits in any::<u128>()) {
        let d = Decimal128::from_bits(bits);
        let once = d.canonicalize();
        let twice = once.canonicalize();
        prop_assert_eq!(once.to_bits(), twice.to_bits());
    }

    /// The canonicalized result is itself canonical. Together with
    /// idempotence this pins `canonicalize` as a projection onto the
    /// canonical-encoding subset.
    #[test]
    fn canonicalize_yields_canonical(bits in any::<u128>()) {
        let d = Decimal128::from_bits(bits);
        prop_assert!(d.canonicalize().is_canonical());
    }
}
