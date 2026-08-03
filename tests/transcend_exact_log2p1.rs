//! Exact-result and special-value gate for `Decimal128::log2_1p`
//! (IEEE 754-2019 §9.2 `log2p1`; ADR-0059 Track D).
//!
//! The classifier `ferrodec_transcend::exact::log2p1_exact` claims a
//! complete exact set: a rational `log2(1 + x)` at a representable `x`
//! is an integer `k` with `1 + x = 2^k`, which splits into the odd
//! integers `x = 2^k − 1` for `k ≥ 1` and the fractions
//! `x = −(10^m − 5^m)·10^−m` for `k = −m ≤ −1`. This file is that
//! claim's exhaustive witness at 34 digits: every `k` in `1..=112` and
//! every `m` in `1..=34`, in all five rounding directions, delivered
//! exactly with status `OK` and no `INEXACT` (§7.5 forbids it on an
//! exact result).
//!
//! Each classified case is cross-checked against an independent
//! witness, the house two-proofs pattern: reconstruct `2^k` in `u128`
//! integer arithmetic, confirm `1 ⊕ x` reproduces it exactly through
//! the format's own addition, and confirm the separately derived
//! `log2` classifier reads `k` back off it. The neighbour probes then
//! pin that one ulp beside an exact input the result is inexact and
//! lands within one ulp of `k` on the side the strictly increasing
//! function requires.

#![cfg(feature = "exp-log")]

use core::cmp::Ordering;
use ferrodec::{Decimal128, RoundingMode, Status};

const NE: RoundingMode = RoundingMode::NearestEven;
const NA: RoundingMode = RoundingMode::NearestAway;
const TZ: RoundingMode = RoundingMode::TowardZero;
const TP: RoundingMode = RoundingMode::TowardPositive;
const TN: RoundingMode = RoundingMode::TowardNegative;
const ALL: [RoundingMode; 5] = [NE, NA, TZ, TP, TN];

/// The format's exact-family ceilings: `2^112 − 1` is exactly 34
/// digits, and `10^34 − 5^34` is exactly 34 digits.
const K_MAX: u32 = 112;
const M_MAX: u32 = 34;

fn parse(s: &str) -> Decimal128 {
    Decimal128::parse_str(s, NE)
        .unwrap_or_else(|_| panic!("test literal parses: {s:?}"))
        .0
}

fn eq(got: Decimal128, want: Decimal128) -> bool {
    got.partial_cmp(want).0 == Some(Ordering::Equal)
}

/// The positive exact family input `x = 2^k − 1`, built from integer
/// arithmetic through its decimal string (no float detour).
fn pos_input(k: u32) -> Decimal128 {
    parse(&format!("{}", (1u128 << k) - 1))
}

/// The negative exact family input `x = −(10^m − 5^m)·10^−m`.
fn neg_input(m: u32) -> Decimal128 {
    parse(&format!("-{}e-{m}", 10u128.pow(m) - 5u128.pow(m)))
}

fn assert_exact_int(got: (Decimal128, Status), want: i32, label: &str) {
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

/// Every `x = 2^k − 1` in reach of 34 digits, all five modes.
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

/// Every `x = −(10^m − 5^m)·10^−m` in reach of 34 digits, all five
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
        // 2^k as an exact integer, independent of the classifier.
        let pow2 = parse(&format!("{}", 1u128 << k));
        let (sum, st) = Decimal128::ONE.add(x, NE);
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
        // 2^-m = 5^m · 10^-m as an exact decimal fraction.
        let pow2 = parse(&format!("{}e-{m}", 5u128.pow(m)));
        let (sum, st) = Decimal128::ONE.add(x, NE);
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
/// requires: above `k` for the step up, below `k` for the step down.
#[test]
fn neighbour_probes_step_the_right_way() {
    for k in [1u32, 10, 57, K_MAX] {
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
        // ±0 → ±0, sign preserved, no exception.
        let (r, st) = Decimal128::ZERO.log2_1p(rm);
        assert!(r.is_zero() && !r.is_sign_negative(), "log2_1p(+0) = {r}");
        assert_eq!(st, Status::OK, "log2_1p(+0) status {st:?}");
        let (r, st) = Decimal128::NEG_ZERO.log2_1p(rm);
        assert!(r.is_zero() && r.is_sign_negative(), "log2_1p(-0) = {r}");
        assert_eq!(st, Status::OK, "log2_1p(-0) status {st:?}");

        // −1 → −∞ with DIV_BY_ZERO.
        let (r, st) = Decimal128::NEG_ONE.log2_1p(rm);
        assert!(r.is_infinite() && r.is_sign_negative(), "log2_1p(-1) = {r}");
        assert!(st.div_by_zero(), "log2_1p(-1) status {st:?}");

        // Below −1, and −∞, are domain errors.
        let (r, st) = parse("-2").log2_1p(rm);
        assert!(r.is_nan() && st.invalid(), "log2_1p(-2) = {r} {st:?}");
        // The representable neighbour just past −1 (34 digits, so the
        // literal survives the parse intact).
        let just_past = parse(&format!("-1.{}1", "0".repeat(32)));
        let (r, st) = just_past.log2_1p(rm);
        assert!(
            r.is_nan() && st.invalid(),
            "log2_1p(just below -1) = {r} {st:?}"
        );
        let (r, st) = Decimal128::NEG_INFINITY.log2_1p(rm);
        assert!(r.is_nan() && st.invalid(), "log2_1p(-inf) = {r} {st:?}");

        // +∞ → +∞.
        let (r, st) = Decimal128::INFINITY.log2_1p(rm);
        assert!(
            r.is_infinite() && !r.is_sign_negative(),
            "log2_1p(+inf) = {r}"
        );
        assert_eq!(st, Status::OK, "log2_1p(+inf) status {st:?}");

        // NaN propagation; sNaN raises INVALID.
        let (r, st) = Decimal128::NAN.log2_1p(rm);
        assert!(r.is_nan() && st.is_ok(), "log2_1p(NaN) = {r} {st:?}");
        let (r, st) = Decimal128::SIGNALING_NAN.log2_1p(rm);
        assert!(r.is_nan() && st.invalid(), "log2_1p(sNaN) = {r} {st:?}");
    }
}

/// Flag honesty (§7.5): generic finite inputs are inexact in every
/// mode, and every classifier delivery is exact in every mode.
#[test]
fn flag_honesty_across_modes() {
    for label in [
        "0.1",
        "2",
        "-0.3",
        "-0.9999999999999999999999999999999999",
        "1e100",
        "1e-100",
        "9.999999999999999999999999999999999e6144",
    ] {
        let x = parse(label);
        for rm in ALL {
            let (_, st) = x.log2_1p(rm);
            assert!(st.inexact(), "log2_1p({label}) [{rm:?}] status {st:?}");
        }
    }
    // The classifier's deliveries carry no INEXACT anywhere.
    for k in [1u32, 2, 3, 57, K_MAX] {
        let x = pos_input(k);
        for rm in ALL {
            let (_, st) = x.log2_1p(rm);
            assert!(
                !st.inexact(),
                "log2_1p(2^{k} - 1) [{rm:?}] must not be INEXACT: {st:?}"
            );
        }
    }
    for m in [1u32, 2, 3, 17, M_MAX] {
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

/// The classifier decides on the *stripped* form, so an exact family
/// member handed in any cohort of its value must classify the same
/// way. Trailing zeros move the stored exponent, which is exactly the
/// quantity both bail sites read, so this is the cohort witness for
/// those bails.
#[test]
fn cohorts_of_an_exact_input_classify_alike() {
    let cases: [(&str, i32); 12] = [
        ("1", 1),
        ("1.0", 1),
        ("1.000000000000000000000000000000000", 1),
        ("3", 2),
        ("3.00", 2),
        ("3.000000000000000000000000000000000", 2),
        ("-0.5", -1),
        ("-0.50", -1),
        ("-0.5000000000000000000000000000000000", -1),
        ("-0.75", -2),
        ("-0.750000", -2),
        ("-0.8750000000000000000000000000000000", -3),
    ];
    for (label, want) in cases {
        for rm in ALL {
            assert_exact_int(
                parse(label).log2_1p(rm),
                want,
                &format!("log2_1p({label}) [{rm:?}]"),
            );
        }
    }
}

/// Tiny inputs, where the `log1p` series collapses to exactly `x` and
/// only the `1/ln 2` scaling separates the result from the input's own
/// grid point. `log2p1` carries no ADR-0051 anchor seam by design (its
/// slope at zero is `1/ln 2`, not 1), so this is the executable
/// witness that the ladder still terminates and delivers there, on
/// every rung the test lanes route through, down to the subnormal
/// floor.
#[test]
fn tiny_inputs_deliver_without_an_anchor_seam() {
    for label in [
        "1e-40", "-1e-40", "1e-3000", "-1e-3000", "1e-6100", "-1e-6100", "1e-6170", "-1e-6170",
    ] {
        let x = parse(label);
        for rm in ALL {
            let (r, st) = x.log2_1p(rm);
            assert!(
                !r.is_nan() && !r.is_infinite(),
                "log2_1p({label}) [{rm:?}] left the finite range: {r}"
            );
            assert!(!r.is_zero(), "log2_1p({label}) [{rm:?}] collapsed to zero");
            assert_eq!(
                r.is_sign_negative(),
                x.is_sign_negative(),
                "log2_1p({label}) [{rm:?}] flipped the sign: {r}"
            );
            assert!(
                st.inexact(),
                "log2_1p({label}) [{rm:?}] must be INEXACT: {st:?}"
            );
        }
        // `|log2(1 + x)| > |x|` for a tiny `x`: the slope `1/ln 2`
        // exceeds 1 and dominates the second order term. This is the
        // property the missing anchor seam rests on, so it is asserted
        // rather than assumed.
        let (r, _) = x.log2_1p(NE);
        assert!(
            r.abs().partial_cmp(x.abs()).0 == Some(Ordering::Greater),
            "log2_1p({label}) = {r} did not grow past |{label}|"
        );
    }
}

/// One digit off an exact family member is not exact: the classifier
/// must decline, and the kernel must then raise `INEXACT`. Guards the
/// bail sites against a classifier that over-claims.
#[test]
fn near_misses_are_declined() {
    for label in [
        // Positive side: c + 1 not a power of two, and the mod 10
        // bail (a stripped exponent above zero).
        "2", "4", "5", "6", "8", "9", "30", "1000",
        // Fractional positives: 1 + x is never an integer.
        "0.5", "1.5", "3.5",
        // Negative side: the wrong coefficient at the right exponent.
        "-0.4", "-0.6", "-0.74", "-0.76", "-0.874", "-0.876",
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
    // The family ceiling from the other side: `2^112` is itself
    // representable at 34 digits, but `1 + 2^112` is no power of two,
    // so the classifier declines and the kernel raises INEXACT.
    let ceiling = parse("5192296858534827628530496329220096");
    for rm in ALL {
        let (_, st) = ceiling.log2_1p(rm);
        assert!(
            st.inexact(),
            "log2_1p(2^112) [{rm:?}] is irrational: {st:?}"
        );
    }
}
