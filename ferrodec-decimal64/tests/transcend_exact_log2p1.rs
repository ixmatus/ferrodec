//! Exact-result and special-value gate for `Decimal64::log2_1p`
//! (IEEE 754-2019 §9.2 `log2p1`; ADR-0059 Track D), the sibling mirror
//! of the root crate's `tests/transcend_exact_log2p1.rs`.
//!
//! The classifier `ferrodec_transcend::exact::log2p1_exact` claims a
//! complete exact set: a rational `log2(1 + x)` at a representable `x`
//! is an integer `k` with `1 + x = 2^k`, which splits into the odd
//! integers `x = 2^k − 1` for `k ≥ 1` and the fractions
//! `x = −(10^m − 5^m)·10^−m` for `k = −m ≤ −1`. This file is that
//! claim's exhaustive witness at 16 digits: every `k` in `1..=53` and
//! every `m` in `1..=16`, in all five rounding directions, delivered
//! exactly with status `OK` and no `INEXACT` (§7.5 forbids it on an
//! exact result).
//!
//! Each classified case is cross-checked against an independent
//! witness, the house two-proofs pattern: reconstruct `2^k` in `u128`
//! integer arithmetic, confirm `1 ⊕ x` reproduces it exactly through
//! the format's own addition, and confirm the separately derived
//! `log2` classifier reads `k` back off it.

#![cfg(feature = "exp-log")]

use core::cmp::Ordering;
use ferrodec_decimal64::{Decimal64, RoundingMode, Status};

const NE: RoundingMode = RoundingMode::NearestEven;
const NA: RoundingMode = RoundingMode::NearestAway;
const TZ: RoundingMode = RoundingMode::TowardZero;
const TP: RoundingMode = RoundingMode::TowardPositive;
const TN: RoundingMode = RoundingMode::TowardNegative;
const ALL: [RoundingMode; 5] = [NE, NA, TZ, TP, TN];

/// The format's exact-family ceilings: `2^53 − 1` is exactly 16
/// digits, and `10^16 − 5^16` is exactly 16 digits.
const K_MAX: u32 = 53;
const M_MAX: u32 = 16;

fn parse(s: &str) -> Decimal64 {
    Decimal64::parse_str(s, NE)
        .unwrap_or_else(|_| panic!("test literal parses: {s:?}"))
        .0
}

fn eq(got: Decimal64, want: Decimal64) -> bool {
    got.partial_cmp(want).0 == Some(Ordering::Equal)
}

/// The positive exact family input `x = 2^k − 1`, built from integer
/// arithmetic through its decimal string (no float detour).
fn pos_input(k: u32) -> Decimal64 {
    parse(&format!("{}", (1u128 << k) - 1))
}

/// The negative exact family input `x = −(10^m − 5^m)·10^−m`.
fn neg_input(m: u32) -> Decimal64 {
    parse(&format!("-{}e-{m}", 10u128.pow(m) - 5u128.pow(m)))
}

fn assert_exact_int(got: (Decimal64, Status), want: i32, label: &str) {
    let (r, st) = got;
    let want_d = parse(&format!("{want}"));
    assert!(eq(r, want_d), "{label}: got {r}, want {want}");
    assert_eq!(
        st,
        Status::OK,
        "{label}: exact result must be OK, got {st:?}"
    );
    assert!(!st.inexact(), "{label}: §7.5 forbids INEXACT here");
}

/// Every `x = 2^k − 1` in reach of 16 digits, all five modes.
#[test]
fn exact_family_positive_exhaustive_every_mode() {
    for k in 1..=K_MAX {
        let x = pos_input(k);
        for rm in ALL {
            assert_exact_int(
                x.log2_1p(rm),
                i32::try_from(k).unwrap(),
                &format!("log2_1p(2^{k} - 1) [{rm:?}]"),
            );
        }
    }
}

/// Every `x = −(10^m − 5^m)·10^−m` in reach of 16 digits, all five
/// modes. The delivered value is `−m`.
#[test]
fn exact_family_negative_exhaustive_every_mode() {
    for m in 1..=M_MAX {
        let x = neg_input(m);
        for rm in ALL {
            assert_exact_int(
                x.log2_1p(rm),
                -i32::try_from(m).unwrap(),
                &format!("log2_1p(2^-{m} - 1) [{rm:?}]"),
            );
        }
    }
}

/// Independent witness for both families: reconstruct `2^k` in exact
/// `u128` arithmetic, confirm the format's own addition takes `1 ⊕ x`
/// to it with no rounding, and confirm the separately derived `log2`
/// classifier reads `k` back. Two proofs of the same boundary fact.
#[test]
fn exact_family_matches_the_independent_witness() {
    for k in 1..=K_MAX {
        let x = pos_input(k);
        let pow2 = parse(&format!("{}", 1u128 << k));
        let (sum, st) = Decimal64::ONE.add(x, NE);
        assert!(eq(sum, pow2), "1 + (2^{k} - 1) = {sum}, want 2^{k}");
        assert!(!st.inexact(), "1 + (2^{k} - 1) is exact, got {st:?}");
        let (back, st) = pow2.log2(NE);
        assert!(
            eq(back, parse(&format!("{k}"))) && st == Status::OK,
            "log2(2^{k}) = {back} {st:?}, want {k} OK"
        );
    }
    for m in 1..=M_MAX {
        let x = neg_input(m);
        let pow2 = parse(&format!("{}e-{m}", 5u128.pow(m)));
        let (sum, st) = Decimal64::ONE.add(x, NE);
        assert!(eq(sum, pow2), "1 + (2^-{m} - 1) = {sum}, want 2^-{m}");
        assert!(!st.inexact(), "1 + (2^-{m} - 1) is exact, got {st:?}");
        let (back, st) = pow2.log2(NE);
        assert!(
            eq(back, parse(&format!("-{m}"))) && st == Status::OK,
            "log2(2^-{m}) = {back} {st:?}, want -{m} OK"
        );
    }
}

/// One ulp beside an exact input the result is inexact and lands
/// within one ulp of `k`, on the side a strictly increasing function
/// requires.
#[test]
fn neighbour_probes_step_the_right_way() {
    for k in [1u32, 10, 29, K_MAX] {
        let x = pos_input(k);
        let k_d = parse(&format!("{k}"));
        let (up_in, _) = x.next_up();
        let (r, st) = up_in.log2_1p(NE);
        assert!(st.inexact(), "log2_1p(next_up(2^{k} - 1)) must be INEXACT");
        assert!(
            r.partial_cmp(k_d).0 != Some(Ordering::Less),
            "log2_1p above 2^{k} - 1 fell below {k}: {r}"
        );
        assert!(
            r.partial_cmp(k_d.next_up().0).0 != Some(Ordering::Greater),
            "log2_1p above 2^{k} - 1 overshot one ulp past {k}: {r}"
        );

        let (dn_in, _) = x.next_down();
        let (r, st) = dn_in.log2_1p(NE);
        assert!(
            st.inexact(),
            "log2_1p(next_down(2^{k} - 1)) must be INEXACT"
        );
        assert!(
            r.partial_cmp(k_d).0 != Some(Ordering::Greater),
            "log2_1p below 2^{k} - 1 rose above {k}: {r}"
        );
        assert!(
            r.partial_cmp(k_d.next_down().0).0 != Some(Ordering::Less),
            "log2_1p below 2^{k} - 1 undershot one ulp past {k}: {r}"
        );
    }
}

/// IEEE 754-2019 §9.2.1 special values, every rounding direction.
#[test]
fn specials_per_section_9_2_1() {
    for rm in ALL {
        let (r, st) = Decimal64::ZERO.log2_1p(rm);
        assert!(r.is_zero() && !r.is_sign_negative(), "log2_1p(+0) = {r}");
        assert_eq!(st, Status::OK, "log2_1p(+0) status {st:?}");
        let (r, st) = Decimal64::NEG_ZERO.log2_1p(rm);
        assert!(r.is_zero() && r.is_sign_negative(), "log2_1p(-0) = {r}");
        assert_eq!(st, Status::OK, "log2_1p(-0) status {st:?}");

        let (r, st) = Decimal64::NEG_ONE.log2_1p(rm);
        assert!(r.is_infinite() && r.is_sign_negative(), "log2_1p(-1) = {r}");
        assert!(st.div_by_zero(), "log2_1p(-1) status {st:?}");

        let (r, st) = parse("-2").log2_1p(rm);
        assert!(r.is_nan() && st.invalid(), "log2_1p(-2) = {r} {st:?}");
        // The representable neighbour just past −1 (16 digits, so the
        // literal survives the parse intact).
        let just_past = parse(&format!("-1.{}1", "0".repeat(14)));
        let (r, st) = just_past.log2_1p(rm);
        assert!(
            r.is_nan() && st.invalid(),
            "log2_1p(just below -1) = {r} {st:?}"
        );
        let (r, st) = Decimal64::NEG_INFINITY.log2_1p(rm);
        assert!(r.is_nan() && st.invalid(), "log2_1p(-inf) = {r} {st:?}");

        let (r, st) = Decimal64::INFINITY.log2_1p(rm);
        assert!(
            r.is_infinite() && !r.is_sign_negative(),
            "log2_1p(+inf) = {r}"
        );
        assert_eq!(st, Status::OK, "log2_1p(+inf) status {st:?}");

        let (r, st) = Decimal64::NAN.log2_1p(rm);
        assert!(r.is_nan() && st.is_ok(), "log2_1p(NaN) = {r} {st:?}");
        let (r, st) = Decimal64::SIGNALING_NAN.log2_1p(rm);
        assert!(r.is_nan() && st.invalid(), "log2_1p(sNaN) = {r} {st:?}");
    }
}

/// Flag honesty (§7.5): generic finite inputs are inexact in every
/// mode, and every classifier delivery is exact in every mode.
#[test]
fn flag_honesty_across_modes() {
    for label in [
        "0.1".to_string(),
        "2".to_string(),
        "-0.3".to_string(),
        format!("-0.{}", "9".repeat(16)),
        "1e100".to_string(),
        "1e-100".to_string(),
        "9.999999999999999e384".to_string(),
    ] {
        let x = parse(&label);
        for rm in ALL {
            let (_, st) = x.log2_1p(rm);
            assert!(st.inexact(), "log2_1p({label}) [{rm:?}] status {st:?}");
        }
    }
    for k in [1u32, 2, 3, 29, K_MAX] {
        let x = pos_input(k);
        for rm in ALL {
            let (_, st) = x.log2_1p(rm);
            assert!(
                !st.inexact(),
                "log2_1p(2^{k} - 1) [{rm:?}] must not be INEXACT: {st:?}"
            );
        }
    }
    for m in [1u32, 2, 3, 9, M_MAX] {
        let x = neg_input(m);
        for rm in ALL {
            let (_, st) = x.log2_1p(rm);
            assert!(
                !st.inexact(),
                "log2_1p(2^-{m} - 1) [{rm:?}] must not be INEXACT: {st:?}"
            );
        }
    }
}

/// The named exact values the public rustdoc quotes, pinned so the
/// documentation and the kernel cannot drift apart.
#[test]
fn documented_exact_values() {
    for rm in ALL {
        assert_exact_int(parse("1").log2_1p(rm), 1, "log2_1p(1)");
        assert_exact_int(parse("3").log2_1p(rm), 2, "log2_1p(3)");
        assert_exact_int(parse("7").log2_1p(rm), 3, "log2_1p(7)");
        assert_exact_int(parse("-0.5").log2_1p(rm), -1, "log2_1p(-0.5)");
        assert_exact_int(parse("-0.75").log2_1p(rm), -2, "log2_1p(-0.75)");
        assert_exact_int(parse("-0.875").log2_1p(rm), -3, "log2_1p(-0.875)");
    }
}

/// One digit off an exact family member is not exact: the classifier
/// must decline, and the kernel must then raise `INEXACT`. Guards the
/// bail sites against a classifier that over-claims.
#[test]
fn near_misses_are_declined() {
    for label in [
        "2", "4", "5", "6", "8", "9", "30", "1000", "0.5", "1.5", "3.5", "-0.4", "-0.6", "-0.74",
        "-0.76", "-0.874", "-0.876",
    ] {
        let x = parse(label);
        for rm in ALL {
            let (_, st) = x.log2_1p(rm);
            assert!(
                st.inexact(),
                "log2_1p({label}) [{rm:?}] is irrational: {st:?}"
            );
        }
    }
    // The family ceiling from the other side: `2^53` is itself
    // representable at 16 digits, but `1 + 2^53` is no power of two,
    // so the classifier declines and the kernel raises INEXACT.
    let ceiling = parse(&format!("{}", 1u128 << K_MAX));
    for rm in ALL {
        let (_, st) = ceiling.log2_1p(rm);
        assert!(
            st.inexact(),
            "log2_1p(2^{K_MAX}) [{rm:?}] is irrational: {st:?}"
        );
    }
}
