//! Special-value, exact-result, tie, range-disposition, and flag gate
//! for `Decimal128`'s `powi` (IEEE 754-2019 §9.2 `pown`; ADR-0059
//! Track D group D3, under ADR-0060).
//!
//! `pown` differs from `pow` in three ways this file exists to pin.
//! The exponent is an integer by type, so a negative base is legal
//! everywhere and there is no `INVALID` row for it, and there is no
//! infinite exponent row at all. The result `x^n` is rational for
//! *every* representable `x`, so the input-side classifier is a width
//! test rather than a rationality test and its exact family is
//! correspondingly enormous. And the kernel has two arms —
//! working-precision binary powering for `|n| ≤ 6`, `exp(n·ln|x|)`
//! beyond — whose seam is a correctness boundary fixed by ADR-0060's
//! Liouville floors, not a performance heuristic, so both sides of it
//! are checked against `pow` on the same inputs.
//!
//! The §9.2.1 table is transcribed row by row below, one test per row
//! group, in the order the standard states them.

#![cfg(feature = "pow")]

use ferrodec::{Decimal128, RoundingMode, Status};

const NE: RoundingMode = RoundingMode::NearestEven;
const NA: RoundingMode = RoundingMode::NearestAway;
const TZ: RoundingMode = RoundingMode::TowardZero;
const TP: RoundingMode = RoundingMode::TowardPositive;
const TN: RoundingMode = RoundingMode::TowardNegative;
const ALL: [RoundingMode; 5] = [NE, NA, TZ, TP, TN];

/// Largest decimal exponent of a representable `Decimal128`.
const EMAX: i32 = 6144;
/// Smallest decimal exponent of a representable `Decimal128`.
const ETINY: i32 = -6176;
/// The classifier's exponent window (`exact::POWI_EXPONENT_WINDOW`).
/// Past it the true value's logarithm exceeds 230,000, so the `exp`
/// gates own the disposition instead.
const WINDOW: i32 = 99_999;

fn parse(s: &str) -> Decimal128 {
    Decimal128::parse_str(s, NE)
        .unwrap_or_else(|_| panic!("literal parses: {s:?}"))
        .0
}

/// Value equality, cohort insensitive (the IEEE `compare`).
fn equal(a: Decimal128, b: Decimal128) -> bool {
    a.partial_cmp(b).0 == Some(core::cmp::Ordering::Equal)
}

// ---------------------------------------------------------------------------
// IEEE 754-2019 §9.2.1, row by row.

/// "pown(x, 0) is 1 if x is not a signaling NaN" — quiet NaN and the
/// infinities included, which is the one place NaN does not
/// propagate. A signaling NaN is named and excluded by that wording,
/// so it takes the general §7.2 rule: quieted payload plus `INVALID`.
#[test]
fn row_n_zero_is_one_unless_the_base_signals() {
    for x in [
        Decimal128::ZERO,
        Decimal128::NEG_ZERO,
        Decimal128::ONE,
        Decimal128::NEG_ONE,
        parse("-3.5"),
        parse("1e6000"),
        Decimal128::INFINITY,
        Decimal128::NEG_INFINITY,
        Decimal128::NAN,
    ] {
        for rm in ALL {
            let (r, st) = x.powi(0, rm);
            assert!(equal(r, Decimal128::ONE), "powi({x}, 0) at {rm:?} = {r}");
            assert_eq!(st, Status::OK, "powi({x}, 0) at {rm:?}: flags");
        }
    }
    for rm in ALL {
        let (r, st) = Decimal128::SIGNALING_NAN.powi(0, rm);
        assert!(r.is_nan() && !r.is_signaling_nan(), "powi(sNaN, 0) = {r}");
        assert!(st.invalid(), "powi(sNaN, 0): want INVALID, got {st:?}");
    }
}

/// "pown(±0, n) is ±∞ and signals the divideByZero exception for odd
/// n < 0" and "is +∞ and signals the divideByZero exception for even
/// n < 0".
#[test]
fn row_zero_base_negative_n_divides_by_zero() {
    for n in [-1, -3, -7, -49, i32::MAX.wrapping_neg()] {
        assert_eq!(n % 2, -1, "{n} is odd");
        for (x, want_neg) in [(Decimal128::ZERO, false), (Decimal128::NEG_ZERO, true)] {
            for rm in ALL {
                let (r, st) = x.powi(n, rm);
                assert!(r.is_infinite(), "powi({x}, {n}) at {rm:?} = {r}");
                assert_eq!(
                    r.is_sign_negative(),
                    want_neg,
                    "powi({x}, {n}) at {rm:?}: sign"
                );
                assert!(st.div_by_zero(), "powi({x}, {n}): want DIV_BY_ZERO");
            }
        }
    }
    for n in [-2, -4, -8, -112, i32::MIN] {
        assert_eq!(n % 2, 0, "{n} is even");
        for x in [Decimal128::ZERO, Decimal128::NEG_ZERO] {
            for rm in ALL {
                let (r, st) = x.powi(n, rm);
                assert!(
                    r.is_infinite() && !r.is_sign_negative(),
                    "powi({x}, {n}) at {rm:?} = {r}, want +inf"
                );
                assert!(st.div_by_zero(), "powi({x}, {n}): want DIV_BY_ZERO");
            }
        }
    }
}

/// "pown(±0, n) is +0 for even n > 0" and "is ±0 for odd n > 0".
#[test]
fn row_zero_base_positive_n_keeps_the_sign_only_when_odd() {
    for n in [1, 3, 7, 49, i32::MAX] {
        for (x, want_neg) in [(Decimal128::ZERO, false), (Decimal128::NEG_ZERO, true)] {
            for rm in ALL {
                let (r, st) = x.powi(n, rm);
                assert!(r.is_zero(), "powi({x}, {n}) at {rm:?} = {r}");
                assert_eq!(
                    r.is_sign_negative(),
                    want_neg,
                    "powi({x}, {n}) at {rm:?}: sign"
                );
                assert_eq!(st, Status::OK, "powi({x}, {n}): flags");
            }
        }
    }
    for n in [2, 4, 8, 112] {
        for x in [Decimal128::ZERO, Decimal128::NEG_ZERO] {
            for rm in ALL {
                let (r, st) = x.powi(n, rm);
                assert!(
                    r.is_zero() && !r.is_sign_negative(),
                    "powi({x}, {n}) at {rm:?} = {r}, want +0"
                );
                assert_eq!(st, Status::OK, "powi({x}, {n}): flags");
            }
        }
    }
}

/// "pown(+∞, n) is +∞ for n > 0", "pown(−∞, n) is −∞ for odd n > 0",
/// "is +∞ for even n > 0", "pown(+∞, n) is +0 for n < 0",
/// "pown(−∞, n) is −0 for odd n < 0", "is +0 for even n < 0".
#[test]
fn row_infinite_base() {
    for n in [1, 3, 49, i32::MAX] {
        for rm in ALL {
            let (r, st) = Decimal128::INFINITY.powi(n, rm);
            assert!(r.is_infinite() && !r.is_sign_negative(), "powi(+inf, {n})");
            assert_eq!(st, Status::OK);
            let (r, st) = Decimal128::NEG_INFINITY.powi(n, rm);
            assert!(r.is_infinite() && r.is_sign_negative(), "powi(-inf, {n})");
            assert_eq!(st, Status::OK);
        }
    }
    for n in [2, 4, 112] {
        for rm in ALL {
            let (r, _) = Decimal128::INFINITY.powi(n, rm);
            assert!(r.is_infinite() && !r.is_sign_negative(), "powi(+inf, {n})");
            let (r, _) = Decimal128::NEG_INFINITY.powi(n, rm);
            assert!(
                r.is_infinite() && !r.is_sign_negative(),
                "powi(-inf, {n}) must be +inf"
            );
        }
    }
    for n in [-1, -3, -49] {
        for rm in ALL {
            let (r, st) = Decimal128::INFINITY.powi(n, rm);
            assert!(r.is_zero() && !r.is_sign_negative(), "powi(+inf, {n})");
            assert_eq!(st, Status::OK);
            let (r, st) = Decimal128::NEG_INFINITY.powi(n, rm);
            assert!(r.is_zero() && r.is_sign_negative(), "powi(-inf, {n})");
            assert_eq!(st, Status::OK);
        }
    }
    for n in [-2, -4, -112, i32::MIN] {
        for rm in ALL {
            let (r, _) = Decimal128::INFINITY.powi(n, rm);
            assert!(r.is_zero() && !r.is_sign_negative(), "powi(+inf, {n})");
            let (r, _) = Decimal128::NEG_INFINITY.powi(n, rm);
            assert!(
                r.is_zero() && !r.is_sign_negative(),
                "powi(-inf, {n}) must be +0"
            );
        }
    }
}

/// A quiet NaN operand propagates for every `n ≠ 0`; a signaling NaN
/// raises `INVALID` and returns the quieted payload.
#[test]
fn row_nan_base_propagates_for_nonzero_n() {
    for n in [1, -1, 2, -2, 7, -7, i32::MAX, i32::MIN] {
        for rm in ALL {
            let (r, st) = Decimal128::NAN.powi(n, rm);
            assert!(r.is_nan(), "powi(NaN, {n}) = {r}");
            assert_eq!(st, Status::OK, "powi(qNaN, {n}) must not raise");
            let (r, st) = Decimal128::SIGNALING_NAN.powi(n, rm);
            assert!(
                r.is_nan() && !r.is_signaling_nan(),
                "powi(sNaN, {n}) = {r}, want quiet NaN"
            );
            assert!(st.invalid(), "powi(sNaN, {n}): want INVALID");
        }
    }
}

// ---------------------------------------------------------------------------
// The exact family: §7.5 forbids INEXACT on an exact result.

/// Small exact powers, positive and negative bases, in every
/// direction. The sign rule is `x < 0 && n odd`, and the magnitude is
/// rounded under the negation-reflected mode, so a directed mode must
/// land on the same magnitude either side of zero here (these are
/// exact, so nothing rounds at all).
#[test]
fn exact_small_powers_every_mode() {
    let cases = [
        ("1.5", 3, "3.375"),
        ("-1.5", 3, "-3.375"),
        ("-1.5", 2, "2.25"),
        ("0.2", 2, "0.04"),
        ("-0.2", 3, "-0.008"),
        ("2", 10, "1024"),
        ("-2", 3, "-8"),
        ("-2", 4, "16"),
        ("2", -3, "0.125"),
        ("-2", -3, "-0.125"),
        ("5", -2, "0.04"),
        ("10", 300, "1E+300"),
        ("0.5", 6, "0.015625"),
    ];
    for (base, n, want) in cases {
        let x = parse(base);
        let w = parse(want);
        for rm in ALL {
            let (r, st) = x.powi(n, rm);
            assert!(
                equal(r, w),
                "powi({base}, {n}) at {rm:?} = {r}, want {want}"
            );
            assert_eq!(st, Status::OK, "powi({base}, {n}) at {rm:?}: flags");
        }
    }
}

/// `2^112` is exactly 34 digits: the widest exact power of two the
/// format holds, delivered clean in every direction. `2^113` needs 35
/// and is the first inexact one, where the dropped digit is a 2 —
/// below the midpoint, so the three "down" modes keep the truncation
/// and only `TowardPositive` steps up.
#[test]
fn two_to_the_112_is_exact_and_113_is_not() {
    let want = parse("5192296858534827628530496329220096");
    for rm in ALL {
        let (r, st) = parse("2").powi(112, rm);
        assert!(equal(r, want), "powi(2, 112) at {rm:?} = {r}");
        assert_eq!(st, Status::OK, "powi(2, 112) at {rm:?}: flags");
    }
    let down = parse("1.038459371706965525706099265844019E+34");
    let up = parse("1.038459371706965525706099265844020E+34");
    for (rm, w) in [(NE, down), (NA, down), (TZ, down), (TN, down), (TP, up)] {
        let (r, st) = parse("2").powi(113, rm);
        assert!(equal(r, w), "powi(2, 113) at {rm:?} = {r}, want {w}");
        assert!(st.inexact(), "powi(2, 113) at {rm:?}: want INEXACT");
        assert!(!st.overflow(), "powi(2, 113) at {rm:?}: no overflow");
    }
}

/// The real tie. `5^49` has exactly 35 digits and ends in 5, so the
/// true value IS the midpoint between two representable neighbours —
/// a value the approximation kernel cannot resolve at any rung, since
/// its error picks an arbitrary side. The classifier hands the exact
/// coefficient to the format rounder, whose own tie rule then decides:
/// half-to-even keeps the even last digit 2, `NearestAway` and
/// `TowardPositive` step up, `TowardZero` and `TowardNegative` keep
/// the truncation.
#[test]
fn the_tie_at_five_to_the_49_resolves_by_mode() {
    let down = parse("1.776356839400250464677810668945312E+34");
    let up = parse("1.776356839400250464677810668945313E+34");
    for (rm, w, label) in [
        (NE, down, "half-to-even keeps the even digit"),
        (NA, up, "half-away steps up"),
        (TZ, down, "toward zero truncates"),
        (TN, down, "toward -inf truncates"),
        (TP, up, "toward +inf steps up"),
    ] {
        let (r, st) = parse("5").powi(49, rm);
        assert!(equal(r, w), "powi(5, 49) at {rm:?} = {r}: {label}");
        assert!(st.inexact(), "powi(5, 49) at {rm:?}: want INEXACT");
    }
    // The negative-exponent mirror: 2^-49 = 5^49 · 10^-49, the same
    // 35-digit coefficient one decade family down.
    for (rm, w) in [
        (NE, "1.776356839400250464677810668945312E-15"),
        (NA, "1.776356839400250464677810668945313E-15"),
    ] {
        let (r, st) = parse("2").powi(-49, rm);
        assert!(equal(r, parse(w)), "powi(2, -49) at {rm:?} = {r}");
        assert!(st.inexact(), "powi(2, -49) at {rm:?}: want INEXACT");
    }
    // The tie is negated coherently under an odd n over a negative
    // base: the magnitude rounds under the reflected mode, so
    // `TowardNegative` on the result is `TowardPositive` on the
    // magnitude.
    let (r, _) = parse("-5").powi(49, TN);
    assert!(equal(r, up.neg()), "powi(-5, 49) at TowardNegative = {r}");
    let (r, _) = parse("-5").powi(49, TP);
    assert!(equal(r, down.neg()), "powi(-5, 49) at TowardPositive = {r}");
}

/// `n = ±1`. The identity is exact for every input, and the
/// reciprocal is exact exactly when the stripped coefficient is a
/// 2-and-5 product (`1/s^1` terminates only for `s = 1`); otherwise
/// the powering arm's Newton `recip` runs and must agree with the
/// format's own division, which is correctly rounded by construction.
#[test]
fn n_plus_minus_one_is_identity_and_reciprocal() {
    for literal in [
        "1.5",
        "-1.5",
        "0.2",
        "3",
        "-7",
        "1e6000",
        "1e-6000",
        "9.999999999999999999999999999999999E+6144",
    ] {
        let x = parse(literal);
        for rm in ALL {
            let (r, st) = x.powi(1, rm);
            assert!(equal(r, x), "powi({literal}, 1) at {rm:?} = {r}");
            assert_eq!(st, Status::OK, "powi({literal}, 1): flags");
        }
    }
    for (literal, want) in [
        ("2", "0.5"),
        ("-2", "-0.5"),
        ("0.5", "2"),
        ("1000", "0.001"),
    ] {
        let x = parse(literal);
        for rm in ALL {
            let (r, st) = x.powi(-1, rm);
            assert!(equal(r, parse(want)), "powi({literal}, -1) at {rm:?} = {r}");
            assert_eq!(st, Status::OK, "powi({literal}, -1): flags");
        }
    }
    for literal in ["3", "7", "-3", "1.7", "9.87654321"] {
        let x = parse(literal);
        for rm in ALL {
            let (r, st) = x.powi(-1, rm);
            let (d, ds) = Decimal128::ONE.div(x, rm);
            assert!(
                equal(r, d),
                "powi({literal}, -1) at {rm:?} = {r}, div gives {d}"
            );
            assert_eq!(
                st.inexact(),
                ds.inexact(),
                "powi({literal}, -1) at {rm:?}: INEXACT must match div"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// The whole-range power-of-ten family and the §7.4 dispositions.

/// `powi(10, n) = 10^n` must be exact across the format's whole
/// exponent range, subnormal tail included, with clean flags (§7.5
/// forbids `INEXACT` on an exact result and `UNDERFLOW` on an exact
/// subnormal one). The family is the input-side mirror of `exp10`'s:
/// the value sits exactly on a grid point at its own exponent, which
/// no rung of the ladder can move off.
#[test]
fn powers_of_ten_across_the_range_every_mode() {
    for n in ETINY..=EMAX {
        let want = parse(&format!("1e{n}"));
        for rm in ALL {
            let (r, st) = parse("10").powi(n, rm);
            assert!(equal(r, want), "powi(10, {n}) at {rm:?} = {r}, want 1e{n}");
            assert_eq!(st, Status::OK, "powi(10, {n}) at {rm:?}: flags");
        }
    }
}

/// Other bases whose power is a power of ten: the classifier reads
/// the stripped form, so every cohort of the base and every
/// `10^j`-shaped base joins the family.
#[test]
fn power_of_ten_bases_in_every_cohort() {
    for (literal, j) in [
        ("10", 1),
        ("10.00", 1),
        ("100", 2),
        ("0.001", -3),
        ("1e300", 300),
        ("1e-300", -300),
        ("1e6144", 6144),
    ] {
        for n in [-6, -3, -1, 1, 2, 3, 6, 7] {
            let w = i64::from(j) * i64::from(n);
            if !(i64::from(ETINY)..=i64::from(EMAX)).contains(&w) {
                continue; // over/underflow, covered by its own test
            }
            let want = parse(&format!("1e{w}"));
            for rm in ALL {
                let (r, st) = parse(literal).powi(n, rm);
                assert!(
                    equal(r, want),
                    "powi({literal}, {n}) at {rm:?} = {r}, want 1e{w}"
                );
                assert_eq!(st, Status::OK, "powi({literal}, {n}) at {rm:?}: flags");
            }
        }
    }
}

/// Above the range, the §7.4 disposition per direction: `+∞` at both
/// nearest modes and toward `+∞`, the largest finite magnitude toward
/// zero and toward `−∞`, always with `OVERFLOW` and `INEXACT`. Run on
/// the power-of-ten family (classifier-delivered at any exponent) and
/// on a generic base whose exact power is far too wide to classify
/// (kernel-delivered), so both delivery paths are pinned.
#[test]
fn above_range_overflows_per_mode() {
    let mut cases: Vec<(Decimal128, i32, &str)> = Vec::new();
    for n in [
        EMAX + 1,
        EMAX + 2,
        10_000,
        50_000,
        WINDOW,
        WINDOW + 1,
        1_000_000,
        i32::MAX,
    ] {
        cases.push((parse("10"), n, "10"));
    }
    cases.push((parse("1e2000"), 6, "1e2000"));
    cases.push((parse("1.7e2000"), 6, "1.7e2000"));
    cases.push((parse("1.7e2000"), 7, "1.7e2000"));
    cases.push((parse("1e1000"), 2000, "1e1000"));
    for (x, n, label) in cases {
        for rm in [NE, NA, TP] {
            let (r, st) = x.powi(n, rm);
            assert!(
                r.is_infinite() && !r.is_sign_negative(),
                "powi({label}, {n}) at {rm:?} = {r}, want +inf"
            );
            assert!(
                st.overflow() && st.inexact(),
                "powi({label}, {n}) at {rm:?}: want OVERFLOW + INEXACT, got {st:?}"
            );
        }
        for rm in [TZ, TN] {
            let (r, st) = x.powi(n, rm);
            assert!(
                equal(r, Decimal128::MAX),
                "powi({label}, {n}) at {rm:?} = {r}, want MAX"
            );
            assert!(
                st.overflow() && st.inexact(),
                "powi({label}, {n}) at {rm:?}: want OVERFLOW + INEXACT, got {st:?}"
            );
        }
    }
}

/// A negative base with an odd exponent overflows on the other side:
/// `−∞` at the nearest modes and toward `−∞`, `−MAX` toward zero and
/// toward `+∞`. This is the fd-aqs.5 reflection rule at the format
/// boundary, where getting it backwards is most visible.
#[test]
fn above_range_negative_base_overflows_on_the_negative_side() {
    for (x, n, label) in [
        (parse("-10"), 7001, "-10"),
        (parse("-1.7e2000"), 7, "-1.7e2000"),
        (parse("-1e2000"), 5, "-1e2000"),
    ] {
        assert_eq!(n % 2, 1, "the negative side needs an odd exponent");
        for rm in [NE, NA, TN] {
            let (r, st) = x.powi(n, rm);
            assert!(
                r.is_infinite() && r.is_sign_negative(),
                "powi({label}, {n}) at {rm:?} = {r}, want -inf"
            );
            assert!(st.overflow() && st.inexact(), "powi({label}, {n}): {st:?}");
        }
        for rm in [TZ, TP] {
            let (r, st) = x.powi(n, rm);
            assert!(
                equal(r, Decimal128::MAX.neg()),
                "powi({label}, {n}) at {rm:?} = {r}, want -MAX"
            );
            assert!(st.overflow() && st.inexact(), "powi({label}, {n}): {st:?}");
        }
    }
}

/// Below the range, the §7.4 disposition per direction: `+0` at both
/// nearest modes, toward zero and toward `−∞`, the smallest subnormal
/// toward `+∞`, always with `UNDERFLOW` and `INEXACT`.
#[test]
fn below_range_underflows_per_mode() {
    let mut cases: Vec<(Decimal128, i32, &str)> = Vec::new();
    for n in [
        ETINY - 1,
        ETINY - 2,
        -10_000,
        -50_000,
        -WINDOW,
        -WINDOW - 1,
        i32::MIN + 1,
    ] {
        cases.push((parse("10"), n, "10"));
    }
    cases.push((parse("1e-2000"), 6, "1e-2000"));
    cases.push((parse("1.7e-2000"), 6, "1.7e-2000"));
    cases.push((parse("1.7e-2000"), 7, "1.7e-2000"));
    cases.push((parse("1e1000"), -2000, "1e1000"));
    for (x, n, label) in cases {
        for rm in [NE, NA, TZ, TN] {
            let (r, st) = x.powi(n, rm);
            assert!(
                r.is_zero() && !r.is_sign_negative(),
                "powi({label}, {n}) at {rm:?} = {r}, want +0"
            );
            assert!(
                st.underflow() && st.inexact(),
                "powi({label}, {n}) at {rm:?}: want UNDERFLOW + INEXACT, got {st:?}"
            );
        }
        let (r, st) = x.powi(n, TP);
        assert!(
            equal(r, Decimal128::MIN_POSITIVE),
            "powi({label}, {n}) at TowardPositive = {r}, want the smallest subnormal"
        );
        assert!(
            st.underflow() && st.inexact(),
            "powi({label}, {n}) at TowardPositive: {st:?}"
        );
    }
}

/// The subnormal band, where the quantum is pinned at `etiny` rather
/// than by the precision: an exact power landing there must still be
/// exact, with no `UNDERFLOW` (§7.5 asks for the flag only on an
/// inexact tiny result). Every case below lands at a quantum at or
/// above `etiny = −6176` with an adjusted exponent below `emin`, so
/// it is a genuine exact subnormal; the last two rows are the
/// neighbouring inexact ones, where the true value needs a quantum
/// *past* `etiny` and `UNDERFLOW | INEXACT` is then correct.
#[test]
fn exact_results_in_the_subnormal_band_are_clean() {
    for (base, n, want) in [
        ("1e-3088", 2, "1e-6176"),
        ("2e-3088", 2, "4e-6176"),
        ("3e-3088", 2, "9e-6176"),
        ("2e-2058", 3, "8e-6174"),
        ("1e-1544", 4, "1e-6176"),
    ] {
        let x = parse(base);
        let w = parse(want);
        for rm in ALL {
            let (r, st) = x.powi(n, rm);
            assert!(
                equal(r, w),
                "powi({base}, {n}) at {rm:?} = {r}, want {want}"
            );
            assert_eq!(
                st,
                Status::OK,
                "powi({base}, {n}) at {rm:?}: an exact subnormal must not flag, got {st:?}"
            );
        }
    }
    // 5^3 · 10^-6177 needs a quantum one below `etiny`: inexact, and
    // tiny, so both flags are owed.
    for (base, n) in [("5e-2059", 3), ("3e-3089", 2)] {
        let (_, st) = parse(base).powi(n, NE);
        assert!(
            st.underflow() && st.inexact(),
            "powi({base}, {n}): want UNDERFLOW + INEXACT, got {st:?}"
        );
    }
}

/// The powering arm's reciprocal seam. For `n < 0` the arm powers
/// first and inverts second, so `|x|^|n|` can leave the format's
/// exponent range entirely *before* the reciprocal runs. `recip`
/// seeds Newton through a format round trip, so such an accumulator
/// used to reach `from_format` as an infinity and panic on the
/// non-finite datum; the arm now
/// scales into `[1, 10)` first and shifts the exponent back. Both
/// directions of the seam, and the sign reflection across it — the
/// classifier declines every case here (`s ≠ 1` with `n < 0`), so
/// they all reach the arm.
#[test]
fn the_powering_arms_reciprocal_survives_out_of_range_accumulators() {
    for (base, n) in [("1.7e2000", -6), ("1.7e1500", -5), ("9.9e6144", -6)] {
        for rm in [NE, NA, TZ, TN] {
            let (r, st) = parse(base).powi(n, rm);
            assert!(
                r.is_zero() && !r.is_sign_negative(),
                "powi({base}, {n}) at {rm:?} = {r}, want +0"
            );
            assert!(
                st.underflow() && st.inexact(),
                "powi({base}, {n}) at {rm:?}: want UNDERFLOW + INEXACT, got {st:?}"
            );
        }
        let (r, st) = parse(base).powi(n, TP);
        assert!(
            equal(r, Decimal128::MIN_POSITIVE),
            "powi({base}, {n}) at TowardPositive = {r}"
        );
        assert!(st.underflow() && st.inexact(), "powi({base}, {n}): {st:?}");
    }
    for (base, n) in [("1.7e-2000", -6), ("1.7e-1500", -5), ("9e-6176", -6)] {
        for rm in [NE, NA, TP] {
            let (r, st) = parse(base).powi(n, rm);
            assert!(
                r.is_infinite() && !r.is_sign_negative(),
                "powi({base}, {n}) at {rm:?} = {r}, want +inf"
            );
            assert!(
                st.overflow() && st.inexact(),
                "powi({base}, {n}) at {rm:?}: want OVERFLOW + INEXACT, got {st:?}"
            );
        }
        for rm in [TZ, TN] {
            let (r, _) = parse(base).powi(n, rm);
            assert!(
                equal(r, Decimal128::MAX),
                "powi({base}, {n}) at {rm:?} = {r}"
            );
        }
    }
    // A negative base with an odd exponent reflects the whole seam:
    // the magnitude overflows toward `+∞` while the result goes to
    // `−∞`, and the directed modes swap with it.
    for rm in [NE, NA, TN] {
        let (r, st) = parse("-1.7e-2000").powi(-5, rm);
        assert!(
            r.is_infinite() && r.is_sign_negative(),
            "powi(-1.7e-2000, -5) at {rm:?} = {r}, want -inf"
        );
        assert!(st.overflow() && st.inexact(), "flags at {rm:?}: {st:?}");
    }
    for rm in [TZ, TP] {
        let (r, _) = parse("-1.7e-2000").powi(-5, rm);
        assert!(
            equal(r, Decimal128::MAX.neg()),
            "powi(-1.7e-2000, -5) at {rm:?} = {r}, want -MAX"
        );
    }
}

// ---------------------------------------------------------------------------
// The arm seam: |n| ≤ 6 powers at working precision, |n| ≥ 7 goes
// through exp(n·ln|x|).

/// Both arms must agree with `pow` on the same inputs, which is a real
/// differential: `pow` composes `exp(y·ln|x|)` (or its bit-exact
/// format-precision fast path) for both exponents, while `powi` runs
/// binary powering at working precision for 6 and the composition for
/// 7. Two independently derived pipelines, one claimed correctly
/// rounded result. The sweep is deterministic and straddles the seam
/// at every rounding direction.
#[test]
fn the_arm_seam_agrees_with_pow() {
    let bases = [
        "1.7",
        "-1.7",
        "3",
        "-3",
        "0.7",
        "9.87654321",
        "1.0000000000000000000000000000000001",
        "0.9999999999999999999999999999999999",
        "123456789.123456789",
        "-0.001",
        "7e100",
        "3e-100",
        "2.5",
        "1e-40",
    ];
    for base in bases {
        let x = parse(base);
        for n in [-8i32, -7, -6, -5, -2, -1, 1, 2, 5, 6, 7, 8] {
            let y = Decimal128::from_i32(n);
            for rm in ALL {
                let (a, sa) = x.powi(n, rm);
                let (b, sb) = x.pow(y, rm);
                assert!(
                    equal(a, b) || (a.is_nan() && b.is_nan()),
                    "powi({base}, {n}) at {rm:?} = {a}, pow gives {b}"
                );
                assert_eq!(
                    sa.inexact(),
                    sb.inexact(),
                    "powi({base}, {n}) at {rm:?}: INEXACT disagrees with pow ({sa:?} vs {sb:?})"
                );
            }
        }
    }
}

/// The powering arm's own metamorphic check, independent of `pow`:
/// `x^6 = (x^3)^2` and `x^-n = 1/x^n` hold exactly whenever every
/// intermediate is exact, which a base with few digits guarantees.
#[test]
fn the_powering_arm_composes() {
    for base in ["1.5", "-1.5", "2", "0.2", "3", "-0.5", "1.25"] {
        let x = parse(base);
        let (cube, _) = x.powi(3, NE);
        let (six_direct, s6) = x.powi(6, NE);
        let (six_composed, sc) = cube.powi(2, NE);
        assert!(
            equal(six_direct, six_composed),
            "powi({base}, 6) = {six_direct}, (x^3)^2 = {six_composed}"
        );
        assert_eq!(
            s6.inexact(),
            sc.inexact(),
            "powi({base}, 6): flag agreement"
        );
        let (pos, _) = x.powi(4, NE);
        let (neg, _) = x.powi(-4, NE);
        let (recip, _) = Decimal128::ONE.div(pos, NE);
        assert!(
            equal(neg, recip),
            "powi({base}, -4) = {neg}, 1/x^4 = {recip}"
        );
    }
}

// ---------------------------------------------------------------------------
// §9.2.2 preferred exponent, recorded rather than asserted-into-shape.

/// IEEE 754-2019 §9.2.2 states `Q(pown(x, n))` is `floor(n × Q(x))`.
/// The classifier delivers the *stripped* coefficient through the
/// format rounder with a preferred quantum of 0, so what actually
/// ships is the §6.3 "as close to zero as the value allows" quantum:
/// `powi(1.20, 2)` delivers `1.44` (quantum −2), not the `1.4400`
/// (quantum −4) §9.2.2 asks for, and `powi(1.20, 1)` delivers `1.2`
/// (quantum −1) rather than `1.20`. This test pins the *observed*
/// behaviour so the delta is a recorded fact rather than a surprise;
/// changing it means changing `exact::pack_value`'s preferred quantum
/// for every classifier that shares it (`pow` included, which behaves
/// identically), which is out of scope here.
#[test]
fn preferred_exponent_deltas_are_pinned_as_observed() {
    // (base, n, delivered string, the §9.2.2 quantum, the delivered one)
    let cases = [
        ("1.20", 2, "1.44", -4, -2),
        ("1.20", 1, "1.2", -2, -1),
        ("1.5", 3, "3.375", -3, -3),
        ("0.2", 2, "0.04", -2, -2),
        ("2", 3, "8", 0, 0),
        ("2.00", 3, "8", 0, 0),
    ];
    for (base, n, delivered, want_9_2_2, observed) in cases {
        let (r, _) = parse(base).powi(n, NE);
        assert_eq!(
            r.to_string(),
            delivered,
            "powi({base}, {n}): delivered form drifted"
        );
        // The pin is the string above; these two numbers are the
        // documentation of what the string means.
        let _ = (want_9_2_2, observed);
    }
}

// ---------------------------------------------------------------------------
// Cohort insensitivity and the classifier's stripped-form reading.

/// The classifier reads the stripped form, so every cohort of a base
/// takes the same path to the same value.
#[test]
fn cohort_variants_of_the_base_agree() {
    for (a, b) in [
        ("2", "2.000"),
        ("2", "20e-1"),
        ("1.5", "1.5000"),
        ("0.2", "0.2000"),
        ("-3", "-3.00"),
        ("10", "1e1"),
        ("10", "100e-1"),
    ] {
        for n in [-7, -6, -1, 1, 2, 3, 6, 7] {
            for rm in ALL {
                let (ra, sa) = parse(a).powi(n, rm);
                let (rb, sb) = parse(b).powi(n, rm);
                assert!(
                    equal(ra, rb),
                    "powi({a}, {n}) = {ra} but powi({b}, {n}) = {rb} at {rm:?}"
                );
                assert_eq!(sa, sb, "powi({a}/{b}, {n}) at {rm:?}: flags differ");
            }
        }
    }
}

/// `powi(±1, n)` is 1 (or −1 for an odd `n` over `−1`) for every `n`,
/// including exponents far past any width the classifier could
/// otherwise reach — the `|x| = 1` case that anchors the bail proofs.
#[test]
fn unit_bases_at_every_exponent() {
    for n in [1, -1, 2, -2, 7, -7, 99_999, -99_999, i32::MAX, i32::MIN] {
        for rm in ALL {
            let (r, st) = Decimal128::ONE.powi(n, rm);
            assert!(equal(r, Decimal128::ONE), "powi(1, {n}) at {rm:?} = {r}");
            assert_eq!(st, Status::OK, "powi(1, {n}): flags");
            let (r, st) = Decimal128::NEG_ONE.powi(n, rm);
            let want = if n % 2 == 0 {
                Decimal128::ONE
            } else {
                Decimal128::NEG_ONE
            };
            assert!(equal(r, want), "powi(-1, {n}) at {rm:?} = {r}");
            assert_eq!(st, Status::OK, "powi(-1, {n}): flags");
        }
    }
}
