//! Faithful-rounding contract for `Decimal128::powi` (IEEE 754-2019
//! §9.2 `pown`) against astro-float, asserted for every rounding
//! direction (ADR-0021's floor under ADR-0059's correctly rounded
//! claim). See `tests/common/mod.rs`; this is not a `± ULP` tolerance
//! envelope.
//!
//! The oracle is the shared two-argument `pow` builder with the
//! exponent rendered as an integer literal, so the true value is
//! `x^n` computed independently of either kernel arm: astro-float
//! forms it at 256 bits from its own constants, neither by
//! square-and-multiply at 50 digits nor from `ferrodec`'s `ln`.
//!
//! Both arms are swept deliberately. `|n| ≤ 6` runs the
//! working-precision powering arm that ADR-0060's Liouville floors
//! require; `|n| ≥ 7` runs `exp(n·ln|x|)`. A misplaced seam would
//! show up as one band failing while the other passes.
//!
//! The exact and tie families are not sampled here — they belong to
//! `tests/transcend_exact_powi.rs`, which walks them per mode. The
//! oracle cannot see a negative base (astro-float's `pow` is the real
//! `e^(y ln x)`), so the sign rule is checked against the magnitude
//! run instead, which is the sharper test anyway: it pins the
//! directed-mode reflection (fd-aqs.5) rather than merely the sign.

#![cfg(feature = "pow")]

use ferrodec::{Decimal128, RoundingMode};
use ferrodec_test_support::transcend_oracle::{oracle, Consts};
use proptest::prelude::*;

mod common;
use common::{assert_faithful, parse, MODES};

fn check_powi_at(x_str: &str, n: i32) {
    let x = parse(x_str);
    let exact = format!("{x:e}");
    let mut cc = Consts::new().expect("init consts");
    let oracle = oracle::pow(&exact, &n.to_string(), &mut cc);
    for &rm in MODES {
        let (got, status) = x.powi(n, rm);
        assert_faithful(
            got,
            status,
            &oracle,
            &mut cc,
            rm,
            &format!("powi({x_str} → {exact}, {n})"),
        );
    }
}

// Spot tests --------------------------------------------------------------
//
// The bands the two arms actually branch on: either side of the seam,
// the reciprocal close, a base hugging 1 (where `ln`'s relative model
// is the ADR-0050 hazard), and a full-width coefficient.

#[test]
fn spot_across_the_seam() {
    for n in [-8i32, -7, -6, -5, -1, 1, 2, 5, 6, 7, 8] {
        check_powi_at("1.7", n);
        check_powi_at("0.7", n);
    }
}

#[test]
fn spot_full_width_base() {
    for n in [-7i32, -6, 2, 3, 6, 7] {
        check_powi_at("3.141592653589793238462643383279503", n);
    }
}

#[test]
fn spot_base_hugging_one() {
    // `n · ln x` amplifies an absolute `ln` error by `n`; the near-1
    // direct path (ADR-0050, fd-aqs.6) is what keeps the relative
    // model intact here, on both arms.
    for n in [-7i32, -6, 6, 7, 1000, -1000] {
        check_powi_at("1.000000000000000000000000000000001", n);
        check_powi_at("0.999999999999999999999999999999999", n);
    }
}

#[test]
fn spot_large_exponents() {
    for n in [100i32, -100, 1000, -1000, 4096] {
        check_powi_at("1.0001", n);
    }
}

#[test]
fn spot_wide_decades() {
    check_powi_at("7.77e100", 3);
    check_powi_at("7.77e-100", 3);
    check_powi_at("2.5e-1000", 5);
    check_powi_at("2.5e1000", -5);
}

// Property sweeps ---------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// Faithful in every direction on the powering arm. The base
    /// carries a full-width coefficient (the arm's own stress: every
    /// squaring doubles the accumulated relative error) with its
    /// decade placed so the result stays finite and normal, keeping
    /// the assertion on the rounding rather than on the §7.4
    /// dispositions.
    #[test]
    fn powi_small_n_random_faithful(
        coef_bits in 1u128..=u128::MAX,
        decade in -3i32..=3,
        n in -6i32..=6,
    ) {
        if n == 0 {
            return Ok(());
        }
        let coef = coef_bits % (10u128.pow(34)) + 1;
        let digits = coef.to_string().len() as i32;
        let x = parse(&format!("{coef}e{}", decade - digits + 1));
        let exact = format!("{x:e}");
        let mut cc = Consts::new().expect("init consts");
        let oracle = oracle::pow(&exact, &n.to_string(), &mut cc);
        for &rm in MODES {
            let (got, status) = x.powi(n, rm);
            assert_faithful(
                got,
                status,
                &oracle,
                &mut cc,
                rm,
                &format!("powi({exact}, {n})"),
            );
        }
    }

    /// The same on the `exp(n·ln|x|)` arm, with the exponent band
    /// chosen so `|n · ln x| ≤ ~14150` keeps the result finite.
    #[test]
    fn powi_large_n_random_faithful(
        coef_bits in 1u128..=u128::MAX,
        decade in -2i32..=2,
        n in 7i32..=200,
        negate in any::<bool>(),
    ) {
        let coef = coef_bits % (10u128.pow(34)) + 1;
        let digits = coef.to_string().len() as i32;
        let x = parse(&format!("{coef}e{}", decade - digits + 1));
        let n = if negate { -n } else { n };
        let exact = format!("{x:e}");
        let mut cc = Consts::new().expect("init consts");
        let oracle = oracle::pow(&exact, &n.to_string(), &mut cc);
        for &rm in MODES {
            let (got, status) = x.powi(n, rm);
            assert_faithful(
                got,
                status,
                &oracle,
                &mut cc,
                rm,
                &format!("powi({exact}, {n})"),
            );
        }
    }

    /// The sign rule and its directed-mode reflection, oracle free.
    /// `powi(−x, n)` is `powi(x, n)` for even `n` and its negation for
    /// odd `n` — but for an odd `n` the magnitude must be rounded
    /// under the *reflected* mode (fd-aqs.5), so `TowardPositive` on
    /// the negative result is `TowardZero` on the magnitude. Getting
    /// the reflection backwards is invisible at the nearest modes and
    /// one ulp wrong at the directed ones, which is exactly what this
    /// pins.
    #[test]
    fn powi_negative_base_reflects_the_directed_modes(
        coef_bits in 1u128..=u128::MAX,
        decade in -3i32..=3,
        n in -6i32..=6,
    ) {
        if n == 0 {
            return Ok(());
        }
        let coef = coef_bits % (10u128.pow(34)) + 1;
        let digits = coef.to_string().len() as i32;
        let x = parse(&format!("{coef}e{}", decade - digits + 1));
        let xn = x.neg();
        for &rm in MODES {
            let (neg_result, ns) = xn.powi(n, rm);
            let magnitude_rm = if n % 2 == 0 { rm } else { reflect(rm) };
            let (mag, ms) = x.powi(n, magnitude_rm);
            let want = if n % 2 == 0 { mag } else { mag.neg() };
            prop_assert!(
                neg_result.partial_cmp(want).0 == Some(core::cmp::Ordering::Equal),
                "powi(-{x}, {n}) at {rm:?} = {neg_result}, want {want}"
            );
            prop_assert_eq!(ns.inexact(), ms.inexact(),
                "powi(-{}, {}) at {:?}: flag disagreement", x, n, rm);
        }
    }
}

/// The negation reflection of a rounding mode: the two nearest modes
/// are symmetric about zero, the two directed ones swap, and toward
/// zero is its own reflection.
fn reflect(rm: RoundingMode) -> RoundingMode {
    match rm {
        RoundingMode::TowardPositive => RoundingMode::TowardNegative,
        RoundingMode::TowardNegative => RoundingMode::TowardPositive,
        other => other,
    }
}

/// Monotonicity across the seam, oracle free: for a positive base
/// above 1, `x^n` is strictly increasing in `n`, so the powering arm's
/// `x^6` must not exceed the composition arm's `x^7`, and below 1 the
/// order flips. A seam that dropped or duplicated a factor would break
/// this without breaking either arm's own faithfulness.
#[test]
fn the_seam_is_monotone_in_n() {
    for base in [
        "1.0000001",
        "1.7",
        "2",
        "9.87654321",
        "1e10",
        "0.9999999",
        "0.7",
        "0.5",
        "1e-10",
    ] {
        let x = parse(base);
        let above_one = x.partial_cmp(Decimal128::ONE).0 == Some(core::cmp::Ordering::Greater);
        let mut prev: Option<Decimal128> = None;
        for n in 1..=12i32 {
            let (v, _) = x.powi(n, RoundingMode::NearestEven);
            if let Some(p) = prev {
                let cmp = v.partial_cmp(p).0.expect("finite");
                if above_one {
                    assert!(
                        cmp != core::cmp::Ordering::Less,
                        "powi({base}, {n}) = {v} fell below powi({base}, {}) = {p}",
                        n - 1
                    );
                } else {
                    assert!(
                        cmp != core::cmp::Ordering::Greater,
                        "powi({base}, {n}) = {v} rose above powi({base}, {}) = {p}",
                        n - 1
                    );
                }
            }
            prev = Some(v);
        }
    }
}
