//! Exact correctly-rounded oracle for `Decimal128::fma`.
//!
//! The 6-agent correctness review (May 2026) found that FMA had only
//! decTest coverage — no property test, no Kani harness, no fuzz —
//! which let the H5 sub-ULP directional-rounding bug survive four
//! decTest cases that flagged it. ADR-0010 documents the testing
//! response; ADR-0021 replaces the 1-ULP astro-float envelope this
//! file used with the exact oracle.
//!
//! `fma(a, b, c)` must equal `round(a·b + c)` under a *single*
//! rounding. `a·b + c` of three scaled integers is itself a scaled
//! integer, so the oracle forms it exactly and the assertion is
//! bit-exact (cohort included) with an exact IEEE 754 status match,
//! across the full finite domain and every rounding direction.
//!
//! Two further independent layers stay:
//!
//! * **`mul`-then-`add` cross-check** for inputs where both stages are
//!   exact: catches "FMA disagrees with the obvious implementation".
//! * **Pinned H5 reproducers**, now asserted bit-for-bit against the
//!   exact oracle so a future refactor cannot quietly reintroduce the
//!   directional-rounding defect.

#![cfg(feature = "fmt")]

use ferrodec::{Decimal128, RoundingMode};
use ferrodec_test_support::conformance::status_conformance_eq;
use ferrodec_test_support::oracle::{self, parse_decimal, Expect, Format};
use proptest::prelude::*;
use proptest::test_runner::TestCaseError;

/// Cohort-exact equality between a ferrodec result and the oracle's
/// prediction, without round-tripping the oracle string back through
/// `parse_str` (which debug-asserts on some valid extreme-exponent
/// spellings — a verification-path fragility, not an arithmetic bug).
/// `got` is decoded via forced scientific, which is cohort-faithful.
fn result_matches(got: Decimal128, want: &Expect) -> bool {
    match want {
        Expect::Infinity { neg } => got.is_infinite() && got.is_sign_negative() == *neg,
        Expect::Finite { neg, coeff, exp } => {
            if !got.is_finite() {
                return false;
            }
            let g = parse_decimal(&format!("{got:e}")).expect("finite decode");
            g.neg == *neg && g.coeff == *coeff && g.exp == *exp
        }
    }
}

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

/// Full-domain finite sample: the three exponent bands (far underflow,
/// central, far overflow) exercise exactly the alignment-window shapes
/// the static-window FMA defect family lived in.
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

/// Bounded `(a, b, c)` for the `mul`-then-`add` cross-check, where the
/// product must fit in 34 digits with no over/underflow at either
/// stage.
fn fma_triple() -> impl Strategy<Value = (Decimal128, Decimal128, Decimal128)> {
    let pat = r"-?[0-9]{1,15}(\.[0-9]{1,15})?(E-?[0-9]{1,2})?";
    (pat, pat, pat).prop_filter_map("parse", |(a, b, c)| {
        let a = Decimal128::parse_str(&a, RoundingMode::NearestEven).ok()?.0;
        let b = Decimal128::parse_str(&b, RoundingMode::NearestEven).ok()?.0;
        let c = Decimal128::parse_str(&c, RoundingMode::NearestEven).ok()?.0;
        if !a.is_finite() || !b.is_finite() || !c.is_finite() {
            return None;
        }
        Some((a, b, c))
    })
}

/// Assert `a.fma(b, c, rm)` is *the* correctly-rounded fused result,
/// bit-for-bit, with the exact IEEE 754 status. Operands are read via
/// forced scientific (`{:e}`), which is cohort-faithful — Auto Display
/// re-quantizes in the comfortable range and would feed the oracle a
/// wrong ideal exponent.
fn assert_exact_fma(
    a: Decimal128,
    b: Decimal128,
    c: Decimal128,
    rm: RoundingMode,
) -> Result<(), TestCaseError> {
    let (got, gs) = a.fma(b, c, rm);
    let da = parse_decimal(&format!("{a:e}")).expect("finite operand");
    let db = parse_decimal(&format!("{b:e}")).expect("finite operand");
    let dc = parse_decimal(&format!("{c:e}")).expect("finite operand");
    let r = oracle::fma(&da, &db, &dc, Format::DECIMAL128, rm);
    prop_assert!(
        result_matches(got, &r.value),
        "value fma({}, {}, {}) rm={:?}: got {} ({:#034x}), want oracle {}",
        a,
        b,
        c,
        rm,
        got,
        got.to_bits(),
        r.decimal_string()
    );
    prop_assert!(
        status_conformance_eq(gs, r.status),
        "status fma({}, {}, {}) rm={:?}: got {:?}, want {:?} [oracle {}]",
        a,
        b,
        c,
        rm,
        gs,
        r.status,
        r.decimal_string()
    );
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2048))]

    /// `fma` is the exact correctly-rounded fused multiply-add,
    /// bit-for-bit, across the full finite domain and every IEEE
    /// rounding direction.
    #[test]
    fn fma_is_exactly_correctly_rounded(
        a in arbitrary_finite(),
        b in arbitrary_finite(),
        c in arbitrary_finite(),
        rm_idx in 0u8..5,
    ) {
        assert_exact_fma(a, b, c, MODES[rm_idx as usize])?;
    }
}

proptest! {
    /// `fma(a, b, c)` agrees with `mul`-then-`add` (numerically)
    /// whenever the product fits in 34 digits and no overflow /
    /// underflow occurs at either step.
    ///
    /// **Cohort note**: the two paths may legitimately produce
    /// different *cohorts* of the same value because their preferred
    /// quanta differ, so this compares numerical values via
    /// `partial_cmp`, not bit patterns (the bit-exact contract is the
    /// oracle test above).
    #[test]
    fn fma_agrees_with_mul_then_add_when_product_fits(
        (a, b, c) in fma_triple(),
    ) {
        let (prod, st_p) = a.mul(b, RoundingMode::NearestEven);
        prop_assume!(prod.is_finite() && !st_p.inexact());
        let (sum, st_s) = prod.add(c, RoundingMode::NearestEven);
        prop_assume!(sum.is_finite() && !st_s.inexact());

        let (got, _) = a.fma(b, c, RoundingMode::NearestEven);
        let (cmp, _) = got.partial_cmp(sum);
        prop_assert_eq!(
            cmp,
            Some(core::cmp::Ordering::Equal),
            "FMA must equal mul-then-add (numerically) when both stages are exact: \
             fma = {:?}, sum = {:?}",
            got,
            sum,
        );
    }
}

// Pinned regressions for the H5 sub-ULP directional-rounding shape,
// now asserted bit-for-bit against the exact oracle: this both locks
// down the 6-agent-review reproducers and cross-checks the oracle on
// the exact boundary shape the defect lived in.

/// `1e-6176` — the smallest positive subnormal.
fn min_subnormal() -> Decimal128 {
    Decimal128::parse_str("1e-6176", RoundingMode::NearestEven)
        .unwrap()
        .0
}

fn assert_h5(a: Decimal128, b: Decimal128, c: Decimal128, rm: RoundingMode, want_lit: &str) {
    let (r, s) = a.fma(b, c, rm);
    let da = parse_decimal(&format!("{a:e}")).unwrap();
    let db = parse_decimal(&format!("{b:e}")).unwrap();
    let dc = parse_decimal(&format!("{c:e}")).unwrap();
    let exp = oracle::fma(&da, &db, &dc, Format::DECIMAL128, rm);
    assert!(
        result_matches(r, &exp.value),
        "H5 {a} fma {b} {c} rm={rm:?}: got {r}, oracle {}",
        exp.decimal_string()
    );
    assert!(status_conformance_eq(s, exp.status));
    // Legibility: the correctly-rounded value is the 34-nine neighbour.
    let target = Decimal128::parse_str(want_lit, rm).unwrap().0;
    let (cmp, _) = r.partial_cmp(target);
    assert_eq!(
        cmp,
        Some(core::cmp::Ordering::Equal),
        "got {r}, want {target}"
    );
    assert!(s.inexact());
}

#[test]
fn h5_repro_one_fma_eps_neg_one_toward_positive() {
    // True value -1 + 1e-6176, just above -1. TowardPositive picks the
    // next representable >= true: -0.999…9 (34 nines).
    assert_h5(
        Decimal128::ONE,
        min_subnormal(),
        Decimal128::NEG_ONE,
        RoundingMode::TowardPositive,
        "-0.9999999999999999999999999999999999",
    );
}

#[test]
fn h5_repro_neg_one_fma_eps_one_toward_negative() {
    assert_h5(
        Decimal128::NEG_ONE,
        min_subnormal(),
        Decimal128::ONE,
        RoundingMode::TowardNegative,
        "0.9999999999999999999999999999999999",
    );
}
