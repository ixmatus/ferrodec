//! Property tests for `Decimal128::mul`.
//!
//! Same three-tier setup as `property_addsub.rs`:
//!
//! 1. Algebraic identities (commutativity, identity, annihilator,
//!    sign rule, `(−a) × b == −(a × b)`).
//! 2. `i128` integer oracle: for `|a|, |b| ≤ 2^31`, the product of two
//!    `i64` operands fits in `i128`.
//! 3. astro-float oracle (TODO — same caveat as the addsub harness).

use proptest::prelude::*;

use ferrodec::{Decimal128, RoundingMode};

const MODES: &[RoundingMode] = &[
    RoundingMode::NearestEven,
    RoundingMode::NearestAway,
    RoundingMode::TowardZero,
    RoundingMode::TowardPositive,
    RoundingMode::TowardNegative,
];

const BIAS_U32: u32 = 6176;

fn decimal_finite(sign: bool, biased_exp: u32, coef: u128) -> Decimal128 {
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

fn decimal_from_i128(n: i128) -> Decimal128 {
    if n == 0 {
        return Decimal128::ZERO;
    }
    let sign = n < 0;
    let abs = n.unsigned_abs();
    debug_assert!(abs < 1u128 << 113);
    decimal_finite(sign, BIAS_U32, abs)
}

fn dec_from_i64(n: i64) -> Decimal128 {
    decimal_from_i128(n as i128)
}

fn dec_from_i32(n: i32) -> Decimal128 {
    decimal_from_i128(n as i128)
}

fn small_finite() -> impl Strategy<Value = Decimal128> {
    (any::<i64>()).prop_map(dec_from_i64)
}

fn arbitrary_finite() -> impl Strategy<Value = Decimal128> {
    (
        any::<bool>(),
        prop_oneof![
            0u32..=64u32,
            (BIAS_U32 - 100)..=(BIAS_U32 + 100),
            (12287u32 - 64)..=12287u32,
        ],
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

    /// `1 × a` is numerically equal to `a` for any finite `a`.
    #[test]
    fn one_is_identity(a in small_finite(), rm_idx in 0u8..5) {
        prop_assume!(!a.is_nan());
        let rm = MODES[rm_idx as usize];
        let (r, _) = Decimal128::ONE.mul(a, rm);
        let (cmp, _) = r.partial_cmp(a);
        prop_assert_eq!(cmp, Some(core::cmp::Ordering::Equal));
    }

    /// `0 × a` is `±0` for any finite, non-NaN, non-Inf `a`.
    #[test]
    fn zero_annihilator(a in small_finite(), rm_idx in 0u8..5) {
        prop_assume!(a.is_finite() && !a.is_nan() && !a.is_zero());
        let rm = MODES[rm_idx as usize];
        let (r, _) = Decimal128::ZERO.mul(a, rm);
        prop_assert!(r.is_zero());
    }

    /// `(−a) × b == −(a × b)` numerically, *for sign-symmetric rounding
    /// modes* (NearestEven, NearestAway, TowardZero).
    ///
    /// TowardPositive and TowardNegative are deliberately sign-asymmetric
    /// — flipping the sign of the result changes which side of an
    /// inexact tie the rounding lands on, so this identity legitimately
    /// fails by up to 1 ULP under those modes.
    #[test]
    fn neg_pulls_through_sign_symmetric(
        a in small_finite(),
        b in small_finite(),
        rm_idx in 0u8..3,  // NearestEven / NearestAway / TowardZero
    ) {
        prop_assume!(!a.is_nan() && !b.is_nan());
        let rm = MODES[rm_idx as usize];
        let (left, _) = a.neg().mul(b, rm);
        let (right_inner, _) = a.mul(b, rm);
        let right = right_inner.neg();
        let (cmp, _) = left.partial_cmp(right);
        prop_assert_eq!(cmp, Some(core::cmp::Ordering::Equal));
    }

    /// Sign rule: `sign(a × b) == sign(a) ⊕ sign(b)` for non-zero, non-NaN
    /// finite operands. Captures the IEEE 754 sign convention.
    #[test]
    fn sign_xor_rule(a in small_finite(), b in small_finite(), rm_idx in 0u8..5) {
        prop_assume!(a.is_finite() && b.is_finite());
        prop_assume!(!a.is_nan() && !b.is_nan());
        prop_assume!(!a.is_zero() && !b.is_zero());
        let rm = MODES[rm_idx as usize];
        let (r, _) = a.mul(b, rm);
        // r could be zero only if rounding produces exact zero, which
        // requires under-flow — ignore that corner.
        prop_assume!(!r.is_zero());
        prop_assert_eq!(r.is_sign_negative(), a.is_sign_negative() ^ b.is_sign_negative());
    }
}

// ---------------------------------------------------------------------------
// Integer oracle

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2048))]

    /// `mul` agrees with `i128` arithmetic for any pair of operands that
    /// fit in `i32` (so their product fits in `i64`, far below the
    /// 34-digit precision boundary, eliminating the rounding question).
    #[test]
    fn mul_matches_i128_oracle(a in any::<i32>(), b in any::<i32>(), rm_idx in 0u8..5) {
        let da = dec_from_i32(a);
        let db = dec_from_i32(b);
        let rm = MODES[rm_idx as usize];

        let (product, status) = da.mul(db, rm);
        prop_assert!(!status.invalid());
        prop_assert!(!status.overflow());
        prop_assert!(!status.underflow());

        let truth: i128 = a as i128 * b as i128;
        let truth_dec = decimal_from_i128(truth);
        let (cmp, _) = product.partial_cmp(truth_dec);
        prop_assert_eq!(
            cmp,
            Some(core::cmp::Ordering::Equal),
            "got {:?}, want {:?}, a={}, b={}",
            product,
            truth_dec,
            a,
            b
        );
    }
}

// ---------------------------------------------------------------------------
// Wide-domain identities

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2048))]

    /// Commutativity over the full finite domain. Bit-equal because the
    /// rounding step is deterministic given the same coefficient product
    /// and quantum sum.
    #[test]
    fn mul_commutative_wide(a in arbitrary_finite(), b in arbitrary_finite(), rm_idx in 0u8..5) {
        let rm = MODES[rm_idx as usize];
        let (ab, _) = a.mul(b, rm);
        let (ba, _) = b.mul(a, rm);
        prop_assert_eq!(ab.to_bits(), ba.to_bits(), "a={:?} b={:?} rm={:?}", a, b, rm);
    }
}
