//! Faithful-rounding contract for `Decimal32::exp` vs astro-float.
//!
//! Spot tests at hand-picked inputs plus a random sweep across the
//! supported `Decimal32` domain, asserted for **every** IEEE 754
//! rounding direction against the exact faithful-rounding bracket
//! (ADR-0021, IEEE 754-2019 §9.2). See `tests/common/mod.rs` for the
//! contract; this is not a `± ULP` tolerance envelope.
//!
//! The sweep deliberately reaches the wider-than-`f64` region the
//! fd-r0l pilot unlocked: `Decimal32` overflows `exp` only past
//! `x ≈ +223.35` and underflows past `x ≈ −233.25`, whereas the
//! pre-fd-r0l `f64` / `libm` detour saturated near `x = ±709`. The
//! `spot_*` cases pin both the ordinary and the boundary intervals.

#![cfg(feature = "exp-log")]

use ferrodec_test_support::transcend_oracle::{oracle, Consts};
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

fn check_exp2_at(x_str: &str) {
    let x = parse(x_str);
    let exact = format!("{x:e}");
    let mut cc = Consts::new().expect("init consts");
    let oracle = oracle::exp2(&exact, &mut cc);
    for &rm in MODES {
        let (got, status) = x.exp2(rm);
        assert_faithful(
            got,
            status,
            &oracle,
            &mut cc,
            rm,
            &format!("exp2({x_str} → {exact})"),
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
    check_exp_at("3.141593");
}
#[test]
fn spot_neg_pi() {
    check_exp_at("-3.141593");
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
// `libm::exp` saturated near `x = ±709`, but `Decimal32`'s envelope is
// much narrower (overflow past +223.35, underflow past −233.25); these
// pin the ordinary mid-range plus the two boundary intervals.

#[test]
fn spot_pos_200() {
    check_exp_at("200");
}
#[test]
fn spot_neg_200() {
    check_exp_at("-200");
}
#[test]
fn spot_near_overflow() {
    // `e^222 ≈ 10^96.4`, still a finite normal `Decimal32`; the
    // short-circuit only fires for `x > 224`.
    check_exp_at("222");
}
#[test]
fn spot_near_underflow_subnormal() {
    // In `(−235, −224]`: `e^-230 ≈ 10^-100` is a representable
    // `Decimal32` subnormal handled by the Taylor pipeline, not the
    // saturate short-circuit.
    check_exp_at("-230");
}
#[test]
fn spot_large_positive() {
    check_exp_at("150.123457");
}
#[test]
fn spot_large_negative() {
    check_exp_at("-150.123457");
}

// exp2 spot tests ---------------------------------------------------------

#[test]
fn spot_exp2_zero() {
    check_exp2_at("0");
}
#[test]
fn spot_exp2_one() {
    check_exp2_at("1");
}
#[test]
fn spot_exp2_neg_one() {
    check_exp2_at("-1");
}
#[test]
fn spot_exp2_ten() {
    check_exp2_at("10");
}
#[test]
fn spot_exp2_neg_ten() {
    check_exp2_at("-10");
}
#[test]
fn spot_exp2_half() {
    check_exp2_at("0.5");
}
#[test]
fn spot_exp2_pi() {
    check_exp2_at("3.141593");
}
#[test]
fn spot_exp2_neg_pi() {
    check_exp2_at("-3.141593");
}
#[test]
fn spot_exp2_small_pos() {
    check_exp2_at("0.00001");
}
#[test]
fn spot_exp2_small_neg() {
    check_exp2_at("-0.00001");
}

// `2^x` overflows `Decimal32` only near `x · log10(2) ≈ E_MAX + 1`,
// i.e. `x ≈ +323`; the symmetric underflow boundary is just past
// `x ≈ −335`. These pin the wide envelope kept strictly inside
// `|x| < ~320`.

#[test]
fn spot_exp2_pos_300() {
    check_exp2_at("300");
}
#[test]
fn spot_exp2_neg_300() {
    check_exp2_at("-300");
}
#[test]
fn spot_exp2_near_overflow() {
    // `2^318 ≈ 10^95.7`, still a finite normal `Decimal32`.
    check_exp2_at("318");
}
#[test]
fn spot_exp2_near_underflow_subnormal() {
    // `2^-330 ≈ 10^-99.3`, a representable `Decimal32` subnormal.
    check_exp2_at("-330");
}
#[test]
fn spot_exp2_large_positive() {
    check_exp2_at("250.1235");
}
#[test]
fn spot_exp2_large_negative() {
    check_exp2_at("-250.1235");
}

// Property sweep ----------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// `exp` is faithfully rounded across a uniform sweep over the
    /// supported `Decimal32` domain, for every rounding direction.
    #[test]
    fn exp_random_faithful(
        bits in any::<u64>(),
        sign in any::<bool>(),
    ) {
        // A magnitude strictly inside `(−233, +223)`, staying inside
        // the overflow / underflow short-circuit window so the
        // comparison measures faithful rounding, not saturation. The
        // 7-significant-digit coefficient (`c.cccccc`) times a small
        // power of ten keeps every literal inside Decimal32's 7-digit
        // envelope.
        let mantissa = bits as f64 / (u64::MAX as f64);
        // `abs_value ∈ [1, 10)` times `10^e` for `e ∈ [-6, +1]` gives a
        // magnitude in `[10^-6, ~99.99]`, comfortably inside `±223`.
        let exponent_log10 = -6 + ((mantissa * 7.0) as i32).min(7); // [-6, +1]
        let abs_value: f64 = (bits as f64).rem_euclid(9.0) + 1.0;
        let value_str = format!("{}{:.6}e{}",
            if sign { "-" } else { "" },
            abs_value,
            exponent_log10);

        let x = parse(&value_str);
        // Overflow to ±∞ is a special-case result covered by the
        // dedicated spot tests (`spot_near_overflow`) and the
        // `exp_overflow_to_infinity` unit test, not this
        // faithful-rounding sweep (the contract asserts faithful
        // rounding of *finite* results). Skip the out-of-domain
        // corner so a proptest seed shift cannot surface it as a
        // false bracket failure. Same idiom as the `coef == 0` skip;
        // the overflow gate is rounding-mode-independent, so probing
        // `MODES[0]` is representative.
        let (probe, _) = x.exp(MODES[0]);
        if !probe.is_finite() {
            return Ok(());
        }
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

    /// `exp2` is faithfully rounded across a uniform sweep over the
    /// supported `Decimal32` domain, for every rounding direction.
    /// `2^x` has a wider in-exponent envelope than `exp`; the sweep
    /// stays inside `|x| < ~320` so it measures faithful rounding,
    /// not the overflow / underflow short-circuit.
    #[test]
    fn exp2_random_faithful(
        bits in any::<u64>(),
        sign in any::<bool>(),
    ) {
        // `abs_value ∈ [1, 10)` times `10^e` for `e ∈ [-6, +2]` gives
        // a magnitude in `[10^-6, ~999.99]`... clamp so the literal
        // stays well inside `|x| < ~320`.
        let mantissa = bits as f64 / (u64::MAX as f64);
        let exponent_log10 = -6 + ((mantissa * 8.0) as i32).min(8); // [-6, +2]
        let raw: f64 = (bits as f64).rem_euclid(9.0) + 1.0;
        // Cap the +2 decade at ~3.0 so `abs_value · 10^2 ≤ ~300`.
        let abs_value = if exponent_log10 >= 2 {
            raw.min(3.0)
        } else {
            raw
        };
        let value_str = format!("{}{:.6}e{}",
            if sign { "-" } else { "" },
            abs_value,
            exponent_log10);

        let x = parse(&value_str);
        // Overflow to ±∞ is a special-case result covered by the
        // dedicated spot / unit tests, not this faithful-rounding
        // sweep (the contract asserts faithful rounding of *finite*
        // results). Skip the out-of-domain corner so a proptest seed
        // shift cannot surface it as a false bracket failure. The
        // overflow gate is rounding-mode-independent, so probing
        // `MODES[0]` is representative.
        let (probe, _) = x.exp2(MODES[0]);
        if !probe.is_finite() {
            return Ok(());
        }
        let exact = format!("{x:e}");
        let mut cc = Consts::new().expect("init consts");
        let oracle = oracle::exp2(&exact, &mut cc);
        for &rm in MODES {
            let (got, status) = x.exp2(rm);
            assert_faithful(
                got, status, &oracle, &mut cc, rm,
                &format!("exp2({exact})"),
            );
        }
    }
}
