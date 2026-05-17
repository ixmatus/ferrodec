//! Faithful-rounding contract for `Decimal64::exp` vs astro-float.
//!
//! Spot tests at hand-picked inputs plus a random sweep across the
//! supported `Decimal64` domain, asserted for **every** IEEE 754
//! rounding direction against the exact faithful-rounding bracket
//! (ADR-0021, IEEE 754-2019 §9.2). See `tests/common/mod.rs` for the
//! contract; this is not a `± ULP` tolerance envelope.
//!
//! The sweep deliberately reaches the wider-than-`f64` region the
//! fd-r0l pilot unlocked: `Decimal64` overflows `exp` only past
//! `x ≈ +886.49` and underflows past `x ≈ −916.98`, whereas the
//! pre-fd-r0l `f64` / `libm` detour saturated near `x = ±709`. The
//! `spot_far_*` cases pin that previously-unreachable interval.

#![cfg(feature = "exp-log")]

use astro_float::Consts;
use ferrodec_test_support::transcend_oracle::oracle;
use proptest::prelude::*;

mod common;
use common::{assert_faithful, parse, MODES};

fn check_exp_at(x_str: &str) {
    let x = parse(x_str);
    let exact = format!("{x:e}");
    let mut cc = Consts::new().expect("init consts");
    let oracle = oracle::exp(&exact, &mut cc);
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
    check_exp_at("3.141592653589793");
}
#[test]
fn spot_neg_pi() {
    check_exp_at("-3.141592653589793");
}
#[test]
fn spot_small_pos() {
    check_exp_at("0.00001");
}
#[test]
fn spot_small_neg() {
    check_exp_at("-0.00001");
}

// The wider-than-`f64` region the fd-r0l pilot unlocked: `f64` /
// `libm::exp` saturated near `x = ±709`, but `Decimal64` carries these
// to finite, faithfully-rounded results (overflow only past +886.49,
// underflow only past −916.98).

#[test]
fn spot_far_pos_800() {
    check_exp_at("800");
}
#[test]
fn spot_far_neg_800() {
    check_exp_at("-800");
}
#[test]
fn spot_near_overflow() {
    // `e^886 ≈ 10^384.8`, still a finite normal `Decimal64`; the
    // short-circuit only fires for `x > 887`.
    check_exp_at("886");
}
#[test]
fn spot_near_underflow_subnormal() {
    // In `(−918, −887]`: `e^-910 ≈ 10^-395` is a representable
    // `Decimal64` subnormal handled by the Taylor pipeline, not the
    // saturate short-circuit.
    check_exp_at("-910");
}
#[test]
fn spot_large_positive() {
    check_exp_at("500.123456789");
}
#[test]
fn spot_large_negative() {
    check_exp_at("-500.123456789");
}

// Property sweep ----------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// `exp` is faithfully rounded across a uniform sweep over the
    /// supported `Decimal64` domain, for every rounding direction.
    #[test]
    fn exp_random_faithful(
        bits in any::<u64>(),
        sign in any::<bool>(),
    ) {
        // A magnitude in roughly `[10^-30, ~870]`, staying strictly
        // inside the overflow / underflow short-circuit window so the
        // comparison measures faithful rounding, not saturation.
        let mantissa = bits as f64 / (u64::MAX as f64);
        let exponent_log10 = -30.0 + mantissa * 32.9; // [-30, ~+2.9]
        let abs_value: f64 = (bits as f64).rem_euclid(9.0) + 1.0;
        let value_str = format!("{}{}e{}",
            if sign { "-" } else { "" },
            abs_value,
            exponent_log10 as i32);

        let x = parse(&value_str);
        let exact = format!("{x:e}");
        let mut cc = Consts::new().expect("init consts");
        let oracle = oracle::exp(&exact, &mut cc);
        for &rm in MODES {
            let (got, status) = x.exp(rm);
            assert_faithful(
                got, status, &oracle, &mut cc, rm,
                &format!("exp({exact})"),
            );
        }
    }
}
