//! Property sweep for `Decimal128::exp2_m1` (IEEE 754-2019 §9.2
//! `exp2m1`; ADR-0059 Track D).
//!
//! Oracle free by design: the correctly rounded values live in the
//! frozen Arb corpus (`tests/vectors/transcend/exp2m1.txt`), and the
//! exact family and the two ties live in
//! `tests/transcend_exact_exp2m1.rs`. What a random sweep adds is the
//! structural contract those two cannot reach input by input:
//!
//! * the range map (no finite input produces a NaN; the result stays
//!   finite unless `OVERFLOW` is raised, and never falls below `−1`);
//! * flag honesty as an *equivalence*, `INEXACT` set exactly when the
//!   input is outside the exact family. The family membership test
//!   here is built independently of the kernel's classifier, from the
//!   two closed forms `2^n − 1` and `−(10^m − 5^m)·10^−m`, so a
//!   classifier that over-claims or under-claims on a random input
//!   fails here;
//! * mode coherence, the `NearestEven` value one representable step at
//!   most below the `TowardPositive` value;
//! * monotonicity, since `2^x − 1` is strictly increasing everywhere;
//! * agreement with the separately shipped `exp2` kernel on the band
//!   where `2^x ⊖ 1` does not cancel, which is a cross-check between
//!   two independently derived pipelines rather than against a
//!   reference.

#![cfg(feature = "exp-log")]

use core::cmp::Ordering;
use ferrodec::{Decimal128, RoundingMode, Status};
use proptest::prelude::*;

const NE: RoundingMode = RoundingMode::NearestEven;
const NA: RoundingMode = RoundingMode::NearestAway;
const TZ: RoundingMode = RoundingMode::TowardZero;
const TP: RoundingMode = RoundingMode::TowardPositive;
const TN: RoundingMode = RoundingMode::TowardNegative;
const ALL: [RoundingMode; 5] = [NE, NA, TZ, TP, TN];

fn parse(s: &str) -> Decimal128 {
    Decimal128::parse_str(s, NE)
        .unwrap_or_else(|_| panic!("test literal parses: {s:?}"))
        .0
}

fn eq(a: Decimal128, b: Decimal128) -> bool {
    a.partial_cmp(b).0 == Some(Ordering::Equal)
}

/// The whole exact family at 34 digits, rebuilt from the two closed
/// forms rather than from the kernel's classifier: `x = n` for
/// `1 ≤ n ≤ 112` and `x = −m` for `1 ≤ m ≤ 34`. The ties (`n = 116`,
/// `m = 35`) are deliberately absent: their values are midpoints, not
/// representable, so they must carry `INEXACT`.
fn exact_family_inputs() -> Vec<Decimal128> {
    let mut v = Vec::new();
    for n in 1i32..=112 {
        v.push(parse(&format!("{n}")));
    }
    for m in 1i32..=34 {
        v.push(parse(&format!("-{m}")));
    }
    v
}

fn is_exact_family(x: Decimal128, family: &[Decimal128]) -> bool {
    family.iter().any(|&f| eq(x, f))
}

/// Every structural assertion for one finite input.
fn check_finite(x: Decimal128, family: &[Decimal128]) {
    let exact = is_exact_family(x, family);
    let mut ne_value = None;
    let mut tp_value = None;
    for rm in ALL {
        let (r, st) = x.exp2_m1(rm);
        assert!(!r.is_nan(), "exp2_m1({x}) [{rm:?}] produced a NaN");
        assert!(
            !r.is_infinite() || st.overflow(),
            "exp2_m1({x}) [{rm:?}] went infinite without OVERFLOW: {st:?}"
        );
        if !r.is_infinite() {
            assert!(
                r.partial_cmp(Decimal128::NEG_ONE).0 != Some(Ordering::Less),
                "exp2_m1({x}) [{rm:?}] = {r} fell below -1"
            );
        }
        assert!(
            st.inexact() != exact,
            "exp2_m1({x}) [{rm:?}] flagged {st:?} while exact-family membership is {exact}"
        );
        if !st.inexact() {
            assert_eq!(st, Status::OK, "exp2_m1({x}) [{rm:?}] exact but {st:?}");
        }
        if rm == NE {
            ne_value = Some(r);
        }
        if rm == TP {
            tp_value = Some(r);
        }
    }
    // The nearest value is the upward one or its immediate neighbour
    // below: the two rounding directions bracket the same true value.
    let (ne_v, tp_v) = (ne_value.unwrap(), tp_value.unwrap());
    assert!(
        eq(ne_v, tp_v) || eq(ne_v, tp_v.next_down().0),
        "exp2_m1({x}): NearestEven {ne_v} is more than one step below TowardPositive {tp_v}"
    );
}

/// `0` when the two values are equal, `1` one representable step
/// apart, `2` for anything wider (the assertions only ever need the
/// distinction).
fn step_distance(a: Decimal128, b: Decimal128) -> u8 {
    if eq(a, b) {
        return 0;
    }
    if eq(a, b.next_up().0) || eq(a, b.next_down().0) {
        return 1;
    }
    if eq(a, b.next_up().0.next_up().0) || eq(a, b.next_down().0.next_down().0) {
        return 2;
    }
    3
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// Positive inputs across the whole non-overflowing band and past
    /// it, where the §7.4 saturation takes over.
    #[test]
    fn exp2_m1_positive_contract(
        coef_bits in 1u128..=u128::MAX,
        exp in -60i32..=60,
    ) {
        let coef = coef_bits % 10u128.pow(34);
        if coef == 0 { return Ok(()); }
        let family = exact_family_inputs();
        check_finite(parse(&format!("{coef}e{exp}")), &family);
    }

    /// Negative inputs, spanning the tiny band, the ordinary band, and
    /// the `−1` collapse band past `x ≈ −173`.
    #[test]
    fn exp2_m1_negative_contract(
        coef_bits in 1u128..=u128::MAX,
        exp in -60i32..=60,
    ) {
        let coef = coef_bits % 10u128.pow(34);
        if coef == 0 { return Ok(()); }
        let family = exact_family_inputs();
        check_finite(parse(&format!("-{coef}e{exp}")), &family);
    }

    /// `2^x − 1` is strictly increasing, so the delivered values are
    /// non-decreasing under any single rounding direction.
    #[test]
    fn exp2_m1_is_monotone(
        a_bits in 1u128..=u128::MAX,
        b_bits in 1u128..=u128::MAX,
        exp_a in -40i32..=40,
        exp_b in -40i32..=40,
        neg_a in any::<bool>(),
        neg_b in any::<bool>(),
    ) {
        let (ca, cb) = (a_bits % 10u128.pow(34), b_bits % 10u128.pow(34));
        if ca == 0 || cb == 0 { return Ok(()); }
        let sa = if neg_a { "-" } else { "" };
        let sb = if neg_b { "-" } else { "" };
        let p = parse(&format!("{sa}{ca}e{exp_a}"));
        let q = parse(&format!("{sb}{cb}e{exp_b}"));
        let (lo, hi) = match p.partial_cmp(q).0 {
            Some(Ordering::Greater) => (q, p),
            _ => (p, q),
        };
        for rm in ALL {
            let (rl, _) = lo.exp2_m1(rm);
            let (rh, _) = hi.exp2_m1(rm);
            assert!(
                rl.partial_cmp(rh).0 != Some(Ordering::Greater),
                "exp2_m1 [{rm:?}] not monotone: {lo} -> {rl} but {hi} -> {rh}"
            );
        }
    }

    /// On the band `1 ≤ x ≤ 5000` the subtraction `2^x ⊖ 1` loses
    /// nothing to cancellation (`2^x ≥ 2`), so composing the
    /// separately shipped `exp2` kernel with a format subtraction must
    /// land within two representable steps of `exp2_m1`: each side
    /// carries at most a half ULP of its own rounding, and the
    /// composed side one more from the subtraction. Two independently
    /// derived pipelines, cross-checked against each other rather than
    /// against a reference.
    #[test]
    fn exp2_m1_agrees_with_exp2_minus_one(
        milli in 1_000u32..=5_000_000,
    ) {
        let x = parse(&format!("{milli}e-3"));
        let (direct, _) = x.exp2_m1(NE);
        let (composed, _) = x.exp2(NE);
        let (composed, _) = composed.sub(Decimal128::ONE, NE);
        let d = step_distance(direct, composed);
        assert!(
            d <= 2,
            "exp2_m1({x}) = {direct} but exp2({x}) - 1 = {composed} ({d} steps apart)"
        );
    }
}

/// Monotonicity across the sign change, on a fixed deterministic grid
/// spanning the `−1` collapse band, the exact families, zero, and the
/// overflow band. A sampled grid rather than a random pair, so the
/// sign seam and both saturation edges are covered every run.
#[test]
fn monotone_across_the_sign_seam() {
    let mut grid: Vec<Decimal128> = Vec::new();
    for label in ["-9.999999e6144", "-1e100", "-1000", "-200", "-174", "-173"] {
        grid.push(parse(label));
    }
    for m in (1i32..=34).rev() {
        grid.push(parse(&format!("-{m}")));
    }
    for e in [-40i32, -20, -10, -3, -1] {
        grid.push(parse(&format!("-1e{e}")));
    }
    grid.push(Decimal128::ZERO);
    for e in [-40i32, -20, -10, -3, -1] {
        grid.push(parse(&format!("1e{e}")));
    }
    for n in 1i32..=116 {
        grid.push(parse(&format!("{n}")));
    }
    for label in ["1000", "20414", "20415", "30000", "1e100", "9.999999e6144"] {
        grid.push(parse(label));
    }
    grid.sort_by(|a, b| a.partial_cmp(*b).0.expect("finite grid"));
    for rm in ALL {
        for pair in grid.windows(2) {
            let (rl, _) = pair[0].exp2_m1(rm);
            let (rh, _) = pair[1].exp2_m1(rm);
            assert!(
                rl.partial_cmp(rh).0 != Some(Ordering::Greater),
                "exp2_m1 [{rm:?}] not monotone across the seam: {} -> {rl} but {} -> {rh}",
                pair[0],
                pair[1]
            );
        }
    }
}

/// The sign relation `sign(2^x − 1) = sign(x)`, on the band where the
/// result does not saturate. Cheap, exact, and independent of any
/// rounding argument: `2^x > 1` iff `x > 0`.
#[test]
fn sign_follows_the_argument() {
    for label in [
        "1e-6100", "1e-40", "0.5", "1", "2.5", "100", "1000", "20000",
    ] {
        let pos = parse(label);
        let neg = parse(&format!("-{label}"));
        for rm in ALL {
            let (r, _) = pos.exp2_m1(rm);
            assert!(
                r.is_zero() || !r.is_sign_negative(),
                "exp2_m1({label}) [{rm:?}] = {r} is negative"
            );
            let (r, _) = neg.exp2_m1(rm);
            assert!(
                r.is_zero() || r.is_sign_negative(),
                "exp2_m1(-{label}) [{rm:?}] = {r} is positive"
            );
        }
    }
}
