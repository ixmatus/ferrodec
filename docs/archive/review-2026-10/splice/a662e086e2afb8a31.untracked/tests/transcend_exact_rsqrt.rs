//! Exact-result, tie, special-value, and quantum gate for
//! `Decimal128::rsqrt` (IEEE 754-2019 §9.2 `rSqrt`; ADR-0059 Track D
//! group D3, under the ADR-0060 phase gate).
//!
//! The classifier `ferrodec_transcend::exact::rsqrt_exact_input` claims
//! a complete boundary set. Writing the stripped input as
//! `x = 2^A · 5^B · s` with `gcd(s, 10) = 1`, `1/√x` is rational iff
//! `s = 1` and both `A` and `B` are even, and the value is then the
//! pure power `2^−A/2 · 5^−B/2` folded to a decimal — a power of five
//! over a power of ten when `A ≥ B`, a power of two over one when
//! `A < B`. Everything else leaves a `√2`, `√5`, `√10`, or `√s` factor
//! and is irrational, hence neither exact nor a nearest mode tie. This
//! file is the claim's witness at 34 digits.
//!
//! ## The ties, and why they are only witnessed here
//!
//! Powers of five end in 5, so a `5^d` of exactly `PRECISION + 1`
//! digits IS a nearest mode midpoint. At `Decimal128` two `d` reach
//! that width — 49 and 50 — from the representable inputs `2^98` and
//! `2^100`, the same two-per-format pattern `exp2`'s classifier records
//! at `exp2(-49)` / `exp2(-50)`. The corpus generator cannot carry
//! either — a certified ball around an exact midpoint never becomes
//! decisive — so the literal assertions below are the ties' only
//! witnesses. Each is checked twice: against the value the derivation
//! predicts, and against the format's own decimal parser applied to the
//! exact midpoint string, which rounds it under the same mode through a
//! path sharing no code with the kernel.
//!
//! ## The §9.2.2 quantum delta (recorded, not repaired)
//!
//! §9.2.2 states the preferred exponent as `Q(rSqrt(x)) = −⌊Q(x)/2⌋`.
//! The delivered quantum is *not* that: every exact classifier in
//! `ferrodec-transcend` packs through `exact::pack_value`, which asks
//! the format rounder for the preferred quantum **0**, so the delivery
//! is the cohort member whose quantum sits as close to zero as the
//! format's precision allows. The two agree only when the §9.2.2
//! exponent is already the closest-to-zero reachable one
//! (`rsqrt(1) = 1`, `rsqrt(1E+72) = 1E-36`); elsewhere the delivered
//! quantum is strictly below it, by 1 at `rsqrt(4)` and by 33 at
//! `rsqrt(1E-100)`. Value and flags are unaffected — the cohort members
//! are numerically equal — and the fix, if the lane wants one, is a
//! cross-function decision about `pack_value`'s `q_preferred` argument
//! rather than anything local to `rSqrt`.
//! [`quantum_pins_record_the_section_9_2_2_delta`] pins both columns so
//! neither can drift unobserved.

#![cfg(feature = "exp-log")]

use core::cmp::Ordering;
use ferrodec::{Decimal128, RoundingMode, Status};

const NE: RoundingMode = RoundingMode::NearestEven;
const NA: RoundingMode = RoundingMode::NearestAway;
const TZ: RoundingMode = RoundingMode::TowardZero;
const TP: RoundingMode = RoundingMode::TowardPositive;
const TN: RoundingMode = RoundingMode::TowardNegative;
const ALL: [RoundingMode; 5] = [NE, NA, TZ, TP, TN];

/// The format's two tie rows. `5^d` spans exactly `PRECISION + 1 = 35`
/// digits for `d = 49` and `d = 50` and ends in 5 either way, so
/// `rsqrt(2^98)` and `rsqrt(2^100)` are both exact nearest mode
/// midpoints — the same two-per-format pattern `exp2`'s classifier
/// records at `exp2(-49)` / `exp2(-50)`. Each row is
/// `(d, input = 2^2d, midpoint, smaller neighbour, larger neighbour)`.
const TIE_ROWS: [(u32, &str, &str, &str, &str); 2] = [
    (
        49,
        "316912650057057350374175801344",
        "1.7763568394002504646778106689453125E-15",
        "1.776356839400250464677810668945312E-15",
        "1.776356839400250464677810668945313E-15",
    ),
    (
        50,
        "1267650600228229401496703205376",
        "8.8817841970012523233890533447265625E-16",
        "8.881784197001252323389053344726562E-16",
        "8.881784197001252323389053344726563E-16",
    ),
];

fn parse(s: &str) -> Decimal128 {
    Decimal128::parse_str(s, NE)
        .unwrap_or_else(|_| panic!("test literal parses: {s:?}"))
        .0
}

/// The format's own decimal parser under an explicit rounding mode: the
/// independent oracle for a value spelled out exactly, sharing no code
/// with the transcendental kernel.
fn parse_rm(s: &str, rm: RoundingMode) -> Decimal128 {
    Decimal128::parse_str(s, rm)
        .unwrap_or_else(|_| panic!("test literal parses: {s:?}"))
        .0
}

fn eq(got: Decimal128, want: Decimal128) -> bool {
    got.partial_cmp(want).0 == Some(Ordering::Equal)
}

/// The stored quantum exponent of a finite datum.
fn quantum(d: Decimal128) -> i32 {
    i32::from(d.decode().expect("finite datum decodes").exponent)
}

fn assert_exact(got: (Decimal128, Status), want: Decimal128, label: &str) {
    let (r, st) = got;
    assert!(eq(r, want), "{label}: got {r}, want {want}");
    assert_eq!(
        st,
        Status::OK,
        "{label}: exact result must be OK, got {st:?}"
    );
    assert!(!st.inexact(), "{label}: §7.5 forbids INEXACT here");
}

/// IEEE 754-2019 §9.2.1, every row, every rounding direction. The
/// standard's own words for the two positive rows: "rSqrt(+∞) is +0
/// with no exception" and "rSqrt(±0) is ±∞ and signals the
/// divideByZero exception".
#[test]
fn specials_per_section_9_2_1() {
    for rm in ALL {
        // rSqrt(+∞) is +0 with no exception.
        let (r, st) = Decimal128::INFINITY.rsqrt(rm);
        assert!(
            r.is_zero() && !r.is_sign_negative(),
            "rsqrt(+inf) [{rm:?}] = {r}"
        );
        assert_eq!(st, Status::OK, "rsqrt(+inf) [{rm:?}] status {st:?}");

        // rSqrt(±0) is ±∞ and signals divideByZero — sign preserved.
        let (r, st) = Decimal128::ZERO.rsqrt(rm);
        assert!(
            r.is_infinite() && !r.is_sign_negative(),
            "rsqrt(+0) [{rm:?}] = {r}"
        );
        assert!(st.div_by_zero(), "rsqrt(+0) [{rm:?}] status {st:?}");
        assert!(!st.inexact(), "rsqrt(+0) [{rm:?}] is not inexact");

        let (r, st) = Decimal128::NEG_ZERO.rsqrt(rm);
        assert!(
            r.is_infinite() && r.is_sign_negative(),
            "rsqrt(-0) [{rm:?}] = {r}"
        );
        assert!(st.div_by_zero(), "rsqrt(-0) [{rm:?}] status {st:?}");

        // Every other negative operand is a domain error.
        for label in ["-4", "-1e-6100", "-9.999999e6144", "-0.5"] {
            let (r, st) = parse(label).rsqrt(rm);
            assert!(r.is_nan(), "rsqrt({label}) [{rm:?}] = {r}, want NaN");
            assert!(st.invalid(), "rsqrt({label}) [{rm:?}] status {st:?}");
        }
        let (r, st) = Decimal128::NEG_INFINITY.rsqrt(rm);
        assert!(r.is_nan(), "rsqrt(-inf) [{rm:?}] = {r}");
        assert!(st.invalid(), "rsqrt(-inf) [{rm:?}] status {st:?}");

        // NaN propagation; sNaN raises INVALID and quiets.
        let (r, st) = Decimal128::NAN.rsqrt(rm);
        assert!(r.is_nan() && st.is_ok(), "rsqrt(NaN) [{rm:?}] = {r} {st:?}");
        let (r, st) = Decimal128::SIGNALING_NAN.rsqrt(rm);
        assert!(
            r.is_nan() && st.invalid(),
            "rsqrt(sNaN) [{rm:?}] = {r} {st:?}"
        );
    }
}

/// The named exact values the public rustdoc quotes, pinned so the
/// documentation and the kernel cannot drift apart. Every one is exact
/// in every rounding direction with status `OK` (§7.5).
#[test]
fn documented_exact_values() {
    let cases: [(&str, &str); 14] = [
        ("4", "0.5"),
        ("0.04", "5"),
        ("6.25", "0.4"),
        ("0.25", "2"),
        ("1", "1"),
        ("100", "0.1"),
        ("0.0001", "100"),
        ("16", "0.25"),
        ("0.0625", "4"),
        ("1024", "0.03125"),
        ("64", "0.125"),
        ("0.16", "2.5"),
        ("2.56", "0.625"),
        ("1E+72", "1E-36"),
    ];
    for (input, want) in cases {
        for rm in ALL {
            assert_exact(
                parse(input).rsqrt(rm),
                parse(want),
                &format!("rsqrt({input}) [{rm:?}]"),
            );
        }
    }
}

/// The power-of-ten family across the *whole* exponent range, both
/// directions: `rsqrt(1E-2k) = 1E+k` for every `k` the input side
/// admits (`2k ≤ 6176`, the subnormal floor) and `rsqrt(1E+2k) = 1E-k`
/// for every `k` the input side admits (`2k ≤ 6144`). Both result
/// columns stay inside the normal range with thousands of decades to
/// spare, which is the executable form of the module's "no subnormal
/// edge" claim.
#[test]
fn power_of_ten_family_across_the_exponent_range() {
    for k in 1..=3088i32 {
        let x = parse(&format!("1E-{}", 2 * k));
        let want = parse(&format!("1E+{k}"));
        let (r, st) = x.rsqrt(NE);
        assert!(eq(r, want), "rsqrt(1E-{}) = {r}, want 1E+{k}", 2 * k);
        assert_eq!(st, Status::OK, "rsqrt(1E-{}) status {st:?}", 2 * k);
        assert!(
            !r.is_subnormal() && r.is_finite(),
            "rsqrt(1E-{}) left the normal range: {r}",
            2 * k
        );
    }
    for k in 1..=3072i32 {
        let x = parse(&format!("1E+{}", 2 * k));
        let want = parse(&format!("1E-{k}"));
        let (r, st) = x.rsqrt(NE);
        assert!(eq(r, want), "rsqrt(1E+{}) = {r}, want 1E-{k}", 2 * k);
        assert_eq!(st, Status::OK, "rsqrt(1E+{}) status {st:?}", 2 * k);
        assert!(
            !r.is_subnormal() && r.is_finite(),
            "rsqrt(1E+{}) left the normal range: {r}",
            2 * k
        );
    }
    // The range edges, in all five directions.
    for (input, want) in [("1E-6176", "1E+3088"), ("1E+6144", "1E-3072")] {
        for rm in ALL {
            assert_exact(
                parse(input).rsqrt(rm),
                parse(want),
                &format!("rsqrt({input}) [{rm:?}]"),
            );
        }
    }
}

/// The two exact branches, exhaustively over the widths this format
/// admits: `rsqrt(2^(2d)) = 5^d · 10^-d` for every `d` whose `5^d` fits
/// `PRECISION` digits (`d ≤ 48`), and the mirror
/// `rsqrt(5^(2d)) = 2^d · 10^-d` for every `d` whose input fits
/// (`d ≤ 24`). The expected value is built independently in `u128`
/// integer arithmetic, never from the kernel, and where `r²` is itself
/// representable the format's own multiplication closes the loop
/// exactly: `x · r² = 1` with no `INEXACT` anywhere in the chain.
#[test]
fn power_families_exhaustive_at_this_width() {
    let one = parse("1");
    // `5^d` branch: input `2^(2d)` (29 digits at the widest `d`),
    // value exact while `5^d ≤ 10^34`, i.e. `d ≤ 48`; `d = 49` is the
    // tie row above.
    for d in 0..=48u32 {
        let input = 1u128 << (2 * d);
        let coef = 5u128.pow(d);
        let x = parse(&format!("{input}"));
        let want = parse(&format!("{coef}E-{d}"));
        for rm in ALL {
            assert_exact(x.rsqrt(rm), want, &format!("rsqrt(2^{}) [{rm:?}]", 2 * d));
        }
        // `r² = 5^(2d)·10^-2d` is representable while `2d ≤ 48`; there
        // the round trip is exact end to end.
        if 2 * d <= 48 {
            assert_exact_round_trip(x, want, one, &format!("2^{}", 2 * d));
        }
    }
    // `2^d` branch: input `5^(2d)`, representable while `5^(2d) < 10^34`
    // (`d ≤ 24`); `r² = 2^(2d)·10^-2d` is always representable there.
    for d in 0..=24u32 {
        let input = 5u128.pow(2 * d);
        let coef = 1u128 << d;
        let x = parse(&format!("{input}"));
        let want = parse(&format!("{coef}E-{d}"));
        for rm in ALL {
            assert_exact(x.rsqrt(rm), want, &format!("rsqrt(5^{}) [{rm:?}]", 2 * d));
        }
        assert_exact_round_trip(x, want, one, &format!("5^{}", 2 * d));
    }
}

/// `x · r² = 1` through the format's own multiplication, exactly and
/// with clean flags. The independent witness for an exact `rSqrt`
/// delivery, in the shape `exact::rsqrt_is_exact` carries in the
/// kernel crate's unit tests.
fn assert_exact_round_trip(x: Decimal128, r: Decimal128, one: Decimal128, label: &str) {
    let (sq, st_sq) = r.mul(r, NE);
    assert!(!st_sq.inexact(), "rsqrt({label})² is not exact: {st_sq:?}");
    let (round_trip, st) = x.mul(sq, NE);
    assert!(
        eq(round_trip, one) && !st.inexact(),
        "{label} · rsqrt({label})² = {round_trip} ({st:?}), want exactly 1"
    );
}

/// Both `Decimal128` tie rows, literal, every mode. `NearestEven`
/// picks the even neighbour (the smaller-magnitude one in both rows),
/// `NearestAway` and `TowardPositive` the larger, `TowardZero` and
/// `TowardNegative` the smaller. §7.5: a tie delivery drops a nonzero
/// digit in every mode, so `INEXACT` is raised on all five.
#[test]
fn tie_rows_literal() {
    for (d, input, mid, down_s, up_s) in TIE_ROWS {
        let x = parse(input);
        let up = parse(up_s);
        let down = parse(down_s);
        for (rm, want) in [(NE, down), (NA, up), (TZ, down), (TP, up), (TN, down)] {
            let (r, st) = x.rsqrt(rm);
            assert!(
                eq(r, want),
                "rsqrt(2^{}) [{rm:?}]: got {r}, want {want}",
                2 * d
            );
            assert!(
                st.inexact(),
                "rsqrt(2^{}) [{rm:?}] drops a nonzero digit: {st:?}",
                2 * d
            );
            // The format's own parser on the exact midpoint string is
            // the independent oracle: same value, same mode, no kernel
            // code in the path.
            assert!(
                eq(r, parse_rm(mid, rm)),
                "rsqrt(2^{}) [{rm:?}] disagrees with parse_str on the midpoint",
                2 * d
            );
        }
        // The literals really are the midpoint and its neighbours,
        // proved in `u128` integer arithmetic — the only arena where
        // the statement is exact, since a 35-digit midpoint is by
        // definition not representable at 34 digits.
        let mid_i = 5u128.pow(d);
        assert_eq!(
            mid_i.to_string(),
            digits_of(mid),
            "the tie literal is 5^{d}"
        );
        assert_eq!(mid_i % 10, 5, "a midpoint's last digit is 5");
        assert_eq!(
            mid_i.to_string().len(),
            35,
            "5^{d} spans PRECISION + 1 digits"
        );
        let down_i: u128 = digits_of(down_s).parse().expect("neighbour is a u128");
        let up_i: u128 = digits_of(up_s).parse().expect("neighbour is a u128");
        assert_eq!(
            mid_i - down_i * 10,
            up_i * 10 - mid_i,
            "the {d} row sits exactly halfway between the neighbours"
        );
        // And the input is representable: `2^98` spans 30 digits,
        // `2^100` spans 31.
        assert_eq!(
            (1u128 << (2 * d)).to_string(),
            input,
            "the tie input literal is 2^{}",
            2 * d
        );
    }
}

/// The significant digits of a `d.dddEsnn` literal, as a bare string.
fn digits_of(literal: &str) -> String {
    literal
        .split('E')
        .next()
        .expect("scientific literal")
        .replace('.', "")
}

/// Quantum pins. Column one is what the kernel delivers, column two is
/// §9.2.2's preferred exponent `−⌊Q(x)/2⌋`. Both are pinned: the
/// delivery so it cannot drift, the preferred value so the recorded
/// delta stays visible (see the module header).
#[test]
fn quantum_pins_record_the_section_9_2_2_delta() {
    // (input, delivered quantum, §9.2.2 preferred quantum)
    let rows: [(&str, i32, i32); 11] = [
        ("1", 0, 0),
        ("1E+72", -36, -36),
        ("4", -1, 0),
        ("4.0", -1, 1),
        ("0.04", 0, 1),
        ("6.25", -1, 1),
        ("0.25", 0, 1),
        ("100", -1, 0),
        ("16", -2, 0),
        ("1024", -5, 0),
        ("1E-100", 17, 50),
    ];
    for (label, delivered, preferred) in rows {
        let x = parse(label);
        // §9.2.2's own formula, recomputed from the input's quantum.
        let qx = quantum(x);
        let want_preferred = -qx.div_euclid(2);
        assert_eq!(
            want_preferred, preferred,
            "{label}: the §9.2.2 preferred exponent is -floor({qx}/2)"
        );
        for rm in ALL {
            let (r, _) = x.rsqrt(rm);
            assert_eq!(
                quantum(r),
                delivered,
                "rsqrt({label}) [{rm:?}] delivered quantum drifted"
            );
        }
    }
    // Inexact deliveries are unaffected by the delta: the quantum there
    // is forced by the precision, not by any preference.
    let (r, st) = parse("2").rsqrt(NE);
    assert!(st.inexact());
    assert_eq!(quantum(r), -34, "an inexact rsqrt fills the precision");
}

/// Directed-mode correctness on non-exact neighbours of exact cases.
/// `rSqrt` is strictly decreasing, so one ulp above an exact input puts
/// the true value just below the exact result and one ulp below puts it
/// just above. The two directed modes must bracket it, one ulp apart,
/// and the exact value itself must never acquire a spurious `INEXACT`.
#[test]
fn directed_modes_bracket_exact_neighbours() {
    // (neighbour input, the exact result it sits beside, side)
    let cases: [(&str, &str, bool); 4] = [
        // Just above 4 → just below 0.5.
        ("4.000000000000000000000000000000001", "0.5", false),
        // Just below 4 → just above 0.5.
        ("3.999999999999999999999999999999999", "0.5", true),
        // Just above 0.04 → just below 5.
        ("0.04000000000000000000000000000000001", "5", false),
        // Just below 0.0625 → just above 4.
        ("0.06249999999999999999999999999999999", "4", true),
    ];
    for (label, exact, above) in cases {
        let x = parse(label);
        let anchor = parse(exact);
        let (up, st_up) = x.rsqrt(TP);
        let (down, st_down) = x.rsqrt(TZ);
        for st in [st_up, st_down] {
            assert!(st.inexact(), "rsqrt({label}) must be INEXACT: {st:?}");
        }
        // TowardZero and TowardNegative agree on a positive result.
        let (tn, _) = x.rsqrt(TN);
        assert!(eq(tn, down), "rsqrt({label}): TowardNegative ≠ TowardZero");
        // The bracket is one ulp wide and straddles the true value.
        assert!(
            down.partial_cmp(up).0 == Some(Ordering::Less),
            "rsqrt({label}): directed modes did not bracket"
        );
        let (gap, _) = up.sub(down, NE);
        let (ulp_at, _) = down.ulp().partial_cmp(gap);
        assert_eq!(
            ulp_at,
            Some(Ordering::Equal),
            "rsqrt({label}): the bracket is not one ulp wide"
        );
        // And it sits on the correct side of the neighbouring exact
        // value: strictly below it above the anchor, strictly above it
        // below the anchor.
        if above {
            assert!(
                down.partial_cmp(anchor).0 == Some(Ordering::Greater) || eq(down, anchor),
                "rsqrt({label}) = {down} should sit at or above {anchor}"
            );
        } else {
            assert!(
                up.partial_cmp(anchor).0 == Some(Ordering::Less) || eq(up, anchor),
                "rsqrt({label}) = {up} should sit at or below {anchor}"
            );
        }
    }
    // The exact values themselves keep clean flags in every mode.
    for label in ["4", "0.04", "0.0625"] {
        for rm in ALL {
            let (_, st) = parse(label).rsqrt(rm);
            assert!(
                !st.inexact(),
                "rsqrt({label}) [{rm:?}] must not be INEXACT: {st:?}"
            );
        }
    }
}

/// A deterministic sweep across the exponent range: every result must
/// satisfy `x · rsqrt(x)² ≈ 1` within a coarse bound, stay finite and
/// normal, and — since every coefficient below carries a prime factor
/// coprime to ten, which the classifier's `s ≠ 1` bail rules out — be
/// `INEXACT` in every rounding direction.
#[test]
fn sweep_round_trips_and_flags_inexact() {
    // Coefficients with a factor of 3 or 7: `s ≠ 1`, so provably
    // irrational results.
    let coefficients: [&str; 8] = [
        "3",
        "7",
        "21",
        "1.5",
        "2.7",
        "9.3",
        "123456789",
        "6.999999999999999999999999999999999",
    ];
    let tolerance = parse("1E-30");
    let one = parse("1");
    let mut checked = 0usize;
    for coef in coefficients {
        for exp in (-6100i32..=6100).step_by(97) {
            let x = parse(&format!("{coef}E{exp}"));
            if x.is_zero() || !x.is_finite() {
                continue;
            }
            let (r, st) = x.rsqrt(NE);
            assert!(
                r.is_finite() && !r.is_zero() && !r.is_subnormal(),
                "rsqrt({coef}E{exp}) = {r} left the normal range"
            );
            assert!(st.inexact(), "rsqrt({coef}E{exp}) must be INEXACT: {st:?}");
            for rm in ALL {
                let (_, st) = x.rsqrt(rm);
                assert!(st.inexact(), "rsqrt({coef}E{exp}) [{rm:?}]: {st:?}");
            }
            // x · r² ≈ 1. Computed in the format itself, so the bound
            // absorbs three format roundings (~3e-33 relative) with
            // three orders to spare.
            let (sq, _) = r.mul(r, NE);
            let (round_trip, _) = x.mul(sq, NE);
            let (diff, _) = round_trip.sub(one, NE);
            assert!(
                diff.abs().partial_cmp(tolerance).0 == Some(Ordering::Less),
                "rsqrt({coef}E{exp}): x·r² = {round_trip}, off by {diff}"
            );
            checked += 1;
        }
    }
    assert!(checked > 800, "sweep covered only {checked} inputs");
}

/// Cohorts of one exact input classify identically: the decision is
/// made on the stripped form, so `4`, `4.0`, and `4.000…0` are one
/// value and one answer (their *quanta* differ — see the quantum pins —
/// but the value and the flags do not).
#[test]
fn cohorts_of_an_exact_input_classify_alike() {
    for (label, want) in [
        ("4", "0.5"),
        ("4.0", "0.5"),
        ("4.000000000000000000000000000000000", "0.5"),
        ("4E+2", "0.05"),
        ("0.04", "5"),
        ("0.0400", "5"),
        ("400E-4", "5"),
        ("0.000004000", "500"),
    ] {
        for rm in ALL {
            assert_exact(
                parse(label).rsqrt(rm),
                parse(want),
                &format!("rsqrt({label}) [{rm:?}]"),
            );
        }
    }
}

/// One step off an exact family member is not exact: the classifier
/// must decline and the kernel must raise `INEXACT`. Guards the three
/// bail sites — a surviving factor coprime to ten, an odd power of two,
/// an odd power of five — against a classifier that over-claims.
#[test]
fn near_misses_are_declined() {
    for label in [
        // `s ≠ 1`: a factor of 3 or 7 survives the 2/5 stripping.
        "3", "9", "36", "0.09", "49", "1.44", // 1.44 = 144e-2 = 2^4·3^2·10^-2
        // Odd power of two.
        "2", "8", "32", "0.5", "0.125", // Odd power of five.
        "5", "125", "0.2", "0.008", // Odd power of ten.
        "10", "1000", "0.1", "1E-6175",
    ] {
        let x = parse(label);
        for rm in ALL {
            let (_, st) = x.rsqrt(rm);
            assert!(st.inexact(), "rsqrt({label}) [{rm:?}] is not exact: {st:?}");
        }
    }
}
