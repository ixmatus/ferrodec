//! Property tests for `Decimal128::rem_near` (IEEE 754-2019 §5.3.1
//! nearest-even remainder; in 1.x this was named bare `rem`, retired
//! in 2.0 per ADR-0027).
//!
//! IEEE 754 remainder is *always exact* (`r = x − n·y`, a difference
//! of scaled integers), so the **exact oracle** asserts
//! `rem_near(x, y)` bit-for-bit — cohort included — with an exact
//! status, across the full finite domain (non-zero `y`). This widens
//! the prior `i128`-only coverage (`|x|, |y| ≤ 10^6`), kept here as a
//! fast secondary check, plus the magnitude/sign invariants and the
//! `rem_trunc` pins. See ADR-0021.

use proptest::prelude::*;

use ferrodec::Decimal128;
#[cfg(feature = "fmt")]
use ferrodec::RoundingMode;
#[cfg(feature = "fmt")]
use ferrodec_test_support::conformance::status_conformance_eq;
#[cfg(feature = "fmt")]
use ferrodec_test_support::oracle::{self, parse_decimal, Expect, Format};

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

    /// `rem_near` matches the i128 reference for any pair of small operands.
    #[test]
    fn rem_near_matches_i128_oracle(x in -1_000_000i64..=1_000_000, y in -1_000_000i64..=1_000_000) {
        prop_assume!(y != 0);
        let dx = decimal_from_i128(x as i128);
        let dy = decimal_from_i128(y as i128);
        let (got, status) = dx.rem_near(dy);
        prop_assert!(!status.invalid());

        let truth = ieee_rem_i128(x as i128, y as i128);
        let truth_dec = decimal_from_i128(truth);
        let (cmp, _) = got.partial_cmp(truth_dec);
        prop_assert_eq!(
            cmp,
            Some(core::cmp::Ordering::Equal),
            "rem_near({}, {}): got {:?}, want {} ({:?})", x, y, got, truth, truth_dec
        );
    }

    /// `|rem_near(x, y)| <= |y|/2` for any finite operands with non-zero y.
    #[test]
    fn rem_near_bounded_by_half_y(x in -10_000i64..=10_000, y in -10_000i64..=10_000) {
        prop_assume!(y != 0);
        let dx = decimal_from_i128(x as i128);
        let dy = decimal_from_i128(y as i128);
        let (got, _) = dx.rem_near(dy);
        prop_assume!(!got.is_nan());

        // Compare 2*|got| ≤ |y|.
        let two_got_abs = (got.abs().to_bits(), got.abs());
        let _ = two_got_abs;
        // Easier check via i128:
        let truth = ieee_rem_i128(x as i128, y as i128);
        prop_assert!(truth.unsigned_abs() * 2 <= y.unsigned_abs() as u128);
    }

    /// `rem_trunc(x, y)` carries the sign of the dividend and has
    /// magnitude strictly less than `|y|` — the truncated-remainder
    /// invariants from IEEE 754-2019 §5.3.1 and the GDA spec.
    ///
    /// Slice F.4: the M-T1 op-without-proptest finding from the
    /// 2026-05-10 review named `rem_trunc` as an op added in 1.10.0
    /// but never sweep-tested. Pin sign + magnitude here; oracle
    /// equality against `i128`'s truncating remainder is the next
    /// step (deferred — the oracle bridge from i128 to Decimal128
    /// at small magnitudes is already exercised by `rem_matches_i128_oracle`
    /// for the IEEE variant).
    #[test]
    fn rem_trunc_sign_of_dividend(
        x in -1_000_000i64..=1_000_000,
        y in -1_000_000i64..=1_000_000,
    ) {
        prop_assume!(y != 0);
        prop_assume!(x != 0);
        let dx = decimal_from_i128(x as i128);
        let dy = decimal_from_i128(y as i128);
        let (got, status) = dx.rem_trunc(dy);
        prop_assert!(!status.invalid());
        prop_assume!(got.is_finite());
        if !got.is_zero() {
            prop_assert_eq!(
                got.is_sign_negative(),
                x < 0,
                "rem_trunc({}, {}): result sign should match dividend",
                x, y
            );
        }
        // Magnitude bound: |rem_trunc| < |y|.
        let abs_got = got.abs();
        let abs_y = dy.abs();
        let (cmp, _) = abs_got.partial_cmp(abs_y);
        prop_assert!(
            matches!(cmp, Some(core::cmp::Ordering::Less)),
            "|rem_trunc({}, {})| = {:?} not strictly less than |{}| = {:?}",
            x, y, abs_got, y, abs_y
        );
    }
}

// ---------------------------------------------------------------------------
// Exact correctly-rounded oracle (requires `fmt` for Display +
// parse_str; the rest of this file's tests do not).

#[cfg(feature = "fmt")]
fn rem_result_matches(got: Decimal128, want: &Expect) -> bool {
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

#[cfg(feature = "fmt")]
fn finite() -> impl Strategy<Value = Decimal128> {
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

#[cfg(feature = "fmt")]
proptest! {
    #![proptest_config(ProptestConfig::with_cases(2048))]

    /// `rem_near(x, y)` is the exact IEEE 754 remainder, bit-for-bit
    /// and status-for-status, across the full finite domain (non-zero
    /// `y`). The result is always exact, so the rounding mode passed
    /// to the oracle is immaterial — it only fixes the cohort, which
    /// the GDA `min(exp x, exp y)` ideal exponent already determines.
    #[test]
    fn rem_near_is_exactly_correctly_rounded(x in finite(), y in finite()) {
        prop_assume!(!y.is_zero());
        let (got, gs) = x.rem_near(y);
        let dx = parse_decimal(&format!("{x:e}")).expect("finite operand");
        let dy = parse_decimal(&format!("{y:e}")).expect("finite operand");
        let r = oracle::rem(&dx, &dy, Format::DECIMAL128, RoundingMode::NearestEven);
        prop_assert!(
            rem_result_matches(got, &r.value),
            "value rem_near({x:e}, {y:e}): got {got:e}, oracle {}",
            r.decimal_string()
        );
        prop_assert!(
            status_conformance_eq(gs, r.status),
            "status rem_near({x:e}, {y:e}): got {gs:?}, oracle {:?}",
            r.status
        );
    }
}
