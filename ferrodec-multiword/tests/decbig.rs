//! Property tests for `DecBig`, cross-checked against `u128` arithmetic
//! where the operands fit and against algebraic identities (division
//! reconstruction, scale round-trips, square-root floor) for operands wider
//! than `u128`. Requires the `alloc` feature; compiles to nothing without it.

#![cfg(feature = "alloc")]

use core::cmp::Ordering;
use ferrodec_multiword::DecBig;
use proptest::prelude::*;

fn db(x: u128) -> DecBig {
    DecBig::from_u128(x)
}

proptest! {
    #[test]
    fn prop_add_matches_u128(a in 0u128..=u128::MAX / 2, b in 0u128..=u128::MAX / 2) {
        prop_assert_eq!(db(a).add(&db(b)).to_u128(), Some(a + b));
    }

    #[test]
    fn prop_sub_matches_u128(a in 0u128..=u128::MAX, b in 0u128..=u128::MAX) {
        let (hi, lo) = if a >= b { (a, b) } else { (b, a) };
        prop_assert_eq!(db(hi).sub(&db(lo)).to_u128(), Some(hi - lo));
    }

    #[test]
    fn prop_mul_matches_u128(a: u64, b: u64) {
        // Products of two u64 fit in u128.
        let prod = u128::from(a) * u128::from(b);
        prop_assert_eq!(db(u128::from(a)).mul(&db(u128::from(b))).to_u128(), Some(prod));
    }

    #[test]
    fn prop_cmp_matches_u128(a in 0u128..=u128::MAX, b in 0u128..=u128::MAX) {
        prop_assert_eq!(db(a).cmp_ref(&db(b)), a.cmp(&b));
    }

    #[test]
    fn prop_div_rem_matches_u128(a in 0u128..=u128::MAX, b in 1u128..=u128::MAX) {
        let (q, r) = db(a).div_rem(&db(b));
        prop_assert_eq!(q.to_u128(), Some(a / b));
        prop_assert_eq!(r.to_u128(), Some(a % b));
    }

    #[test]
    fn prop_div_rem_reconstructs(a in 0u128..=u128::MAX, b in 1u128..=u128::MAX) {
        // q*b + r == a and r < b, for operands widened beyond u128.
        let big_a = db(a).mul_pow10(15);
        let big_b = db(b);
        let (q, r) = big_a.div_rem(&big_b);
        prop_assert_eq!(q.mul(&big_b).add(&r), big_a.clone());
        prop_assert_eq!(r.cmp_ref(&big_b), Ordering::Less);
    }

    #[test]
    fn prop_mul_pow10_div_rem_pow10_roundtrip(a in 0u128..=u128::MAX, k in 0u32..60) {
        // (a * 10^k) split at 10^k recovers a with zero remainder.
        let scaled = db(a).mul_pow10(k);
        let (q, r) = scaled.div_rem_pow10(k);
        prop_assert_eq!(q, db(a));
        prop_assert!(r.is_zero());
    }

    #[test]
    fn prop_div_rem_pow10_matches_u128(a in 0u128..=u128::MAX, k in 0u32..30) {
        let pow = 10u128.pow(k);
        let (q, r) = db(a).div_rem_pow10(k);
        prop_assert_eq!(q.to_u128(), Some(a / pow));
        prop_assert_eq!(r.to_u128(), Some(a % pow));
    }

    #[test]
    fn prop_isqrt_floor(a in 0u128..=u128::MAX) {
        let (s, r) = db(a).isqrt();
        let s = s.to_u128().unwrap();
        // s^2 <= a < (s+1)^2, checked via the returned remainder.
        prop_assert_eq!(r.to_u128(), Some(a - s * s));
        // (s+1)^2 can overflow u128 only when s is enormous; guard it.
        if let Some(next) = (s + 1).checked_mul(s + 1) {
            prop_assert!(a < next);
        }
    }

    #[test]
    fn prop_large_mul_roundtrips(
        xa in prop::collection::vec(0u8..=9u8, 290..420),
        xb in prop::collection::vec(0u8..=9u8, 290..420),
    ) {
        // 290..420 digits is 33..47 limbs, past KARATSUBA_THRESHOLD, so `mul`
        // takes the Karatsuba path. `div_rem` is independent of `mul`, so
        // recovering the factor from the product cross-checks Karatsuba on
        // operands far wider than the u128 oracle reaches.
        let to_decbig = |v: &[u8]| {
            let ascii: Vec<u8> = v.iter().map(|d| d + b'0').collect();
            DecBig::from_ascii_digits(&ascii)
        };
        let a = to_decbig(&xa);
        let b = to_decbig(&xb);
        prop_assume!(!a.is_zero() && !b.is_zero());
        let product = a.mul(&b);
        let (q, r) = product.div_rem(&a);
        prop_assert_eq!(q, b);
        prop_assert!(r.is_zero());
    }

    #[test]
    fn prop_digit_count_matches_string(a in 0u128..=u128::MAX) {
        let expected = a.to_string().len() as u64;
        prop_assert_eq!(db(a).decimal_digit_count(), expected);
    }

    #[test]
    fn prop_ascii_digits_and_display_roundtrip(a in 0u128..=u128::MAX) {
        let s = a.to_string();
        // from_ascii_digits parses the canonical decimal string back to a.
        prop_assert_eq!(DecBig::from_ascii_digits(s.as_bytes()), db(a));
        // Display renders the canonical decimal string.
        prop_assert_eq!(db(a).to_string(), s);
    }
}
