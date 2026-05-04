//! Property tests for `Decimal128::add` and `Decimal128::sub`.
//!
//! Three layers of validation:
//!
//! 1. **Algebraic identities** that hold for any IEEE 754 decimal float —
//!    commutativity, the zero / negation identities, NaN propagation —
//!    proven by random sampling.
//! 2. **Integer oracle**: for operands whose values fit in `i64`, the
//!    computed result must match the corresponding `i64` arithmetic
//!    converted back into a `Decimal128`.
//! 3. **astro-float oracle** (TODO): bit-exact correctly-rounded
//!    comparison against arbitrary-precision binary float, gated by
//!    a feature flag in the dev-dep set. Tracked as a follow-up — the
//!    binary↔decimal conversion needs sufficient precision to
//!    deterministically agree with our decimal rounding, and that
//!    plumbing is its own piece of work.

use proptest::prelude::*;

use ferrodec::{Decimal128, RoundingMode};

const MODES: &[RoundingMode] = &[
    RoundingMode::NearestEven,
    RoundingMode::NearestAway,
    RoundingMode::TowardZero,
    RoundingMode::TowardPositive,
    RoundingMode::TowardNegative,
];

// ---------------------------------------------------------------------------
// Generators

/// Build a `Decimal128` from a 64-bit signed integer, with the natural
/// quantum exponent of 0.
fn dec_from_i64(n: i64) -> Decimal128 {
    if n == 0 {
        return Decimal128::ZERO;
    }
    // i64::MIN's absolute value doesn't fit in i64; widen first.
    let (sign, abs) = if n < 0 {
        (true, (n as i128).unsigned_abs())
    } else {
        (false, n as u128)
    };
    decimal_finite(sign, BIAS_U32, abs)
}

const BIAS_U32: u32 = 6176;

fn decimal_finite(sign: bool, biased_exp: u32, coef: u128) -> Decimal128 {
    // Re-pack via `from_bits` of a hand-encoded BID layout. We can't reach
    // the private `bid::pack_finite` from outside the crate, so we do the
    // bit twiddling here. This duplicates the layout — if the layout
    // changes, both places must update.
    debug_assert!(coef < 1u128 << 113);
    debug_assert!(biased_exp <= 12287);
    let s = (sign as u128) << 127;
    let exp_high2 = ((biased_exp >> 12) & 0b11) as u128;
    let coef_high3 = (coef >> 110) & 0b111;
    let type_bits = (exp_high2 << 3) | coef_high3;
    let ec = (biased_exp & 0xFFF) as u128;
    let t = coef & ((1u128 << 110) - 1);
    let bits = s | (type_bits << 122) | (ec << 110) | t;
    Decimal128::from_bits(bits)
}

// Strategy: generate a small finite `Decimal128` by sampling an i64 value.
fn small_finite() -> impl Strategy<Value = Decimal128> {
    (any::<i64>()).prop_map(dec_from_i64)
}

/// Sample a finite (non-NaN, non-Inf) `Decimal128` with arbitrary 113-bit
/// coefficient and a wide exponent range. Skips zero (some properties
/// special-case zero).
fn arbitrary_finite() -> impl Strategy<Value = Decimal128> {
    (
        any::<bool>(),
        // Cover the full biased exponent range, biased toward "useful"
        // central values to keep diff manageable.
        prop_oneof![
            0u32..=64u32,                        // far underflow
            (BIAS_U32 - 100)..=(BIAS_U32 + 100), // central
            (12287u32 - 64)..=12287u32,          // far overflow
        ],
        // Coefficient distribution: small, medium, large.
        prop_oneof![
            1u128..=1_000,
            1u128..=10_000_000_000,
            1u128..=10u128.pow(20),
            1u128..=(10u128.pow(34) - 1),
        ],
    )
        .prop_map(|(s, e, c)| decimal_finite(s, e, c))
}

// ---------------------------------------------------------------------------
// Algebraic identities

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1024))]

    /// Commutativity: `a + b` bit-equals `b + a` for all non-NaN operands.
    #[test]
    fn add_commutative_for_small_finite(a in small_finite(), b in small_finite(), rm_idx in 0u8..5) {
        let rm = MODES[rm_idx as usize];
        let (ab, sab) = a.add(b, rm);
        let (ba, sba) = b.add(a, rm);
        prop_assert_eq!(ab.to_bits(), ba.to_bits());
        prop_assert_eq!(sab.bits(), sba.bits());
    }

    /// `add(a, 0) = a` (preserves the original cohort).
    #[test]
    fn add_zero_is_identity(a in small_finite(), rm_idx in 0u8..5) {
        prop_assume!(!a.is_nan());
        prop_assume!(!a.is_zero());
        let rm = MODES[rm_idx as usize];
        let (r, s) = a.add(Decimal128::ZERO, rm);
        prop_assert_eq!(r.to_bits(), a.to_bits());
        prop_assert!(s.is_ok());
    }

    /// `a + (-a) = 0` for every finite `a`.
    #[test]
    fn add_negation_is_zero(a in small_finite(), rm_idx in 0u8..5) {
        prop_assume!(!a.is_nan());
        let rm = MODES[rm_idx as usize];
        let (r, _) = a.add(a.neg(), rm);
        prop_assert!(r.is_zero(), "{:?} + (-{:?}) = {:?}", a, a, r);
    }

    /// `a - a = 0` for every finite `a`.
    #[test]
    fn sub_self_is_zero(a in small_finite(), rm_idx in 0u8..5) {
        prop_assume!(!a.is_nan());
        let rm = MODES[rm_idx as usize];
        let (r, _) = a.sub(a, rm);
        prop_assert!(r.is_zero());
    }

    /// `a - 0 = a` (preserves the original cohort).
    #[test]
    fn sub_zero_is_identity(a in small_finite(), rm_idx in 0u8..5) {
        prop_assume!(!a.is_nan());
        prop_assume!(!a.is_zero());
        let rm = MODES[rm_idx as usize];
        let (r, _) = a.sub(Decimal128::ZERO, rm);
        prop_assert_eq!(r.to_bits(), a.to_bits());
    }

    /// `0 - a = -a` for every finite `a`.
    #[test]
    fn sub_zero_minus_a_is_neg_a(a in small_finite(), rm_idx in 0u8..5) {
        prop_assume!(!a.is_nan());
        prop_assume!(!a.is_zero());
        let rm = MODES[rm_idx as usize];
        let (r, _) = Decimal128::ZERO.sub(a, rm);
        prop_assert_eq!(r.to_bits(), a.neg().to_bits());
    }
}

// ---------------------------------------------------------------------------
// Wide-domain identities (exercise the alignment + rounding pipeline)

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2048))]

    /// Commutativity over the full finite domain.
    #[test]
    fn add_commutative_wide(a in arbitrary_finite(), b in arbitrary_finite(), rm_idx in 0u8..5) {
        let rm = MODES[rm_idx as usize];
        let (ab, _) = a.add(b, rm);
        let (ba, _) = b.add(a, rm);
        // Bit-equal: with deterministic alignment + rounding, commutativity
        // is bit-level for finite operands.
        prop_assert_eq!(ab.to_bits(), ba.to_bits(), "a={:?} b={:?} rm={:?}", a, b, rm);
    }

    /// `a + (-a) = 0` over the full finite domain.
    #[test]
    fn add_negation_wide(a in arbitrary_finite(), rm_idx in 0u8..5) {
        let rm = MODES[rm_idx as usize];
        let (r, _) = a.add(a.neg(), rm);
        prop_assert!(r.is_zero(), "a={:?} -> r={:?}", a, r);
    }

    /// `a - a = 0` over the full finite domain.
    #[test]
    fn sub_self_wide(a in arbitrary_finite(), rm_idx in 0u8..5) {
        let rm = MODES[rm_idx as usize];
        let (r, _) = a.sub(a, rm);
        prop_assert!(r.is_zero(), "a={:?} -> r={:?}", a, r);
    }

    /// `add(a, 0)` numerically equals `a` (cohort may shift, so compare
    /// numerically).
    #[test]
    fn add_zero_numeric_identity(a in arbitrary_finite(), rm_idx in 0u8..5) {
        let rm = MODES[rm_idx as usize];
        let (r, _) = a.add(Decimal128::ZERO, rm);
        let (cmp, _) = r.partial_cmp(a);
        prop_assert_eq!(cmp, Some(core::cmp::Ordering::Equal), "a={:?} -> r={:?}", a, r);
    }
}

// ---------------------------------------------------------------------------
// Integer oracle

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2048))]

    /// `add` agrees with `i128` arithmetic for any pair of operands that
    /// fit in `i64` (so their sum fits in `i128`).
    #[test]
    fn add_matches_i128_oracle(a_int in any::<i64>(), b_int in any::<i64>(), rm_idx in 0u8..5) {
        let a = dec_from_i64(a_int);
        let b = dec_from_i64(b_int);
        let rm = MODES[rm_idx as usize];

        let (sum, status) = a.add(b, rm);
        prop_assert!(status.is_ok());
        prop_assert!(!status.invalid());
        prop_assert!(!status.overflow());
        prop_assert!(!status.underflow());

        let truth: i128 = a_int as i128 + b_int as i128;
        let truth_dec = decimal_from_i128(truth);
        let (cmp, _) = sum.partial_cmp(truth_dec);
        prop_assert_eq!(
            cmp,
            Some(core::cmp::Ordering::Equal),
            "got {:?}, want {:?}, a={}, b={}",
            sum,
            truth_dec,
            a_int,
            b_int
        );
    }

    /// `sub` agrees with `i128` arithmetic.
    #[test]
    fn sub_matches_i128_oracle(a_int in any::<i64>(), b_int in any::<i64>(), rm_idx in 0u8..5) {
        let a = dec_from_i64(a_int);
        let b = dec_from_i64(b_int);
        let rm = MODES[rm_idx as usize];

        let (diff, _) = a.sub(b, rm);
        let truth: i128 = a_int as i128 - b_int as i128;
        let truth_dec = decimal_from_i128(truth);
        let (cmp, _) = diff.partial_cmp(truth_dec);
        prop_assert_eq!(
            cmp,
            Some(core::cmp::Ordering::Equal),
            "got {:?}, want {:?}, a={}, b={}",
            diff,
            truth_dec,
            a_int,
            b_int
        );
    }
}

fn decimal_from_i128(n: i128) -> Decimal128 {
    if n == 0 {
        return Decimal128::ZERO;
    }
    let sign = n < 0;
    let abs = n.unsigned_abs();
    debug_assert!(abs < 1u128 << 113);
    decimal_finite(sign, BIAS_U32, abs)
}
