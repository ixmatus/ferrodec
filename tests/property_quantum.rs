//! Proptest coverage for the IEEE 754 §5.3 / §5.10 quantum surface.
//!
//! Companion to the Kani harnesses in `src/verify/quantum.rs`. Kani
//! proves the special-case dispatch over symbolic inputs; proptest
//! sweeps random finites where the cohort-normalisation logic
//! exercises the multi-step `mul_pow10` / `div_pow10` paths the SMT
//! solver wouldn't terminate on.

use core::cmp::Ordering;

use ferrodec::Decimal128;
use proptest::prelude::*;

/// Build a random finite Decimal128 from a coefficient (0 … 10^34 - 1)
/// and a quantum exponent in the supported range. Skips coefficients
/// at the `COEFFICIENT_LIMIT` boundary — the `proptest::strategy::Just`
/// equivalent there isn't worth the complication.
fn arb_finite() -> impl Strategy<Value = Decimal128> {
    (0i128..(10_i128.pow(33)), -200_i32..200_i32)
        .prop_map(|(coef, exp)| Decimal128::try_new(coef, exp).expect("in range by construction"))
}

proptest! {
    /// `next_up` followed by `next_down` recovers `x` numerically.
    /// Cohort can shift (next_up renormalises to the finest cohort);
    /// numeric equality is the right invariant.
    #[test]
    fn next_down_inverts_next_up(d in arb_finite()) {
        let (up, _) = d.next_up();
        let (back, _) = up.next_down();
        // Skip when next_up went to ±∞ — the inverse goes to MAX/MIN
        // which is a valid IEEE behaviour but isn't a numeric inverse.
        prop_assume!(up.is_finite());
        let (cmp, _) = back.partial_cmp(d);
        prop_assert_eq!(
            cmp,
            Some(Ordering::Equal),
            "next_down(next_up({:?})) = {:?}",
            d,
            back
        );
    }

    /// `next_up(x) > x` numerically for any finite x where
    /// next_up doesn't saturate to ±∞.
    #[test]
    fn next_up_strictly_greater(d in arb_finite()) {
        let (up, _) = d.next_up();
        prop_assume!(up.is_finite());
        let (cmp, _) = up.partial_cmp(d);
        prop_assert_eq!(cmp, Some(Ordering::Greater));
    }

    /// `same_quantum` is reflexive over arbitrary bit patterns.
    /// Pinning this on random `u128` (not just finites) covers the
    /// NaN-vs-NaN-is-true and Inf-vs-Inf-is-true rules.
    #[test]
    fn same_quantum_reflexive_random(bits in any::<u128>()) {
        let d = Decimal128::from_bits(bits);
        prop_assert!(d.same_quantum(d));
    }

    /// `compare_total_magnitude` is reflexive over arbitrary bit
    /// patterns: |x|.total_cmp(|x|) == Equal.
    #[test]
    fn compare_total_magnitude_reflexive_random(bits in any::<u128>()) {
        let d = Decimal128::from_bits(bits);
        prop_assert_eq!(d.compare_total_magnitude(d), Ordering::Equal);
    }

    /// `logb(scaleb(1, n)) == n` for `|n| <= 6144`. Pins the
    /// scaleb / logb inverse identity — the M-T1 op-without-proptest
    /// finding from the 2026-05-10 review.
    #[test]
    fn logb_inverts_scaleb_at_one(n in -6144i32..=6144i32) {
        use ferrodec::RoundingMode;
        let one = Decimal128::try_new(1, 0).unwrap();
        let (scaled, _) = one.scaleb(n, RoundingMode::NearestEven);
        prop_assume!(scaled.is_finite() && !scaled.is_zero());
        let (back, _) = scaled.logb();
        let expected = Decimal128::try_new(i128::from(n), 0).unwrap();
        prop_assert_eq!(
            back.partial_cmp(expected).0,
            Some(Ordering::Equal),
            "logb(scaleb(1, {})) = {:?}, expected {:?}",
            n,
            back,
            expected,
        );
    }
}
