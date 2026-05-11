//! Property tests for `Decimal128::sqrt`.

use proptest::prelude::*;

use ferrodec::{Decimal128, RoundingMode};

mod common;
use common::within_ulps;

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

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1024))]

    /// For perfect squares of `u32`-sized integers, sqrt should be exact and
    /// `sqrt(n)^2 == n`.
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

    /// `sqrt(x)^2 ≈ x` within rounding for any positive `x` (we accept up
    /// to 2 ULP of relative error since we round twice — once in sqrt,
    /// once in the squaring).
    ///
    /// Pre-1.15 the body only checked `is_finite() && !is_zero() &&
    /// !is_sign_negative()` — a `sqrt` that returned `0.5 * x` would
    /// pass. Slice F's M-T6 finding from the 2026-05-10 review
    /// rewired this through `within_ulps`.
    #[test]
    fn sqrt_roundtrip_within_2_ulp(n in 1u64..u64::MAX) {
        let x = dec_from_u64(n);
        let (s, _) = x.sqrt(RoundingMode::NearestEven);
        let (back, _) = s.mul(s, RoundingMode::NearestEven);
        prop_assert!(
            within_ulps(back, x, 2),
            "sqrt({x}).pow(2) = {back}, expected within 2 ULP of {x}"
        );
    }

    /// Monotonicity: x < y (positive finite) ⇒ sqrt(x) < sqrt(y).
    #[test]
    fn sqrt_monotone(a in 1u64..u64::MAX, b in 1u64..u64::MAX) {
        prop_assume!(a != b);
        let (lo_n, hi_n) = if a < b { (a, b) } else { (b, a) };
        let lo = dec_from_u64(lo_n);
        let hi = dec_from_u64(hi_n);
        let (lo_s, _) = lo.sqrt(RoundingMode::NearestEven);
        let (hi_s, _) = hi.sqrt(RoundingMode::NearestEven);
        let (cmp, _) = lo_s.partial_cmp(hi_s);
        // Allow Equal in case both round to the same Decimal128.
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
