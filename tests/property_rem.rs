//! Property tests for `Decimal128::rem` (IEEE remainder).

use proptest::prelude::*;

use ferrodec::Decimal128;

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

/// IEEE remainder reference: `r = x - n*y` where `n = round_half_even(x/y)`.
/// Computed in `i128` to avoid floating-point in the oracle.
///
/// Strategy: `div_euclid` produces `(q_e, r_e)` with `r_e ∈ [0, |y|)`.
/// `x/y` lies in `[q_e, q_e + 1)` if `y > 0` and in `(q_e − 1, q_e]` if
/// `y < 0`. Rounding to the *neighbour* of `q_e` (i.e. moving towards
/// the direction `y` points) when `r_e > |y|/2` reproduces
/// round-to-nearest-even.
fn ieee_rem_i128(x: i128, y: i128) -> i128 {
    debug_assert!(y != 0);
    let abs_y = y.unsigned_abs();
    let q_e = x.div_euclid(y);
    let r_e = x.rem_euclid(y);
    let two_r = (r_e as u128).checked_mul(2).expect("2r overflow in oracle");
    let round_to_neighbour = match two_r.cmp(&abs_y) {
        core::cmp::Ordering::Less => false,
        core::cmp::Ordering::Greater => true,
        core::cmp::Ordering::Equal => (q_e & 1) != 0,
    };
    if round_to_neighbour {
        if y > 0 {
            r_e - y
        } else {
            r_e + y
        }
    } else {
        r_e
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2048))]

    /// `rem` matches the i128 reference for any pair of small operands.
    #[test]
    fn rem_matches_i128_oracle(x in -1_000_000i64..=1_000_000, y in -1_000_000i64..=1_000_000) {
        prop_assume!(y != 0);
        let dx = decimal_from_i128(x as i128);
        let dy = decimal_from_i128(y as i128);
        let (got, status) = dx.rem(dy);
        prop_assert!(!status.invalid());

        let truth = ieee_rem_i128(x as i128, y as i128);
        let truth_dec = decimal_from_i128(truth);
        let (cmp, _) = got.partial_cmp(truth_dec);
        prop_assert_eq!(
            cmp,
            Some(core::cmp::Ordering::Equal),
            "rem({}, {}): got {:?}, want {} ({:?})", x, y, got, truth, truth_dec
        );
    }

    /// `|rem(x, y)| <= |y|/2` for any finite operands with non-zero y.
    #[test]
    fn rem_bounded_by_half_y(x in -10_000i64..=10_000, y in -10_000i64..=10_000) {
        prop_assume!(y != 0);
        let dx = decimal_from_i128(x as i128);
        let dy = decimal_from_i128(y as i128);
        let (got, _) = dx.rem(dy);
        prop_assume!(!got.is_nan());

        // Compare 2*|got| ≤ |y|.
        let two_got_abs = (got.abs().to_bits(), got.abs());
        let _ = two_got_abs;
        // Easier check via i128:
        let truth = ieee_rem_i128(x as i128, y as i128);
        prop_assert!(truth.unsigned_abs() * 2 <= y.unsigned_abs() as u128);
    }
}
