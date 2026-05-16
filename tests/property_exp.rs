//! Faithful-rounding contract for `Decimal128::exp` vs astro-float.
//!
//! Spot tests at hand-picked inputs plus a random sweep across the
//! supported domain, asserted for **every** IEEE 754 rounding direction
//! against the exact faithful-rounding bracket (ADR-0021, IEEE 754-2019
//! §9.2). See `tests/common/mod.rs` for the contract; this is not a
//! `± ULP` tolerance envelope.

#![cfg(feature = "exp-log")]

use astro_float::{BigFloat, Consts, Radix, RoundingMode as AfRm};
use proptest::prelude::*;

mod common;
use common::{assert_faithful, parse, MODES};

/// Working precision for the astro-float oracle: 256 bits ≈ 77 decimal
/// digits, far above the 36-digit faithful bracket (`common`).
const P: usize = 256;

/// Compute `exp` of the exact value `x_str` at high precision.
fn oracle_exp(x_str: &str, cc: &mut Consts) -> BigFloat {
    let x = BigFloat::parse(x_str, Radix::Dec, P, AfRm::None, cc);
    x.exp(P, AfRm::None, cc)
}

fn check_exp_at(x_str: &str) {
    let x = parse(x_str);
    let exact = format!("{x:e}");
    let mut cc = Consts::new().expect("init consts");
    let oracle = oracle_exp(&exact, &mut cc);
    for &rm in MODES {
        let (got, status) = x.exp(rm);
        assert_faithful(
            got,
            status,
            &oracle,
            &mut cc,
            rm,
            &format!("exp({x_str} → {exact})"),
        );
    }
}

// Spot tests --------------------------------------------------------------

#[test]
fn spot_zero() {
    check_exp_at("0");
}
#[test]
fn spot_one() {
    check_exp_at("1");
}
#[test]
fn spot_neg_one() {
    check_exp_at("-1");
}
#[test]
fn spot_ten() {
    check_exp_at("10");
}
#[test]
fn spot_neg_ten() {
    check_exp_at("-10");
}
#[test]
fn spot_hundred() {
    check_exp_at("100");
}
#[test]
fn spot_pi() {
    check_exp_at("3.14159265358979323846264338327950288");
}
#[test]
fn spot_neg_pi() {
    check_exp_at("-3.14159265358979323846264338327950288");
}
#[test]
fn spot_small_pos() {
    check_exp_at("0.00001");
}
#[test]
fn spot_small_neg() {
    check_exp_at("-0.00001");
}
#[test]
fn spot_near_overflow() {
    check_exp_at("14149");
}
#[test]
fn spot_near_underflow() {
    check_exp_at("-14149");
}
#[test]
fn spot_large_positive() {
    check_exp_at("5000.123456789");
}
#[test]
fn spot_large_negative() {
    check_exp_at("-5000.123456789");
}

// Property sweep ----------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// `exp` is faithfully rounded across a uniform sweep over the
    /// supported domain, for every rounding direction.
    #[test]
    fn exp_random_faithful(
        // Excludes the last few decades to avoid stress-testing
        // OVERFLOW / UNDERFLOW, which the kernel short-circuits.
        bits in any::<u64>(),
        sign in any::<bool>(),
    ) {
        // A value in [10^-30, 10^4] (covers tiny → ~14000).
        let mantissa = bits as f64 / (u64::MAX as f64);
        let exponent_log10 = -30.0 + mantissa * 34.0; // [-30, +4]
        let abs_value: f64 = (bits as f64).rem_euclid(9.0) + 1.0;
        let value_str = format!("{}{}e{}",
            if sign { "-" } else { "" },
            abs_value,
            exponent_log10 as i32);

        let x = parse(&value_str);
        let exact = format!("{x:e}");
        let mut cc = Consts::new().expect("init consts");
        let oracle = oracle_exp(&exact, &mut cc);
        for &rm in MODES {
            let (got, status) = x.exp(rm);
            assert_faithful(
                got, status, &oracle, &mut cc, rm,
                &format!("exp({exact})"),
            );
        }
    }
}
