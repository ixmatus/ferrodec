//! Property tests for `Decimal128::div`.

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
    decimal_finite(sign, BIAS_U32, abs)
}

fn dec_from_i64(n: i64) -> Decimal128 {
    decimal_from_i128(n as i128)
}

fn small_finite_nonzero() -> impl Strategy<Value = Decimal128> {
    (any::<i64>())
        .prop_filter("non-zero", |&n| n != 0)
        .prop_map(dec_from_i64)
}

fn small_finite() -> impl Strategy<Value = Decimal128> {
    any::<i64>().prop_map(dec_from_i64)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1024))]

    /// `a / 1 = a` numerically for any non-NaN finite `a`.
    #[test]
    fn div_by_one_is_identity_numeric(a in small_finite(), rm_idx in 0u8..5) {
        prop_assume!(!a.is_nan());
        let rm = MODES[rm_idx as usize];
        let (r, _) = a.div(Decimal128::ONE, rm);
        let (cmp, _) = r.partial_cmp(a);
        prop_assert_eq!(cmp, Some(core::cmp::Ordering::Equal));
    }

    /// `a / a = 1` numerically for any finite non-zero `a`.
    #[test]
    fn div_self_is_one(a in small_finite_nonzero(), rm_idx in 0u8..5) {
        let rm = MODES[rm_idx as usize];
        let (r, _) = a.div(a, rm);
        let (cmp, _) = r.partial_cmp(Decimal128::ONE);
        prop_assert_eq!(cmp, Some(core::cmp::Ordering::Equal));
    }

    /// Sign rule: `sign(a / b) = sign(a) ⊕ sign(b)` for non-zero, finite operands.
    #[test]
    fn div_sign_rule(a in small_finite_nonzero(), b in small_finite_nonzero(), rm_idx in 0u8..5) {
        let rm = MODES[rm_idx as usize];
        let (r, s) = a.div(b, rm);
        prop_assume!(!s.invalid());
        prop_assume!(!r.is_zero()); // skip cases that round to 0
        prop_assert_eq!(r.is_sign_negative(), a.is_sign_negative() ^ b.is_sign_negative());
    }

    /// `0 / a = ±0` for any finite non-zero `a`.
    #[test]
    fn zero_over_finite_is_signed_zero(a in small_finite_nonzero(), rm_idx in 0u8..5) {
        let rm = MODES[rm_idx as usize];
        let (r, s) = Decimal128::ZERO.div(a, rm);
        prop_assert!(r.is_zero());
        prop_assert!(s.is_ok());
        prop_assert_eq!(r.is_sign_negative(), a.is_sign_negative()); // sign(0) ^ sign(a) = sign(a)
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2048))]

    /// `(a * b) / b = a` for any pair where the product fits without overflow.
    /// Use i32 operands so a * b fits in i64 and is exactly representable.
    #[test]
    fn mul_div_inverts(a in any::<i32>(), b in any::<i32>().prop_filter("nonzero", |&n| n != 0), rm_idx in 0u8..5) {
        let da = decimal_from_i128(a as i128);
        let db = decimal_from_i128(b as i128);
        let rm = MODES[rm_idx as usize];
        let (product, _) = da.mul(db, rm);
        let (quotient, _) = product.div(db, rm);
        let (cmp, _) = quotient.partial_cmp(da);
        prop_assert_eq!(cmp, Some(core::cmp::Ordering::Equal),
            "(({}) * ({})) / ({}) should equal ({})", a, b, b, a);
    }
}
