//! Exact-result, special-value, quantum, range-disposition, and
//! anchor-band gate for `Decimal64`'s `compound` (IEEE 754-2019 §9.2;
//! ADR-0059 Track D group D3, fd-4zo.25). The sibling mirror of the
//! root crate's `tests/transcend_exact_compound.rs`; the two differ
//! only in the format's precision and exponent range.
//!
//! `compound(x, n) = (1 + x)^n` is unlike every other operation in this
//! family: its value is **always rational**. `1 + x` is an exact
//! rational for every representable `x > −1` (the D1 `logp1` exact-sum
//! analysis), and an integer power of a rational is rational. So exact
//! results and nearest-mode ties are not rare corners to be hunted but
//! the operation's ordinary business, and the input-side classifier
//! (`exact::compound_exact_input`) is the whole §7.5 story rather than a
//! filter in front of a transcendental kernel.
//!
//! Two families sit exactly ON a format grid point, where the ADR-0059
//! escalation predicate reads distance zero and no rung improves on it
//! — the third sighting of the class D1's `log10p1` integer anchor and
//! D2's `exp10` integer family already found:
//!
//! * `1 + x = 10^k`, the nines patterns (`x = 9, 99, …` above zero,
//!   `x = −0.9, −0.99, …` below it). Then `compound(x, n) = 10^(k·n)`
//!   is a grid point at its own exponent for every `n`, in the format's
//!   range or thousands of decades outside it. The classifier owns the
//!   family whole and the format rounder's §7.4 disposition answers the
//!   out-of-range half; this file walks both, in all five directions.
//! * The tiny-`n·x` band, where the value hugs 1 closer than any rung
//!   can resolve and the ADR-0051 residual channel decides it from the
//!   strict side theorem
//!   `sign((1+x)^n − 1) = sign(n) · sign(x)`.
//!
//! §9.2.2's preferred exponent (`floor(n × min(0, Q(x)))`) is pinned
//! here too, on the exact deliveries where it has bite. It reads `x`'s
//! *stored* quantum, so the pins below use the canonical string rather
//! than value equality wherever the quantum is the point.

#![cfg(feature = "exp-log")]

use ferrodec_decimal64::{Decimal64, RoundingMode, Status};

const NE: RoundingMode = RoundingMode::NearestEven;
const NA: RoundingMode = RoundingMode::NearestAway;
const TZ: RoundingMode = RoundingMode::TowardZero;
const TP: RoundingMode = RoundingMode::TowardPositive;
const TN: RoundingMode = RoundingMode::TowardNegative;
const ALL: [RoundingMode; 5] = [NE, NA, TZ, TP, TN];

/// `10^384` is the last power of ten inside `Decimal64`.
const EMAX: i32 = 384;
/// `10^-398` is the smallest positive subnormal.
const ETINY: i32 = -398;
/// The integer the shared `exp` overflow gate does not catch:
/// `385 · ln 10 ≈ 886.5` stays inside the format's 887 limit while
/// `10^385` is past `MAX`. Reached here as `compound(9, 6145)`, the
/// `exp10` gate-gap witness in its `compound` costume: without the
/// classifier the kernel would decide it from the sign of its own noise.
const GATE_GAP: i32 = 385;

/// The format's precision in decimal digits.
const PRECISION: usize = 16;
/// A decade far past the huge-`x` anchor gate
/// (`adj ≥ PRECISION + digits(n) + 4`) whose low powers still sit
/// inside the exponent range.
const HUGE_EXP: i32 = 100;

fn parse(s: &str) -> Decimal64 {
    Decimal64::parse_str(s, NE)
        .unwrap_or_else(|_| panic!("literal parses: {s:?}"))
        .0
}

/// Value equality, cohort insensitive (the IEEE `compare`).
fn equal(a: Decimal64, b: Decimal64) -> bool {
    a.partial_cmp(b).0 == Some(core::cmp::Ordering::Equal)
}

/// The canonical string, which pins the *quantum* as well as the value.
fn text(a: Decimal64) -> String {
    format!("{a}")
}

// ---------------------------------------------------------------------------
// IEEE 754-2019 §9.2.1: the special-value table, row by row.

/// "compound(x, 0) is 1 for x ≥ −1 or quiet NaN."
///
/// Both carve-outs are load bearing and easy to get wrong. The quiet
/// NaN yields 1 rather than propagating — one of the few places in the
/// standard where a NaN does not survive — and the rule is conditioned
/// on `x ≥ −1`, so an `x` below the domain takes the invalid-operation
/// row instead of this one even at `n = 0`.
#[test]
fn n_zero_is_one_for_the_whole_domain_and_quiet_nan() {
    for rm in ALL {
        for literal in ["-1", "-0.5", "-0", "0", "1E-398", "1", "9.5", "9.999E+384"] {
            let (r, st) = parse(literal).compound(0, rm);
            assert!(
                equal(r, Decimal64::ONE),
                "compound({literal}, 0) at {rm:?}: got {r}, want 1"
            );
            assert_eq!(st, Status::OK, "compound({literal}, 0) at {rm:?}: flags");
        }
        // +∞ ≥ −1, so it takes the 1 row too.
        let (r, st) = Decimal64::INFINITY.compound(0, rm);
        assert!(equal(r, Decimal64::ONE), "compound(+∞, 0) at {rm:?}: {r}");
        assert_eq!(st, Status::OK);
        // The quiet-NaN carve-out.
        let (r, st) = Decimal64::NAN.compound(0, rm);
        assert!(equal(r, Decimal64::ONE), "compound(qNaN, 0) at {rm:?}: {r}");
        assert_eq!(st, Status::OK, "compound(qNaN, 0) raises nothing");
    }
}

/// "compound(x, n) is qNaN and signals the invalid operation exception
/// for x < −1" — at every `n`, `n = 0` included, since the `n = 0` row
/// above is conditioned on `x ≥ −1`. `−∞` is below `−1` and takes this
/// row rather than the infinity rows.
#[test]
fn below_negative_one_is_invalid_at_every_n() {
    for rm in ALL {
        for n in [i32::MIN, -7, -1, 0, 1, 7, i32::MAX] {
            // The first row is `next_down(−1)`, the closest a
            // `Decimal64` gets to the domain edge from below.
            for literal in ["-1.000000000000001", "-2", "-1E+384"] {
                let (r, st) = parse(literal).compound(n, rm);
                assert!(r.is_nan(), "compound({literal}, {n}) at {rm:?}: got {r}");
                assert_eq!(
                    st,
                    Status::INVALID,
                    "compound({literal}, {n}) at {rm:?}: flags"
                );
            }
            let (r, st) = Decimal64::NEG_INFINITY.compound(n, rm);
            assert!(r.is_nan(), "compound(−∞, {n}) at {rm:?}: got {r}");
            assert_eq!(st, Status::INVALID, "compound(−∞, {n}) at {rm:?}: flags");
        }
    }
}

/// "compound(−1, n) is +∞ and signals the divideByZero exception for
/// n < 0" and "compound(−1, n) is +0 for n > 0" — the `0^n` edge of the
/// domain, from both sides.
#[test]
fn negative_one_is_the_zero_base_edge() {
    for rm in ALL {
        for n in [i32::MIN, -6145, -2, -1] {
            let (r, st) = parse("-1").compound(n, rm);
            assert!(
                r.is_infinite() && !r.is_sign_negative(),
                "compound(−1, {n}) at {rm:?}: got {r}, want +∞"
            );
            assert_eq!(st, Status::DIV_BY_ZERO, "compound(−1, {n}) at {rm:?}");
        }
        for n in [1, 2, 6145, i32::MAX] {
            let (r, st) = parse("-1").compound(n, rm);
            assert!(
                r.is_zero() && !r.is_sign_negative(),
                "compound(−1, {n}) at {rm:?}: got {r}, want +0"
            );
            assert_eq!(st, Status::OK, "compound(−1, {n}) at {rm:?}");
        }
        // A cohort variant of −1 lands on the same rows.
        let (r, st) = parse("-1.000").compound(-1, rm);
        assert!(r.is_infinite() && !r.is_sign_negative());
        assert_eq!(st, Status::DIV_BY_ZERO);
    }
}

/// "compound(±0, n) is 1" — both signed zeros, every `n`.
#[test]
fn signed_zeros_are_one() {
    for rm in ALL {
        for n in [i32::MIN, -3, -1, 1, 3, i32::MAX] {
            for literal in ["0", "-0", "0E+100", "-0E-100"] {
                let (r, st) = parse(literal).compound(n, rm);
                assert!(
                    equal(r, Decimal64::ONE),
                    "compound({literal}, {n}) at {rm:?}: got {r}"
                );
                assert_eq!(st, Status::OK, "compound({literal}, {n}) at {rm:?}");
            }
        }
    }
}

/// "compound(+∞, n) is +∞ for n > 0" and "+0 for n < 0".
#[test]
fn positive_infinity_follows_the_sign_of_n() {
    for rm in ALL {
        for n in [1, 2, i32::MAX] {
            let (r, st) = Decimal64::INFINITY.compound(n, rm);
            assert!(
                r.is_infinite() && !r.is_sign_negative(),
                "compound(+∞, {n}) at {rm:?}: got {r}"
            );
            assert_eq!(st, Status::OK);
        }
        for n in [-1, -2, i32::MIN] {
            let (r, st) = Decimal64::INFINITY.compound(n, rm);
            assert!(
                r.is_zero() && !r.is_sign_negative(),
                "compound(+∞, {n}) at {rm:?}: got {r}"
            );
            assert_eq!(st, Status::OK);
        }
    }
}

/// "compound(qNaN, n) is qNaN for n ≠ 0", and the signaling row the
/// table does not carve out: a signaling NaN raises `INVALID` and
/// quiets at *every* `n`, `n = 0` included.
#[test]
fn nan_rows() {
    for rm in ALL {
        for n in [i32::MIN, -1, 1, i32::MAX] {
            let (r, st) = Decimal64::NAN.compound(n, rm);
            assert!(r.is_nan(), "compound(qNaN, {n}) at {rm:?}");
            assert_eq!(st, Status::OK, "a quiet NaN raises nothing");
        }
        for n in [i32::MIN, -1, 0, 1, i32::MAX] {
            let (r, st) = Decimal64::SIGNALING_NAN.compound(n, rm);
            assert!(r.is_nan() && !r.is_signaling_nan(), "compound(sNaN, {n})");
            assert_eq!(
                st,
                Status::INVALID,
                "compound(sNaN, {n}) at {rm:?}: the n = 0 row's carve-out \
                 is for QUIET NaNs only"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// The exact family, and the §9.2.2 preferred exponent it carries.

/// Named exact results, pinned as canonical strings so the delivered
/// *quantum* is pinned with the value. §7.5 forbids `INEXACT` on all of
/// them, at every rounding direction.
///
/// The quantum column is §9.2.2's `floor(n × min(0, Q(x)))`, read off
/// the stored quantum. `compound(0.20, 2) = 1.4400` is the row that
/// shows it doing real work: the value is 1.44, but `Q(0.20) = −2`
/// makes the preferred exponent `−4`, so two trailing zeros are kept.
/// `compound(0.2, 2) = 1.44` on the next row is the same number with
/// `Q = −1` and therefore a different quantum — cohort sensitivity that
/// §9.2.2 asks for explicitly.
#[test]
fn named_exact_results_with_their_quanta() {
    for (x, n, want) in [
        ("0.05", 3, "1.157625"),
        ("9", 2, "100"),
        ("99", 5, "10000000000"),
        ("-0.99", 2, "0.0001"),
        ("0.25", -1, "0.8"),
        ("5", 2, "36"),
        ("19", 2, "400"),
        ("1", 10, "1024"),
        ("0.2", 2, "1.44"),
        ("0.20", 2, "1.4400"),
        ("-0.5", 3, "0.125"),
        ("-0.5", -3, "8"),
        ("0.5", 4, "5.0625"),
        ("4", 1, "5"),
        ("0.05", 1, "1.05"),
        // 1.3^7 = 13^7 / 10^7 exactly; quantum 7 × (−1) = −7.
        ("0.3", 7, "6.2748517"),
    ] {
        for rm in ALL {
            let (r, st) = parse(x).compound(n, rm);
            assert_eq!(
                text(r),
                want,
                "compound({x}, {n}) at {rm:?}: value or quantum"
            );
            assert_eq!(
                st,
                Status::OK,
                "compound({x}, {n}) at {rm:?}: §7.5 forbids INEXACT here"
            );
        }
    }
}

/// A preferred exponent §6.3 cannot reach is a *preference*, not a
/// promise: the quantum moves only as far as the coefficient allows.
/// A negative `n` over a fractional `x` always asks for a *positive*
/// preferred exponent, which no value below 10 can carry, so this is
/// the systematic case rather than a corner. Recorded as a test rather
/// than a comment because it is the one place §9.2.2's stated exponent
/// and the delivered quantum differ, and the difference is the
/// standard's own §6.3 rule rather than a shortfall.
#[test]
fn unattainable_preferred_exponent_falls_back_to_the_nearest() {
    for rm in ALL {
        // prefers +2, delivers −1
        assert_eq!(text(parse("0.25").compound(-1, rm).0), "0.8");
        // prefers +1, delivers −3
        assert_eq!(text(parse("0.6").compound(-1, rm).0), "0.625");
        // prefers 0, delivers −1
        assert_eq!(text(parse("1").compound(-1, rm).0), "0.5");
    }
}

// ---------------------------------------------------------------------------
// The whole-range power-of-ten family: `1 + x = 10^k`.

/// The nines patterns inside the exponent range: `compound(9, n)` is
/// `10^n` and `compound(-0.9, n)` is `10^-n`, exactly, at every
/// direction, all the way to `emax` and down to `etiny`. The subnormal
/// tail is the witness that §7.5 forbids `UNDERFLOW` on an *exact*
/// subnormal result as well as `INEXACT`.
#[test]
fn nines_family_inside_the_range_every_mode() {
    for n in 1..=EMAX {
        let want = parse(&format!("1e{n}"));
        for rm in ALL {
            let (r, st) = parse("9").compound(n, rm);
            assert!(equal(r, want), "compound(9, {n}) at {rm:?}: got {r}");
            assert_eq!(st, Status::OK, "compound(9, {n}) at {rm:?}: flags");
        }
    }
    for n in 1..=-ETINY {
        let want = parse(&format!("1e-{n}"));
        for rm in ALL {
            let (r, st) = parse("-0.9").compound(n, rm);
            assert!(equal(r, want), "compound(−0.9, {n}) at {rm:?}: got {r}");
            assert_eq!(st, Status::OK, "compound(−0.9, {n}) at {rm:?}: flags");
        }
    }
}

/// The wider nines patterns, and the negative-`n` direction: `1 + x`
/// is `10^k` for `|k|` up to `PRECISION`, and `compound(x, n)` is then
/// `10^(k·n)` — including where `k·n` lands outside the exponent range,
/// which the next tests cover per mode.
#[test]
fn nines_family_across_k_and_both_signs_of_n() {
    for (x, k) in [
        ("9", 1),
        ("99", 2),
        ("999", 3),
        ("9999999999999999", 16),
        ("-0.9", -1),
        ("-0.99", -2),
        ("-0.999", -3),
        ("-0.9999999999999999", -16),
    ] {
        for n in [-7i32, -3, -1, 1, 3, 7, 60] {
            let kn = k * n;
            if !(ETINY..=EMAX).contains(&kn) {
                continue;
            }
            let want = parse(&format!("1e{kn}"));
            for rm in ALL {
                let (r, st) = parse(x).compound(n, rm);
                assert!(
                    equal(r, want),
                    "compound({x}, {n}) at {rm:?}: got {r}, want 1e{kn}"
                );
                assert_eq!(st, Status::OK, "compound({x}, {n}) at {rm:?}: flags");
            }
        }
    }
}

/// Past `emax`, including the gate-gap integer explicitly. The true
/// value `10^(k·n)` is past `MAX`, so §7.4 asks for `+∞` at both
/// nearest modes and toward `+∞`, and the largest finite magnitude
/// toward zero and toward `−∞`, always with `OVERFLOW | INEXACT`.
///
/// `compound(9, 6145)` is the load-bearing row: `6145 · ln 10` stays
/// inside the shared `exp` overflow gate, so without the input-side
/// classifier the kernel would form a working value sitting exactly ON
/// the grid point `1·10^6145` and decide the directed modes by the sign
/// of its own noise.
#[test]
fn above_range_powers_of_ten_overflow_per_mode() {
    let max = parse("9.999999999999999E+384");
    let mut saw_gap = false;
    for n in EMAX + 1..=EMAX + 40 {
        if n == GATE_GAP {
            saw_gap = true;
        }
        for rm in [NE, NA, TP] {
            let (r, st) = parse("9").compound(n, rm);
            assert!(
                r.is_infinite() && !r.is_sign_negative(),
                "compound(9, {n}) at {rm:?}: got {r}, want +∞"
            );
            assert_eq!(st, Status::OVERFLOW | Status::INEXACT, "compound(9, {n})");
        }
        for rm in [TZ, TN] {
            let (r, st) = parse("9").compound(n, rm);
            assert!(
                equal(r, max),
                "compound(9, {n}) at {rm:?}: got {r}, want MAX"
            );
            assert_eq!(st, Status::OVERFLOW | Status::INEXACT, "compound(9, {n})");
        }
    }
    assert!(saw_gap, "the gate-gap integer stayed in the swept range");
    // The far end, where `k·n` runs past every gate but still fits i32.
    for (x, n) in [("9", i32::MAX), ("99", 1_000_000), ("9", 2_000_000)] {
        let (r, st) = parse(x).compound(n, NE);
        assert!(r.is_infinite(), "compound({x}, {n}): got {r}");
        assert_eq!(st, Status::OVERFLOW | Status::INEXACT);
    }
}

/// Below `etiny`: the true value is a tenth of the smallest subnormal
/// or less, so the nearest modes and both downward directions deliver
/// `+0` and `TowardPositive` delivers the smallest subnormal, with
/// `UNDERFLOW | INEXACT`. `CLAMPED` rides along on the zero deliveries
/// exactly as it does for `exp10` at the same magnitudes (ADR-0048): the
/// §9.2.2 preferred quantum falls below `qmin`, and a zero is exact at
/// every exponent, so the clamp is the informational signal §7.4 asks
/// for. Asserted as a subset so this file pins the value and the
/// underflow story without re-litigating the clamp.
#[test]
fn below_range_powers_of_ten_underflow_per_mode() {
    let tiny = parse("1E-398");
    for n in -ETINY + 1..=-ETINY + 40 {
        for rm in [NE, NA, TZ, TN] {
            let (r, st) = parse("-0.9").compound(n, rm);
            assert!(
                r.is_zero() && !r.is_sign_negative(),
                "compound(−0.9, {n}) at {rm:?}: got {r}, want +0"
            );
            assert!(
                (st.underflow() && st.inexact()),
                "compound(−0.9, {n}) at {rm:?}: flags {st:?}"
            );
        }
        let (r, st) = parse("-0.9").compound(n, TP);
        assert!(
            equal(r, tiny),
            "compound(−0.9, {n}) at TowardPositive: got {r}, want 1E-398"
        );
        assert!((st.underflow() && st.inexact()));
    }
    // Past i32 on `k·n`, where the classifier bails and the shared `exp`
    // saturation proxy answers instead — the two must agree.
    let (r, st) = parse("-0.99").compound(i32::MAX, NE);
    assert!(r.is_zero(), "compound(−0.99, i32::MAX): got {r}");
    assert!((st.underflow() && st.inexact()));
}

// ---------------------------------------------------------------------------
// The nearest-mode ties.

/// The ties are real and reachable from both signs of `n`. A midpoint's
/// stripped coefficient carries exactly `PRECISION + 1 = 35` digits and
/// ends in 5, which `compound` reaches through its 5-leg: `1 + 4 = 5`
/// gives `compound(4, n) = 5^n`, and `compound(1, −n) = 2^−n` mirrors
/// it from the negative side.
///
/// No approximation kernel can resolve these: the true value IS the
/// rounding boundary, so a kernel's error picks an arbitrary side. The
/// classifier hands the exact coefficient to the format rounder, whose
/// own tie rule then decides — `NearestEven` down to the even last digit,
/// `NearestAway` and `TowardPositive` up, `TowardZero` and
/// `TowardNegative` truncating.
#[test]
fn nearest_mode_ties_resolve_by_the_rounders_own_rule() {
    let down = "1.192092895507812E+16";
    let up = "1.192092895507813E+16";
    for (rm, want) in [(NE, down), (NA, up), (TZ, down), (TP, up), (TN, down)] {
        let (r, st) = parse("4").compound(23, rm);
        assert_eq!(text(r), want, "compound(4, 23) at {rm:?}");
        assert_eq!(st, Status::INEXACT, "a tie is inexact in every direction");
    }
    // 5^24 is the second 17-digit power of five, so the second tie.
    let down24 = "5.960464477539062E+16";
    let up24 = "5.960464477539063E+16";
    for (rm, want) in [
        (NE, down24),
        (NA, up24),
        (TZ, down24),
        (TP, up24),
        (TN, down24),
    ] {
        let (r, st) = parse("4").compound(24, rm);
        assert_eq!(text(r), want, "compound(4, 24) at {rm:?}");
        assert_eq!(st, Status::INEXACT);
    }

    // The negative-`n` mirror: 2^-23 carries the same coefficient at
    // the mirrored exponent.
    let down_neg = "1.192092895507812E-7";
    let up_neg = "1.192092895507813E-7";
    for (rm, want) in [
        (NE, down_neg),
        (NA, up_neg),
        (TZ, down_neg),
        (TP, up_neg),
        (TN, down_neg),
    ] {
        let (r, st) = parse("1").compound(-23, rm);
        assert_eq!(text(r), want, "compound(1, -23) at {rm:?}");
        assert_eq!(st, Status::INEXACT);
    }
    // One past the tie window the coefficient is too wide, so the
    // classifier declines and the ladder rounds it: still inexact, and
    // still correct, but by a different mechanism.
    let (_, st) = parse("4").compound(25, NE);
    assert_eq!(st, Status::INEXACT);
}

// ---------------------------------------------------------------------------
// The ADR-0051 anchor band.

/// The anchor arm delivers from the grid point 1 on the side theorem.
/// The nearest modes take 1 from both sides; the directed modes need
/// the side, and the boundaries beside 1 are asymmetric, so the two
/// directions are not mirror images: above 1, `TowardPositive` alone
/// moves to `next_up(1)`; below 1, `TowardZero` and
/// `TowardNegative` move to `next_down(1)`.
#[test]
fn anchor_band_delivers_by_the_side_theorem() {
    let one = "1.000000000000000";
    let next_up = "1.000000000000001";
    let next_down = "0.9999999999999999";
    // sign(n) = sign(x): the value is strictly above 1.
    for (x, n) in [("1E-25", 1), ("-1E-25", -1), ("1E-30", 7), ("-1E-35", -3)] {
        for (rm, want) in [(NE, one), (NA, one), (TZ, one), (TP, next_up), (TN, one)] {
            let (r, st) = parse(x).compound(n, rm);
            assert_eq!(text(r), want, "compound({x}, {n}) at {rm:?}");
            assert_eq!(st, Status::INEXACT, "compound({x}, {n}) at {rm:?}");
        }
    }
    // sign(n) ≠ sign(x): the value is strictly below 1.
    for (x, n) in [("1E-25", -1), ("-1E-25", 1), ("-1E-30", 7), ("1E-35", -3)] {
        for (rm, want) in [
            (NE, one),
            (NA, one),
            (TZ, next_down),
            (TP, one),
            (TN, next_down),
        ] {
            let (r, st) = parse(x).compound(n, rm);
            assert_eq!(text(r), want, "compound({x}, {n}) at {rm:?}");
            assert_eq!(st, Status::INEXACT, "compound({x}, {n}) at {rm:?}");
        }
    }
}

/// `n = i32::MAX` at the same tiny `x` is *not* in the anchor band —
/// the gate scales with `|n|`'s digit count, exactly so that a huge
/// multiplier cannot smuggle a resolvable value into it. The value is many boundaries away from 1 and the ladder
/// decides it, which is the correct outcome and the evidence the gate
/// is not merely "x is small".
#[test]
fn a_huge_n_leaves_the_anchor_band() {
    let (r, st) = parse("1E-25").compound(i32::MAX, NE);
    assert_eq!(text(r), "1.000000000000000");
    assert_eq!(st, Status::INEXACT);
    let (r, _) = parse("-1E-25").compound(i32::MAX, NE);
    assert_eq!(text(r), "0.9999999999999998");
}

// ---------------------------------------------------------------------------
// The huge-`x` anchor band: `(1 + x)^n` hugs `x^n`.

/// ADR-0060's second whole-range on-grid family for `compound`. Once
/// `x` outgrows the working width, `logp1`'s wide band absorbs the `1`
/// of `1 ⊕ x` entirely and the kernel is evaluating `x^n`, not
/// `(1 + x)^n`. Where `x^n` is itself a format grid point the true
/// value sits a relative `≈ n/x` off a rounding boundary — a distance
/// no fixed rung can grow — so the directed modes would be decided by
/// the sign of the kernel's own noise.
///
/// This is not hypothetical. Before `exact::compound_huge_x_anchor`
/// existed, `compound(1E+100, 1)` at `TowardPositive` delivered
/// `10^200` where the true value `10^200 + 1` is strictly above it and
/// `next_up(10^200)` is owed; the wider powers and the `3E+100` base were the
/// same shape. That last one happened to come out right, which is
/// worse rather than better: it was right by the sign of the noise,
/// not by construction.
///
/// The side theorem is `1 + x > x > 0`, so the value is above `x^n`
/// for `n > 0` and below it for `n < 0`.
#[test]
fn huge_x_hugs_the_pown_grid_point() {
    // `10^m`'s successor at this precision: 1, then PRECISION − 2
    // zeros, then 1.
    let next_up_of_power_of_ten = |m: i32| parse(&format!("1.{}1E{m}", "0".repeat(PRECISION - 2)));
    // `10^m`'s predecessor: PRECISION nines, one decade down.
    let next_down_of_power_of_ten = |m: i32| {
        parse(&format!(
            "{}E{}",
            "9".repeat(PRECISION),
            m - PRECISION as i32
        ))
    };

    for n in [1i32, 2, 3] {
        let x = parse(&format!("1E+{HUGE_EXP}"));
        let m = HUGE_EXP * n;
        let anchor = parse(&format!("1E{m}"));
        for rm in [NE, NA, TZ, TN] {
            let (r, st) = x.compound(n, rm);
            assert!(
                equal(r, anchor),
                "compound(1E+{HUGE_EXP}, {n}) at {rm:?}: got {r}, want 1E{m}"
            );
            assert_eq!(st, Status::INEXACT, "the true value is off the grid point");
        }
        let (r, st) = x.compound(n, TP);
        assert!(
            equal(r, next_up_of_power_of_ten(m)),
            "compound(1E+{HUGE_EXP}, {n}) at TowardPositive: got {r}, want \
             next_up(1E{m}) — the absorbed `1` of `1 ⊕ x` must not decide \
             this direction"
        );
        assert_eq!(st, Status::INEXACT);
    }

    // Negative `n` mirrors the side: the value is *below* `x^n`.
    let x = parse(&format!("1E+{HUGE_EXP}"));
    let m = -HUGE_EXP;
    let anchor = parse(&format!("1E{m}"));
    for rm in [NE, NA, TP] {
        let (r, st) = x.compound(-1, rm);
        assert!(
            equal(r, anchor),
            "compound(1E+{HUGE_EXP}, -1) at {rm:?}: {r}"
        );
        assert_eq!(st, Status::INEXACT);
    }
    for rm in [TZ, TN] {
        let (r, st) = x.compound(-1, rm);
        assert!(
            equal(r, next_down_of_power_of_ten(m)),
            "compound(1E+{HUGE_EXP}, -1) at {rm:?}: got {r}, want next_down(1E{m})"
        );
        assert_eq!(st, Status::INEXACT);
    }

    // A base that is not a power of ten: `x^1 = 3·10^m` is still a grid
    // point, so the family is about `x^n` landing on the grid, not
    // about `x` being a power of ten.
    let x = parse(&format!("3E+{HUGE_EXP}"));
    for rm in [NE, NA, TZ, TN] {
        let (r, _) = x.compound(1, rm);
        assert!(
            equal(r, parse(&format!("3E+{HUGE_EXP}"))),
            "compound(3E+{HUGE_EXP}, 1) at {rm:?}: got {r}"
        );
    }
    let (r, _) = x.compound(1, TP);
    assert!(
        equal(
            r,
            parse(&format!("3.{}1E+{HUGE_EXP}", "0".repeat(PRECISION - 2)))
        ),
        "compound(3E+{HUGE_EXP}, 1) at TowardPositive: got {r}"
    );
}

/// The threshold-straddling pair. At `Decimal64` with a one-digit `n`
/// the gate fires at adjusted exponent `≤ −(34 + 1 + 4) = −39`, so
/// `1E−39` is inside the band and `1E−38` is outside it, decided by the
/// ladder instead. Both are sound, and both must round identically in
/// every direction — the gate moves the *mechanism*, never the answer.
#[test]
fn the_gate_threshold_does_not_move_the_answer() {
    for n in [1i32, -1] {
        for rm in ALL {
            let (inside, st_in) = parse("1E-21").compound(n, rm);
            let (outside, st_out) = parse("1E-20").compound(n, rm);
            assert_eq!(
                text(inside),
                text(outside),
                "compound(1E-21, {n}) and compound(1E-20, {n}) at {rm:?} \
                 straddle the anchor gate and must agree"
            );
            assert_eq!(st_in, st_out, "and agree on flags");
            assert_eq!(st_in, Status::INEXACT);
        }
    }
}

// ---------------------------------------------------------------------------
// The financial sweep: the shape the operation exists for.

/// A few hundred `(x, n)` at the rates and periods `compound` is named
/// for, checked against two properties that need no external oracle:
/// strict monotonicity in `n` (the direction the side theorem states)
/// and clean flags (no `INVALID`, no `DIV_BY_ZERO`, and no spurious
/// range signals at these magnitudes). Deterministic, no sampling.
///
/// Monotonicity is a real check here rather than a tautology: each
/// `compound(x, n)` is an independent call through the classifier, the
/// anchor gate, or the ladder, with no shared state between them, so a
/// classifier that delivered a wrong coefficient for one `n` would
/// break the chain.
#[test]
fn financial_sweep_is_monotone_in_n_with_clean_flags() {
    for rate in ["0.0001", "0.001", "0.0125", "0.05", "0.1"] {
        let x = parse(rate);
        let mut prev = Decimal64::ONE;
        for n in 1..=360 {
            let (v, st) = x.compound(n, NE);
            assert!(
                !st.invalid() && !st.div_by_zero() && !st.overflow() && !st.underflow(),
                "compound({rate}, {n}): unexpected flags {st:?}"
            );
            assert!(
                v.partial_cmp(prev).0 == Some(core::cmp::Ordering::Greater),
                "compound({rate}, {n}) = {v} is not above compound({rate}, {}) = {prev}",
                n - 1
            );
            prev = v;
        }
    }
    for rate in ["-0.0001", "-0.01", "-0.1"] {
        let x = parse(rate);
        let mut prev = Decimal64::ONE;
        for n in 1..=360 {
            let (v, st) = x.compound(n, NE);
            assert!(
                !st.invalid() && !st.div_by_zero(),
                "compound({rate}, {n}): unexpected flags {st:?}"
            );
            assert!(
                v.partial_cmp(prev).0 == Some(core::cmp::Ordering::Less),
                "compound({rate}, {n}) = {v} is not below compound({rate}, {}) = {prev}",
                n - 1
            );
            prev = v;
        }
    }
}

/// The irrational-looking majority: a rate whose `1 + x` carries a
/// factor coprime to 10 has a value too wide to be exact once `n`
/// grows, so `INEXACT` must be raised — the complement of the exact
/// pins above, and the assertion that the classifier is not claiming
/// values it cannot prove.
#[test]
fn wide_results_are_inexact() {
    for (x, n) in [
        ("0.05", 40),
        ("0.05", 360),
        ("0.0125", 120),
        ("0.1", 100),
        ("-0.01", 50),
        ("0.5", -1),
        ("0.5", -3),
    ] {
        for rm in ALL {
            let (_, st) = parse(x).compound(n, rm);
            assert_eq!(
                st,
                Status::INEXACT,
                "compound({x}, {n}) at {rm:?}: expected a plain INEXACT"
            );
        }
    }
}

/// `n = 1` is `1 + x` exactly whenever that sum is representable, and
/// correctly rounded when it is not. The `1E−38` row is the second kind:
/// `1 + 10^−38` needs 39 digits, so it rounds to 1 in four directions
/// and to `next_up(1)` toward `+∞`.
#[test]
fn n_one_is_the_sum() {
    for literal in ["0.05", "1", "9", "-0.5", "1E-30", "-1E-30", "123.456"] {
        let x = parse(literal);
        let (want, want_st) = Decimal64::ONE.add(x, NE);
        let (got, got_st) = x.compound(1, NE);
        assert!(
            equal(got, want),
            "compound({literal}, 1) = {got}, want 1 + x = {want}"
        );
        assert_eq!(
            got_st, want_st,
            "compound({literal}, 1) flags disagree with the sum's"
        );
    }
}
