//! Faithful-rounding contract for `Decimal64::ln` vs astro-float,
//! asserted for every IEEE 754 rounding direction (ADR-0021,
//! IEEE 754-2019 §9.2). See `tests/common/mod.rs`; this is not a
//! `± ULP` tolerance envelope.
//!
//! `Decimal64` exposes `ln` only (no `log10` on the 64-bit surface),
//! so this file covers `ln` across the full positive `Decimal64`
//! range: down to `~10^-398` (subnormal) and up to `~10^+384`, the
//! wider-than-`f64` interval the fd-r0l pilot unlocked by removing the
//! `libm::log` detour.

#![cfg(feature = "exp-log")]

use astro_float::Consts;
use ferrodec_test_support::transcend_oracle::oracle;
use proptest::prelude::*;

mod common;
use common::{assert_faithful, parse, MODES};

fn check_ln_at(x_str: &str) {
    let x = parse(x_str);
    let exact = format!("{x:e}");
    let mut cc = Consts::new().expect("init consts");
    let oracle = oracle::ln(&exact, &mut cc);
    for &rm in MODES {
        let (got, status) = x.ln(rm);
        assert_faithful(
            got,
            status,
            &oracle,
            &mut cc,
            rm,
            &format!("ln({x_str} → {exact})"),
        );
    }
}

// Spot tests --------------------------------------------------------------

#[test]
fn spot_ln_one() {
    check_ln_at("1");
}
#[test]
fn spot_ln_two() {
    check_ln_at("2");
}
#[test]
fn spot_ln_e() {
    check_ln_at("2.718281828459045");
}
#[test]
fn spot_ln_ten() {
    check_ln_at("10");
}
#[test]
fn spot_ln_hundred() {
    check_ln_at("100");
}
#[test]
fn spot_ln_pi() {
    check_ln_at("3.141592653589793");
}
#[test]
fn spot_ln_half() {
    check_ln_at("0.5");
}
#[test]
fn spot_ln_tiny() {
    check_ln_at("1e-30");
}
#[test]
fn spot_ln_subnormal() {
    // Near the bottom of the `Decimal64` positive range.
    check_ln_at("1e-390");
}
#[test]
fn spot_ln_huge() {
    // Near the top of the `Decimal64` positive range; the pre-fd-r0l
    // `f64` detour could not represent the argument.
    check_ln_at("1e300");
}
#[test]
fn spot_ln_near_max() {
    check_ln_at("9.999999999999999e384");
}
#[test]
fn spot_ln_near_one_above() {
    check_ln_at("1.0000000000001");
}
#[test]
fn spot_ln_near_one_below() {
    check_ln_at("0.9999999999999");
}
#[test]
fn spot_ln_random_finite() {
    check_ln_at("123.456789012345");
}

// Property sweep ----------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// `ln` is faithfully rounded across a uniform sweep over the
    /// positive `Decimal64` range, for every rounding direction.
    #[test]
    fn ln_random_faithful(
        coef_bits in 1u64..=u64::MAX,
        exp in -390i32..=370,
    ) {
        // A positive `Decimal64` with a 16-digit-or-less coefficient,
        // scaled across the format's full exponent envelope.
        let coef = coef_bits % (10u64.pow(16));
        if coef == 0 { return Ok(()); }
        let value_str = format!("{coef}e{exp}");
        let x = parse(&value_str);
        let exact = format!("{x:e}");
        let mut cc = Consts::new().expect("init consts");
        let oracle = oracle::ln(&exact, &mut cc);
        for &rm in MODES {
            let (got, status) = x.ln(rm);
            assert_faithful(got, status, &oracle, &mut cc, rm, &format!("ln({exact})"));
        }
    }
}
