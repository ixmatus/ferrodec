//! Property tests for `Decimal128::sqrt`.
//!
//! The headline check is the **exact correctly-rounded oracle**: the
//! true square root is expanded to `precision + 2` digits with an
//! exact integer remainder (the root being inexact is the exact
//! sticky bit), so `sqrt(x)` is asserted bit-for-bit — cohort
//! included — with an exact status, across the full non-negative
//! finite domain and every IEEE rounding direction. This replaces the
//! former `within_ulps(2)` round-trip, which only checked
//! `sqrt(x)^2 ≈ x` and would pass a `0.5·x`-style bug within
//! tolerance. The algebraic identities (perfect square, monotonicity,
//! 0/1) are kept as independent cross-checks. See ADR-0021.

#![cfg(feature = "fmt")]

use proptest::prelude::*;

use ferrodec::{Decimal128, RoundingMode};
use ferrodec_test_support::conformance::status_conformance_eq;
use ferrodec_test_support::oracle::{self, parse_decimal, Expect, Format};

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

fn dec_from_u64(n: u64) -> Decimal128 {
    decimal_finite(false, BIAS_U32, n as u128)
}

/// Cohort-exact equality between a ferrodec result and the oracle's
/// prediction (decode `got` via cohort-faithful forced scientific).
fn result_matches(got: Decimal128, want: &Expect) -> bool {
    match want {
        Expect::Nan => got.is_nan(),
        Expect::Infinity { neg } => got.is_infinite() && got.is_sign_negative() == *neg,
        Expect::Finite { neg, coeff, exp } => {
            got.is_finite() && {
                let (n, c, e) = oracle::decode_decimal128(got.to_bits());
                n == *neg && c == *coeff && e == *exp
            }
        }
    }
}

/// Non-negative finite sample over the full exponent range.
fn nonneg_finite() -> impl Strategy<Value = Decimal128> {
    (
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
        .prop_map(|(e, c)| decimal_finite(false, e, c))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2048))]

    /// `sqrt(x)` is the exact correctly-rounded square root,
    /// bit-for-bit, across the full non-negative finite domain and
    /// every IEEE rounding direction.
    #[test]
    fn sqrt_is_exactly_correctly_rounded(x in nonneg_finite(), rm_idx in 0u8..5) {
        let rm = MODES[rm_idx as usize];
        let (got, gs) = x.sqrt(rm);
        let dx = parse_decimal(&format!("{x:e}")).expect("finite operand");
        let r = oracle::sqrt(&dx, Format::DECIMAL128, rm);
        prop_assert!(
            result_matches(got, &r.value),
            "value sqrt({x:e}) rm={rm:?}: got {got:e}, oracle {}",
            r.decimal_string()
        );
        prop_assert!(
            status_conformance_eq(gs, r.status),
            "status sqrt({x:e}) rm={rm:?}: got {gs:?}, oracle {:?}",
            r.status
        );
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1024))]

    /// For perfect squares of `u32`-sized integers, sqrt is exact and
    /// `sqrt(n^2) == n`.
    #[test]
    fn sqrt_of_perfect_square(root in any::<u32>()) {
        let n_u128 = (root as u128) * (root as u128);
        let n = decimal_finite(false, BIAS_U32, n_u128);
        let (s, _) = n.sqrt(RoundingMode::default());
        let expected = decimal_finite(false, BIAS_U32, root as u128);
        let (cmp, _) = s.partial_cmp(expected);
        prop_assert_eq!(cmp, Some(core::cmp::Ordering::Equal),
            "sqrt({}^2) ≠ {}", root, root);
    }

    /// Monotonicity: x < y (positive finite) ⇒ sqrt(x) ≤ sqrt(y).
    #[test]
    fn sqrt_monotone(a in 1u64..u64::MAX, b in 1u64..u64::MAX) {
        prop_assume!(a != b);
        let (lo_n, hi_n) = if a < b { (a, b) } else { (b, a) };
        let (lo_s, _) = dec_from_u64(lo_n).sqrt(RoundingMode::NearestEven);
        let (hi_s, _) = dec_from_u64(hi_n).sqrt(RoundingMode::NearestEven);
        let (cmp, _) = lo_s.partial_cmp(hi_s);
        prop_assert!(matches!(cmp, Some(core::cmp::Ordering::Less | core::cmp::Ordering::Equal)),
            "sqrt({}) cmp sqrt({}) = {:?}", lo_n, hi_n, cmp);
    }

    /// `sqrt(0) = 0`, `sqrt(1) = 1`.
    #[test]
    fn sqrt_zero_and_one(_: u8) {
        let (r, _) = Decimal128::ZERO.sqrt(RoundingMode::default());
        prop_assert!(r.is_zero());
        let (r, _) = Decimal128::ONE.sqrt(RoundingMode::default());
        let (cmp, _) = r.partial_cmp(Decimal128::ONE);
        prop_assert_eq!(cmp, Some(core::cmp::Ordering::Equal));
    }
}
