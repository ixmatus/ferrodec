//! Faithful-rounding contract for `Decimal128::exp10` (IEEE 754-2019
//! §9.2) against astro-float, asserted for every rounding direction
//! (ADR-0021's floor under ADR-0059's correctly rounded claim). See
//! `tests/common/mod.rs`; this is not a `± ULP` tolerance envelope.
//!
//! The oracle is the shared two-argument `pow` builder with a literal
//! base of 10, so the true value is `10^x` computed independently of
//! the kernel's own `exp(x · ln 10)` composition: astro-float forms it
//! at 256 bits (about 77 decimal digits) from its own constants, not
//! from `ferrodec`'s `ln 10`.
//!
//! The exact family (the integers, in range and out of it) is not
//! sampled here. It belongs to `tests/transcend_exact_exp10.rs`, which
//! walks it exhaustively; this file covers the irrational complement,
//! the kernel's own ground, and the monotonicity and bracketing
//! properties an oracle comparison alone would not pin.

#![cfg(feature = "exp-log")]

use ferrodec::RoundingMode;
use ferrodec_test_support::transcend_oracle::{oracle, Consts};
use proptest::prelude::*;

mod common;
use common::{assert_faithful, parse, MODES};

fn check_exp10_at(x_str: &str) {
    let x = parse(x_str);
    let exact = format!("{x:e}");
    let mut cc = Consts::new().expect("init consts");
    let oracle = oracle::pow("10", &exact, &mut cc);
    for &rm in MODES {
        let (got, status) = x.exp10(rm);
        assert_faithful(
            got,
            status,
            &oracle,
            &mut cc,
            rm,
            &format!("exp10({x_str} → {exact})"),
        );
    }
}

// Spot tests --------------------------------------------------------------
//
// The bands the pipeline actually branches on: the `k · ln 10`
// reduction window, the decade recomposition, the negative side down
// into the subnormals, and the half-integer arguments whose value
// `sqrt(10) · 10^k` sits as far from a decade point as the family goes.

#[test]
fn spot_half() {
    check_exp10_at("0.5");
}
#[test]
fn spot_neg_half() {
    check_exp10_at("-0.5");
}
#[test]
fn spot_half_integers() {
    for k in [1i32, 2, 7, 34, 100, -1, -7, -34, -100] {
        check_exp10_at(&format!("{k}.5"));
    }
}
#[test]
fn spot_small_positive() {
    check_exp10_at("0.00001");
}
#[test]
fn spot_small_negative() {
    check_exp10_at("-0.00001");
}
#[test]
fn spot_full_width_argument() {
    check_exp10_at("3.141592653589793238462643383279503");
}
#[test]
fn spot_negative_full_width_argument() {
    check_exp10_at("-2.718281828459045235360287471352662");
}
#[test]
fn spot_just_inside_the_top_decade() {
    // `10^6144.5 ≈ 3.16e6144` is representable; the neighbouring
    // integers are the classifier's, not the kernel's.
    check_exp10_at("6144.5");
    check_exp10_at("6144.999999999999999999999999999999");
}
#[test]
fn spot_just_inside_the_subnormal_tail() {
    // Subnormal results, where the rounder's drop position is the
    // quantum excess rather than the precision excess.
    check_exp10_at("-6150.5");
    check_exp10_at("-6175.5");
}
#[test]
fn spot_beside_an_integer() {
    check_exp10_at("2.000000000000000000000000000000001");
    check_exp10_at("1.999999999999999999999999999999999");
}

// Property sweeps ---------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// Faithful in every direction across the ordinary domain. The
    /// argument carries a full width coefficient (the reduction's own
    /// stress) whose decade is placed explicitly, so every generated
    /// case is a live one: no `return Ok(())` filter silently empties
    /// the sweep. The decade band `[10^-5, 10^3)` keeps the assertion
    /// on the kernel's rounding rather than on the §7.4 over/underflow
    /// dispositions, which the exact-family gate pins per mode.
    #[test]
    fn exp10_random_faithful(
        coef_bits in 1u128..=u128::MAX,
        decade in -5i32..=2,
    ) {
        let coef = coef_bits % (10u128.pow(34)) + 1;
        let digits = coef.to_string().len() as i32;
        let x = parse(&format!("{coef}e{}", decade - digits + 1));
        let exact = format!("{x:e}");
        let mut cc = Consts::new().expect("init consts");
        let oracle = oracle::pow("10", &exact, &mut cc);
        for &rm in MODES {
            let (got, status) = x.exp10(rm);
            assert_faithful(
                got,
                status,
                &oracle,
                &mut cc,
                rm,
                &format!("exp10({exact})"),
            );
        }
        // The negation too: the negative side runs the same pipeline
        // with a different decade sign.
        let xn = x.neg();
        let exact_n = format!("{xn:e}");
        let oracle_n = oracle::pow("10", &exact_n, &mut cc);
        for &rm in MODES {
            let (got, status) = xn.exp10(rm);
            assert_faithful(
                got,
                status,
                &oracle_n,
                &mut cc,
                rm,
                &format!("exp10({exact_n})"),
            );
        }
    }

    /// Monotonicity and decade bracketing, oracle free: `exp10` is
    /// strictly increasing, so a step up in the input never lowers the
    /// result, and an argument in `[n, n+1)` lands in
    /// `[10^n, 10^(n+1)]` (the closed upper end absorbs the rounding at
    /// `NearestEven`). The argument is assembled as
    /// `(n·10^6 + frac)·10^-6` rather than spelled with a decimal
    /// point, so the fraction is added toward `+∞` on both sides of
    /// zero and the floor really is `n`.
    #[test]
    fn exp10_is_monotone_and_bracketed_by_its_decades(
        n in -6100i32..=6100,
        frac_bits in 0u32..=999_999,
    ) {
        let scaled = i64::from(n) * 1_000_000 + i64::from(frac_bits);
        let x = parse(&format!("{scaled}e-6"));
        let (v, _) = x.exp10(RoundingMode::NearestEven);
        let low = parse(&format!("1e{n}"));
        let high = parse(&format!("1e{}", n + 1));
        prop_assert!(
            v.partial_cmp(low).0 != Some(core::cmp::Ordering::Less),
            "exp10({x}) = {v} fell below 10^{n}"
        );
        prop_assert!(
            v.partial_cmp(high).0 != Some(core::cmp::Ordering::Greater),
            "exp10({x}) = {v} rose above 10^{}", n + 1
        );

        // Monotone against the next representable input.
        let (up, _) = x.next_up();
        let (v_up, _) = up.exp10(RoundingMode::NearestEven);
        prop_assert!(
            v_up.partial_cmp(v).0 != Some(core::cmp::Ordering::Less),
            "exp10 not monotone at {x}: {v} then {v_up}"
        );
    }
}
