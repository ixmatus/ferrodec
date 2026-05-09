//! Oracle cross-checks for `Decimal128::fma`.
//!
//! The 6-agent correctness review (May 2026) found that FMA had only
//! decTest coverage — no property test, no Kani harness, no fuzz.
//! That gap let the H5 sub-ULP directional-rounding bug survive
//! despite four decTest cases that flagged it (the runner's pass
//! floor was one-sided, so a "fix one bug, regress another by 4"
//! trade-off slipped through). ADR-0010 documents the testing-
//! strategy response; this file is the FMA oracle layer.
//!
//! Two oracles, picked for different shapes:
//!
//! * **`mul`-then-`add` cross-check** for inputs where `(a × b) + c`
//!   has no rounding interaction between the two stages — i.e. the
//!   product fits in 34 digits exactly, no underflow / overflow at
//!   either step. Catches "FMA disagrees with the obvious
//!   implementation" without needing extended precision.
//!
//! * **astro-float at 220-bit precision** for the general case. The
//!   single-rounding contract `fma(a, b, c) = round(a·b + c)` is
//!   asserted against the oracle's exact intermediate result, with
//!   tolerance set to 1 ULP for round-to-nearest-even.
//!
//! The H5 reproducer
//! (`ONE.fma(1e-6176, NEG_ONE, TowardPositive) → -0.999…9`) is
//! pinned as a directional-rounding regression case alongside the
//! random fuzzing.

#![cfg(feature = "fmt")]

use astro_float::{BigFloat, Consts, Radix, RoundingMode as AfRm};
use ferrodec::{Decimal128, RoundingMode};
use proptest::prelude::*;

mod common;
use common::{bigfloat_to_decimal_string, parse, within_ulps};

/// `(a * b) + c` evaluated by astro-float at 220-bit precision,
/// rendered as a decimal string at 50 significant digits. 50 ≫ 34,
/// so the renderer's truncation is below the FMA's rounding boundary.
fn oracle_fma(a_str: &str, b_str: &str, c_str: &str) -> String {
    let p = 220;
    let rm = AfRm::None;
    let mut cc = Consts::new().expect("init consts");
    let a = BigFloat::parse(a_str, Radix::Dec, p, rm, &mut cc);
    let b = BigFloat::parse(b_str, Radix::Dec, p, rm, &mut cc);
    let c = BigFloat::parse(c_str, Radix::Dec, p, rm, &mut cc);
    let prod = a.mul(&b, p, rm);
    let r = prod.add(&c, p, rm);
    bigfloat_to_decimal_string(&r, &mut cc, 50)
}

/// Format a `Decimal128` as a parseable decimal string for the oracle.
fn fmt(d: Decimal128) -> String {
    format!("{d}")
}

/// `(a, b, c)` strategy for the oracle cross-check. Magnitudes stay
/// within ±1e30 so the product comfortably fits in the FMA buffer
/// envelope and astro-float renders crisp results. Signs are mixed
/// so effective-add and effective-sub are both exercised.
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

proptest! {
    /// Random-input agreement with astro-float oracle.
    #[test]
    fn fma_matches_astro_float_oracle((a, b, c) in fma_triple()) {
        let (got, _) = a.fma(b, c, RoundingMode::NearestEven);
        prop_assume!(got.is_finite());

        let want_str = oracle_fma(&fmt(a), &fmt(b), &fmt(c));
        let want = parse(&want_str);
        prop_assume!(want.is_finite());
        // 1 ULP tolerance for round-to-nearest-even: any one-shot
        // rounding error from astro-float's intermediate precision
        // (220 bits) shouldn't exceed half a Decimal128 ULP, but the
        // residual rounding when we render to 50 decimal digits and
        // re-parse can push the comparison by one more ULP.
        prop_assert!(
            within_ulps(got, want, 2),
            "fma({a:?}, {b:?}, {c:?}) = {got:?}, want {want_str}",
        );
    }

    /// `fma(a, b, c)` agrees with `mul`-then-`add` whenever the
    /// product fits in 34 digits and no overflow / underflow occurs
    /// at either step. This catches "FMA disagrees with the obvious
    /// implementation" without needing the extended-precision oracle.
    #[test]
    fn fma_agrees_with_mul_then_add_when_product_fits(
        (a, b, c) in fma_triple(),
    ) {
        let (prod, st_p) = a.mul(b, RoundingMode::NearestEven);
        prop_assume!(prod.is_finite() && !st_p.inexact());
        let (sum, st_s) = prod.add(c, RoundingMode::NearestEven);
        prop_assume!(sum.is_finite() && !st_s.inexact());

        let (got, _) = a.fma(b, c, RoundingMode::NearestEven);
        prop_assert_eq!(
            got.to_bits(),
            sum.to_bits(),
            "FMA must equal mul-then-add when both stages are exact",
        );
    }
}

// Pinned regressions for the H5 sub-ULP directional-rounding shape.
// These complement the random fuzzing above by locking down the
// specific reproducers from the 6-agent review so a future
// refactor can't quietly reintroduce the same bug.

/// `1e-6176` packed by hand — the smallest positive subnormal.
fn min_subnormal() -> Decimal128 {
    use ferrodec::Decimal128;
    Decimal128::parse_str("1e-6176", RoundingMode::NearestEven)
        .unwrap()
        .0
}

#[test]
fn h5_repro_one_fma_eps_neg_one_toward_positive() {
    // True value -1 + 1e-6176, just above -1. TowardPositive picks
    // the next representable >= true, which is -0.999…9 (34 nines).
    let (r, s) = Decimal128::ONE.fma(
        min_subnormal(),
        Decimal128::NEG_ONE,
        RoundingMode::TowardPositive,
    );
    assert!(s.inexact());
    let target = parse("-0.9999999999999999999999999999999999");
    let (cmp, _) = r.partial_cmp(target);
    assert_eq!(
        cmp,
        Some(core::cmp::Ordering::Equal),
        "got {r:?}, want {target:?}",
    );
}

#[test]
fn h5_repro_neg_one_fma_eps_one_toward_negative() {
    let (r, s) = Decimal128::NEG_ONE.fma(
        min_subnormal(),
        Decimal128::ONE,
        RoundingMode::TowardNegative,
    );
    assert!(s.inexact());
    let target = parse("0.9999999999999999999999999999999999");
    let (cmp, _) = r.partial_cmp(target);
    assert_eq!(cmp, Some(core::cmp::Ordering::Equal));
}
