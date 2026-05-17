//! Faithful-rounding contract for `Decimal64::cbrt` vs astro-float,
//! asserted for every IEEE 754 rounding direction (ADR-0021,
//! IEEE 754-2019 §9.2). See `tests/common/mod.rs`; this is not a
//! `± ULP` tolerance envelope.
//!
//! `cbrt` is defined for all real `self` (`cbrt(-x) = -cbrt(x)`) and
//! every exponent, so the spot tests cover negatives, fractional
//! inputs, and both magnitude extremes; the sweep is signed across
//! the full `Decimal64` exponent envelope. The finite path now routes
//! through the shared faithful `ferrodec-transcend` kernel, replacing
//! the pre-fd-r0l `libm::cbrt` detour.

#![cfg(feature = "pow")]

use astro_float::Consts;
use ferrodec_test_support::transcend_oracle::oracle;
use proptest::prelude::*;

mod common;
use common::{assert_faithful, parse, MODES};

fn check_cbrt_at(x_str: &str) {
    let x = parse(x_str);
    let exact = format!("{x:e}");
    let mut cc = Consts::new().expect("init consts");
    let oracle = oracle::cbrt(&exact, &mut cc);
    for &rm in MODES {
        let (got, status) = x.cbrt(rm);
        assert_faithful(
            got,
            status,
            &oracle,
            &mut cc,
            rm,
            &format!("cbrt({x_str} → {exact})"),
        );
    }
}

// Spot tests --------------------------------------------------------------

#[test]
fn spot_cbrt_one() {
    check_cbrt_at("1");
}
#[test]
fn spot_cbrt_eight() {
    check_cbrt_at("8");
}
#[test]
fn spot_cbrt_neg_eight() {
    check_cbrt_at("-8");
}
#[test]
fn spot_cbrt_twentyseven() {
    check_cbrt_at("27");
}
#[test]
fn spot_cbrt_neg_twentyseven() {
    check_cbrt_at("-27");
}
#[test]
fn spot_cbrt_two() {
    check_cbrt_at("2");
}
#[test]
fn spot_cbrt_neg_two() {
    check_cbrt_at("-2");
}
#[test]
fn spot_cbrt_fractional() {
    check_cbrt_at("0.001");
}
#[test]
fn spot_cbrt_neg_fractional() {
    check_cbrt_at("-0.001");
}
#[test]
fn spot_cbrt_pi() {
    check_cbrt_at("3.141592653589793");
}
#[test]
fn spot_cbrt_neg_pi() {
    check_cbrt_at("-3.141592653589793");
}
#[test]
fn spot_cbrt_tiny() {
    check_cbrt_at("1e-390");
}
#[test]
fn spot_cbrt_neg_tiny() {
    check_cbrt_at("-1e-390");
}
#[test]
fn spot_cbrt_huge() {
    check_cbrt_at("9.999999999999999e384");
}
#[test]
fn spot_cbrt_neg_huge() {
    check_cbrt_at("-9.999999999999999e384");
}
#[test]
fn spot_cbrt_random_finite() {
    check_cbrt_at("123.456789012345");
}
#[test]
fn spot_cbrt_neg_random_finite() {
    check_cbrt_at("-123.456789012345");
}

// Property sweep ----------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// `cbrt` is faithfully rounded across a signed uniform sweep over
    /// the full `Decimal64` exponent envelope, for every rounding
    /// direction.
    #[test]
    fn cbrt_random_faithful(
        coef_bits in 1u64..=u64::MAX,
        exp in -390i32..=370,
        sign in any::<bool>(),
    ) {
        // A `Decimal64` with a 16-digit-or-less coefficient, scaled
        // across the format's full exponent envelope, of either sign.
        let coef = coef_bits % (10u64.pow(16));
        if coef == 0 { return Ok(()); }
        let value_str = format!("{}{coef}e{exp}", if sign { "-" } else { "" });
        let x = parse(&value_str);
        // The top of the exponent range can round to ±∞ for a 16-digit
        // coefficient; an infinite argument is a special case, not a
        // faithful-rounding input. Skip it (the `±∞` semantics are
        // pinned by the sibling unit tests).
        if !x.is_finite() { return Ok(()); }
        let exact = format!("{x:e}");
        let mut cc = Consts::new().expect("init consts");
        let oracle = oracle::cbrt(&exact, &mut cc);
        for &rm in MODES {
            let (got, status) = x.cbrt(rm);
            assert_faithful(got, status, &oracle, &mut cc, rm, &format!("cbrt({exact})"));
        }
    }
}
