//! Property sweep for `Decimal128::log2_1p` (IEEE 754-2019 §9.2
//! `log2p1`; ADR-0059 Track D).
//!
//! Oracle free by design: the correctly rounded values live in the
//! frozen Arb corpus (`tests/vectors/transcend/log2p1.txt`) and the
//! exact family lives in `tests/transcend_exact_log2p1.rs`. What a
//! random sweep adds is the structural contract those two cannot
//! reach input by input:
//!
//! * the domain map (finite `x > −1` gives a finite result; `x ≤ −1`
//!   gives the §9.2.1 special);
//! * flag honesty as an *equivalence*, `INEXACT` set exactly when the
//!   input is outside the exact family. The family membership test
//!   here is built independently of the kernel's classifier, from the
//!   two closed forms `2^k − 1` and `−(10^m − 5^m)·10^−m`, so a
//!   classifier that over-claims or under-claims on a random input
//!   fails here;
//! * mode coherence, the `NearestEven` value one representable step
//!   at most below the `TowardPositive` value;
//! * monotonicity, since `log2(1 + x)` is strictly increasing on its
//!   whole domain.

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
/// forms rather than from the kernel's classifier: `x = 2^k − 1` for
/// `1 ≤ k ≤ 112` and `x = −(10^m − 5^m)·10^−m` for `1 ≤ m ≤ 34`.
fn exact_family() -> Vec<Decimal128> {
    let mut v = Vec::new();
    for k in 1u32..=112 {
        v.push(parse(&format!("{}", (1u128 << k) - 1)));
    }
    for m in 1u32..=34 {
        v.push(parse(&format!("-{}e-{m}", 10u128.pow(m) - 5u128.pow(m))));
    }
    v
}

fn is_exact_family(x: Decimal128, family: &[Decimal128]) -> bool {
    family.iter().any(|&f| eq(x, f))
}

/// Every structural assertion for one in-domain input.
fn check_in_domain(x: Decimal128, family: &[Decimal128]) {
    let exact = is_exact_family(x, family);
    let mut ne_value = None;
    let mut tp_value = None;
    for rm in ALL {
        let (r, st) = x.log2_1p(rm);
        assert!(
            !r.is_nan() && !r.is_infinite(),
            "log2_1p({x}) [{rm:?}] left the finite range: {r}"
        );
        assert!(
            st.inexact() != exact,
            "log2_1p({x}) [{rm:?}] flagged {st:?} while exact-family membership is {exact}"
        );
        if !st.inexact() {
            assert_eq!(st, Status::OK, "log2_1p({x}) [{rm:?}] exact but {st:?}");
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
        "log2_1p({x}): NearestEven {ne_v} is more than one step below TowardPositive {tp_v}"
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// Positive inputs across sixty decades either side of one.
    #[test]
    fn log2_1p_positive_domain_contract(
        coef_bits in 1u128..=u128::MAX,
        exp in -60i32..=60,
    ) {
        let coef = coef_bits % 10u128.pow(34);
        if coef == 0 { return Ok(()); }
        let family = exact_family();
        check_in_domain(parse(&format!("{coef}e{exp}")), &family);
    }

    /// Negative inputs pinned inside `(−1, 0)`: scaling by
    /// `10^(−digits − j)` keeps `|x| < 1`, which is the whole
    /// in-domain negative range.
    #[test]
    fn log2_1p_negative_domain_contract(
        coef_bits in 1u128..=u128::MAX,
        j in 0i32..=40,
    ) {
        let coef = coef_bits % 10u128.pow(34);
        if coef == 0 { return Ok(()); }
        let digits = i32::try_from(coef.to_string().len()).unwrap();
        let x = parse(&format!("-{coef}e{}", -digits - j));
        assert!(
            x.partial_cmp(Decimal128::NEG_ONE).0 == Some(Ordering::Greater)
                && x.partial_cmp(Decimal128::ZERO).0 == Some(Ordering::Less),
            "generator escaped (-1, 0): {x}"
        );
        let family = exact_family();
        check_in_domain(x, &family);
    }

    /// Below `−1` the operation is a domain error in every mode
    /// (§9.2.1).
    #[test]
    fn log2_1p_below_minus_one_is_invalid(
        coef_bits in 1u128..=u128::MAX,
        exp in 0i32..=40,
    ) {
        let coef = coef_bits % 10u128.pow(34);
        if coef == 0 { return Ok(()); }
        // `-(1 + coef·10^exp)` is strictly below −1 for every draw.
        let x = parse(&format!("-1.5e{exp}"));
        let y = parse(&format!("-{coef}e{}", exp + 1));
        for candidate in [x, y] {
            for rm in ALL {
                let (r, st) = candidate.log2_1p(rm);
                assert!(
                    r.is_nan() && st.invalid(),
                    "log2_1p({candidate}) [{rm:?}] = {r} {st:?}, want NaN + INVALID"
                );
            }
        }
    }

    /// `log2(1 + x)` is strictly increasing, so the delivered values
    /// are non-decreasing under any single rounding direction.
    #[test]
    fn log2_1p_is_monotone(
        a_bits in 1u128..=u128::MAX,
        b_bits in 1u128..=u128::MAX,
        exp_a in -40i32..=40,
        exp_b in -40i32..=40,
    ) {
        let (ca, cb) = (a_bits % 10u128.pow(34), b_bits % 10u128.pow(34));
        if ca == 0 || cb == 0 { return Ok(()); }
        let p = parse(&format!("{ca}e{exp_a}"));
        let q = parse(&format!("{cb}e{exp_b}"));
        let (lo, hi) = match p.partial_cmp(q).0 {
            Some(Ordering::Greater) => (q, p),
            _ => (p, q),
        };
        for rm in ALL {
            let (rl, _) = lo.log2_1p(rm);
            let (rh, _) = hi.log2_1p(rm);
            assert!(
                rl.partial_cmp(rh).0 != Some(Ordering::Greater),
                "log2_1p [{rm:?}] not monotone: {lo} -> {rl} but {hi} -> {rh}"
            );
        }
    }
}

/// Monotonicity across the sign change, on a fixed deterministic grid
/// spanning the negative branch, zero, and the positive branch. A
/// sampled grid rather than a random pair, so the sign seam is
/// covered every run.
#[test]
fn monotone_across_the_sign_seam() {
    let mut grid: Vec<Decimal128> = Vec::new();
    for m in (1u32..=34).rev() {
        grid.push(parse(&format!("-{}e-{m}", 10u128.pow(m) - 5u128.pow(m))));
    }
    for e in [-40i32, -20, -10, -3, -1] {
        grid.push(parse(&format!("-1e{e}")));
    }
    grid.push(Decimal128::ZERO);
    for e in [-40i32, -20, -10, -3, -1, 0, 1, 10, 40] {
        grid.push(parse(&format!("1e{e}")));
    }
    for k in 1u32..=112 {
        grid.push(parse(&format!("{}", (1u128 << k) - 1)));
    }
    grid.sort_by(|a, b| a.partial_cmp(*b).0.expect("finite grid"));
    for rm in ALL {
        for pair in grid.windows(2) {
            let (rl, _) = pair[0].log2_1p(rm);
            let (rh, _) = pair[1].log2_1p(rm);
            assert!(
                rl.partial_cmp(rh).0 != Some(Ordering::Greater),
                "log2_1p [{rm:?}] not monotone across the seam: {} -> {rl} but {} -> {rh}",
                pair[0],
                pair[1]
            );
        }
    }
}
