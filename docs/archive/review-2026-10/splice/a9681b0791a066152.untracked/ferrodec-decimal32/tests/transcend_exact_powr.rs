//! Special-value, exact-result, quantum, and `pow`-differential gate
//! for `Decimal32`'s `powr` (IEEE 754-2019 §9.2; ADR-0059 Track D D3).
//! The sibling mirror of the root crate's
//! `tests/transcend_exact_powr.rs`; the two differ only in the
//! format's precision and exponent range, which move the
//! `PRECISION + 1` tie, the widest cohort, and which powers of ten
//! stay in range. Decimal32's range is narrow enough to cost one
//! sub-assertion outright; the `pow` fast-path boundary witness is
//! not expressible here (see the §9.2.2 deviation test).
//!
//! `powr(x, y)` is `exp(y · ln x)` taken literally. It runs on `pow`'s
//! kernel — the same input side classifier, the same `ln` then multiply
//! then `exp` composition, the same escalation ladder — and differs
//! only in the §9.2.1 special value table applied before any
//! approximation runs. This file therefore has two jobs:
//!
//! 1. Pin every §9.2.1 `powr` row, and pin the four families where
//!    `powr` refuses a value `pow` supplies *beside the disagreeing
//!    `pow` call*, so the contrast is a test rather than a comment.
//! 2. Prove the shared-kernel claim by differential: over the domain
//!    both operations accept (`x` finite `> 0`, `y` finite non-zero),
//!    `powr` and `pow` must agree on every value and every flag. They
//!    do, without exception. They do NOT always agree on the cohort,
//!    for one understood reason recorded on the differential itself:
//!    `pow` opens with a square-and-multiply fast path that `powr`
//!    (per the D3 kernel spec) does not have, and on a power-of-ten
//!    result with `Q(x) ≠ 0` the two routes prefer different quanta.
//!    That bucket is pinned pair by pair, so it cannot widen unseen.
//!
//! The claim tier is `pow`'s and stays there: ADR-0060's Liouville
//! floors make the rest of the algebraic §9.2 group unconditional but
//! provably cannot lift `powr`, whose guarantee parameter is the
//! second operand's reduced denominator (unbounded in a format
//! operand). That is the ADR's stated negative result, not a gap this
//! file could close.

#![cfg(feature = "pow")]

use ferrodec_decimal32::{Decimal32, RoundingMode, Status};

const NE: RoundingMode = RoundingMode::NearestEven;
const NA: RoundingMode = RoundingMode::NearestAway;
const TZ: RoundingMode = RoundingMode::TowardZero;
const TP: RoundingMode = RoundingMode::TowardPositive;
const TN: RoundingMode = RoundingMode::TowardNegative;
const ALL: [RoundingMode; 5] = [NE, NA, TZ, TP, TN];

fn parse(s: &str) -> Decimal32 {
    Decimal32::parse_str(s, NE)
        .unwrap_or_else(|_| panic!("literal parses: {s:?}"))
        .0
}

/// Value equality, cohort insensitive (the IEEE `compare`).
fn equal(a: Decimal32, b: Decimal32) -> bool {
    a.partial_cmp(b).0 == Some(core::cmp::Ordering::Equal)
}

// ---------------------------------------------------------------------------
// The §9.2.1 table, row by row.
// ---------------------------------------------------------------------------

/// "powr(x, ±0) is 1 for finite x > 0".
#[test]
fn finite_positive_base_zero_exponent_is_one() {
    for x in ["2", "0.5", "1E+90", "1E-90", "123.456", "9.5"] {
        for z in [Decimal32::ZERO, Decimal32::NEG_ZERO] {
            for rm in ALL {
                let (r, s) = parse(x).powr(z, rm);
                assert!(equal(r, Decimal32::ONE), "powr({x}, ±0) = 1, got {r:?}");
                assert!(!s.inexact(), "powr({x}, ±0) is exact");
                assert!(!s.invalid(), "powr({x}, ±0) raises nothing");
            }
        }
    }
}

/// "powr(±0, y) is +∞ and signals the divideByZero exception for
/// finite y < 0", against "powr(±0, −∞) is +∞" with no exception.
/// Both are `exp(y · (−∞))`; only the finite exponent divides by zero.
#[test]
fn zero_base_negative_exponent_is_infinity() {
    for z in [Decimal32::ZERO, Decimal32::NEG_ZERO] {
        for y in ["-1", "-2", "-0.5", "-1E+90"] {
            for rm in ALL {
                let (r, s) = z.powr(parse(y), rm);
                assert!(
                    r.is_infinite() && !r.is_sign_negative(),
                    "powr(±0, {y}) = +∞"
                );
                assert!(s.div_by_zero(), "powr(±0, {y}) signals divideByZero");
            }
        }
        // The −∞ row: same +∞ result, no exception at all.
        for rm in ALL {
            let (r, s) = z.powr(Decimal32::NEG_INFINITY, rm);
            assert!(
                r.is_infinite() && !r.is_sign_negative(),
                "powr(±0, −∞) = +∞"
            );
            assert!(!s.div_by_zero(), "powr(±0, −∞) raises no divideByZero");
            assert_eq!(s, Status::OK, "powr(±0, −∞) raises nothing");
        }
    }
}

/// "powr(±0, y) is +0 for y > 0", `+∞` included
/// (`exp((+∞) · (−∞)) = +0`). The result is `+0` for either sign of the
/// zero base: `powr` has no odd-integer sign rule, so `powr(−0, 3)` is
/// `+0` where `pow(−0, 3)` is `−0`.
#[test]
fn zero_base_positive_exponent_is_positive_zero() {
    for z in [Decimal32::ZERO, Decimal32::NEG_ZERO] {
        for y in ["1", "2", "3", "0.5", "1E+90"] {
            for rm in ALL {
                let (r, s) = z.powr(parse(y), rm);
                assert!(r.is_zero(), "powr(±0, {y}) is zero");
                assert!(!r.is_sign_negative(), "powr(±0, {y}) = +0, not −0");
                assert_eq!(s, Status::OK, "powr(±0, {y}) raises nothing");
            }
        }
        let (r, s) = z.powr(Decimal32::INFINITY, NE);
        assert!(r.is_zero() && !r.is_sign_negative(), "powr(±0, +∞) = +0");
        assert_eq!(s, Status::OK);
    }
    // The sign-rule contrast with pow, pinned.
    let (pr, _) = Decimal32::NEG_ZERO.pow(parse("3"), NE);
    assert!(
        pr.is_sign_negative(),
        "pow(−0, 3) = −0 by the odd-integer rule"
    );
    let (qr, _) = Decimal32::NEG_ZERO.powr(parse("3"), NE);
    assert!(!qr.is_sign_negative(), "powr(−0, 3) = +0: no such rule");
}

/// "powr(+1, y) is 1 for finite y".
#[test]
fn unit_base_finite_exponent_is_one() {
    for y in ["1", "-1", "0.5", "-3.14", "1E+90", "-1E-300"] {
        for rm in ALL {
            let (r, s) = Decimal32::ONE.powr(parse(y), rm);
            assert!(equal(r, Decimal32::ONE), "powr(1, {y}) = 1, got {r:?}");
            assert!(!s.inexact(), "powr(1, {y}) is exact");
            assert_eq!(s, Status::OK, "powr(1, {y}) raises nothing");
        }
    }
}

/// "powr(x, y) signals the invalid operation exception for x < 0" —
/// for EVERY y, integer exponents included. The loudest contrast with
/// `pow`, which answers `pow(−2, 3) = −8`.
#[test]
fn negative_base_is_invalid_for_every_exponent() {
    let ys = ["3", "2", "-3", "0.5", "-0.5", "1", "0", "-0", "1E+90"];
    for x in ["-2", "-1", "-0.5", "-1E+90"] {
        for y in ys {
            for rm in ALL {
                let (r, s) = parse(x).powr(parse(y), rm);
                assert!(r.is_quiet_nan(), "powr({x}, {y}) = qNaN, got {r:?}");
                assert!(s.invalid(), "powr({x}, {y}) signals INVALID");
            }
        }
        // ±∞ and NaN exponents over a negative base are invalid too:
        // §9.2.1 states the quiet-NaN row only "for x ≥ 0".
        for y in [Decimal32::INFINITY, Decimal32::NEG_INFINITY, Decimal32::NAN] {
            let (r, s) = parse(x).powr(y, NE);
            assert!(r.is_quiet_nan(), "powr({x}, {y:?}) = qNaN");
            assert!(s.invalid(), "powr({x}, {y:?}) signals INVALID");
        }
    }
    // −∞ is < 0 and therefore invalid; −0 is NOT and takes the zero rows.
    let (r, s) = Decimal32::NEG_INFINITY.powr(parse("2"), NE);
    assert!(r.is_quiet_nan() && s.invalid(), "powr(−∞, 2) is invalid");
    let (r, s) = Decimal32::NEG_ZERO.powr(parse("2"), NE);
    assert!(r.is_zero() && !s.invalid(), "powr(−0, 2) = +0, not invalid");

    // The contrast: pow answers each integer-exponent case normally.
    for (x, y, want) in [("-2", "3", "-8"), ("-2", "2", "4"), ("-1", "3", "-1")] {
        let (pr, ps) = parse(x).pow(parse(y), NE);
        assert!(equal(pr, parse(want)), "pow({x}, {y}) = {want}");
        assert!(!ps.invalid(), "pow({x}, {y}) raises no INVALID");
        let (qr, qs) = parse(x).powr(parse(y), NE);
        assert!(qr.is_quiet_nan() && qs.invalid(), "powr({x}, {y}) refuses");
    }
}

/// The three indeterminate forms of `y · ln x`, each qNaN + INVALID
/// under `powr` and each `1` under `pow`. Tested beside the `pow` call
/// so the disagreement is pinned in one place.
#[test]
fn the_three_indeterminate_forms_contrast_with_pow() {
    let zeros = [Decimal32::ZERO, Decimal32::NEG_ZERO];
    let infs = [Decimal32::INFINITY, Decimal32::NEG_INFINITY];

    // "powr(±0, ±0) signals the invalid operation exception" — 0 · (−∞).
    for x in zeros {
        for y in zeros {
            for rm in ALL {
                let (r, s) = x.powr(y, rm);
                assert!(r.is_quiet_nan(), "powr(±0, ±0) = qNaN, got {r:?}");
                assert!(s.invalid(), "powr(±0, ±0) signals INVALID");
            }
            let (pr, ps) = x.pow(y, NE);
            assert!(equal(pr, Decimal32::ONE), "pow(±0, ±0) = 1");
            assert!(!ps.invalid(), "pow(±0, ±0) raises nothing");
        }
    }

    // "powr(+∞, ±0) signals the invalid operation exception" — 0 · (+∞).
    for y in zeros {
        for rm in ALL {
            let (r, s) = Decimal32::INFINITY.powr(y, rm);
            assert!(r.is_quiet_nan(), "powr(+∞, ±0) = qNaN, got {r:?}");
            assert!(s.invalid(), "powr(+∞, ±0) signals INVALID");
        }
        let (pr, ps) = Decimal32::INFINITY.pow(y, NE);
        assert!(equal(pr, Decimal32::ONE), "pow(+∞, ±0) = 1");
        assert!(!ps.invalid(), "pow(+∞, ±0) raises nothing");
    }

    // "powr(+1, ±∞) signals the invalid operation exception" — ±∞ · 0.
    for y in infs {
        for rm in ALL {
            let (r, s) = Decimal32::ONE.powr(y, rm);
            assert!(r.is_quiet_nan(), "powr(+1, ±∞) = qNaN, got {r:?}");
            assert!(s.invalid(), "powr(+1, ±∞) signals INVALID");
        }
        let (pr, ps) = Decimal32::ONE.pow(y, NE);
        assert!(equal(pr, Decimal32::ONE), "pow(+1, ±∞) = 1");
        assert!(!ps.invalid(), "pow(+1, ±∞) raises nothing");
        // pow(−1, ±∞) is also 1; powr refuses it as a negative base.
        let (pr, ps) = Decimal32::NEG_ONE.pow(y, NE);
        assert!(equal(pr, Decimal32::ONE), "pow(−1, ±∞) = 1");
        assert!(!ps.invalid());
        let (qr, qs) = Decimal32::NEG_ONE.powr(y, NE);
        assert!(qr.is_quiet_nan() && qs.invalid(), "powr(−1, ±∞) is invalid");
    }
}

/// The remaining infinite combinations, each read off `exp(y · ln x)`:
/// `powr(+∞, y>0) = +∞`, `powr(+∞, y<0) = +0`, and `powr(x, ±∞)`
/// decided by whether `ln x` is positive.
#[test]
fn infinite_operand_limits_follow_the_composition() {
    // powr(+∞, y) for y ≠ 0: exp(y · (+∞)).
    for y in ["1", "2", "0.5", "1E+90"] {
        let (r, s) = Decimal32::INFINITY.powr(parse(y), NE);
        assert!(
            r.is_infinite() && !r.is_sign_negative(),
            "powr(+∞, {y}) = +∞"
        );
        assert_eq!(s, Status::OK);
        let (r, s) = Decimal32::INFINITY.powr(parse(&format!("-{y}")), NE);
        assert!(r.is_zero() && !r.is_sign_negative(), "powr(+∞, -{y}) = +0");
        assert_eq!(s, Status::OK);
    }
    let (r, _) = Decimal32::INFINITY.powr(Decimal32::INFINITY, NE);
    assert!(r.is_infinite(), "powr(+∞, +∞) = +∞");
    let (r, _) = Decimal32::INFINITY.powr(Decimal32::NEG_INFINITY, NE);
    assert!(r.is_zero(), "powr(+∞, −∞) = +0");

    // powr(x, ±∞) for finite x > 0, x ≠ 1: ln x sets the sign.
    for x in ["2", "10", "1.5", "1E+90"] {
        let (r, s) = parse(x).powr(Decimal32::INFINITY, NE);
        assert!(
            r.is_infinite() && !r.is_sign_negative(),
            "powr({x}>1, +∞) = +∞"
        );
        assert_eq!(s, Status::OK);
        let (r, s) = parse(x).powr(Decimal32::NEG_INFINITY, NE);
        assert!(r.is_zero() && !r.is_sign_negative(), "powr({x}>1, −∞) = +0");
        assert_eq!(s, Status::OK);
    }
    for x in ["0.5", "0.1", "0.99999", "1E-90"] {
        let (r, s) = parse(x).powr(Decimal32::INFINITY, NE);
        assert!(r.is_zero() && !r.is_sign_negative(), "powr({x}<1, +∞) = +0");
        assert_eq!(s, Status::OK);
        let (r, s) = parse(x).powr(Decimal32::NEG_INFINITY, NE);
        assert!(
            r.is_infinite() && !r.is_sign_negative(),
            "powr({x}<1, −∞) = +∞"
        );
        assert_eq!(s, Status::OK);
    }
}

/// "powr(x, qNaN) is qNaN for x ≥ 0" and "powr(qNaN, y) is qNaN", with
/// no exception; a signaling NaN anywhere is qNaN + INVALID. Unlike
/// `pow`, no row preempts a NaN operand: `pow(NaN, 0)` and
/// `pow(1, NaN)` are both `1`, while the `powr` analogues are qNaN.
#[test]
fn nan_rows_have_no_preempting_row() {
    for x in ["0", "-0", "1", "2", "1E+90"] {
        let (r, s) = parse(x).powr(Decimal32::NAN, NE);
        assert!(r.is_quiet_nan(), "powr({x}, qNaN) = qNaN");
        assert_eq!(s, Status::OK, "powr({x}, qNaN) raises nothing");
        let (r, s) = parse(x).powr(Decimal32::SIGNALING_NAN, NE);
        assert!(r.is_quiet_nan(), "powr({x}, sNaN) = qNaN");
        assert!(s.invalid(), "powr({x}, sNaN) signals INVALID");
    }
    for y in ["0", "-0", "1", "2", "-2"] {
        let (r, s) = Decimal32::NAN.powr(parse(y), NE);
        assert!(r.is_quiet_nan(), "powr(qNaN, {y}) = qNaN");
        assert_eq!(s, Status::OK, "powr(qNaN, {y}) raises nothing");
        let (r, s) = Decimal32::SIGNALING_NAN.powr(parse(y), NE);
        assert!(r.is_quiet_nan(), "powr(sNaN, {y}) = qNaN");
        assert!(s.invalid(), "powr(sNaN, {y}) signals INVALID");
    }
    // NaN in both operands, and the ±∞ pairing.
    let (r, s) = Decimal32::NAN.powr(Decimal32::NAN, NE);
    assert!(r.is_quiet_nan() && s == Status::OK);
    let (r, s) = Decimal32::NAN.powr(Decimal32::SIGNALING_NAN, NE);
    assert!(r.is_quiet_nan() && s.invalid());

    // The contrast: pow's rules 1 and 2 fire through NaN, powr's do not.
    let (pr, _) = Decimal32::NAN.pow(Decimal32::ZERO, NE);
    assert!(equal(pr, Decimal32::ONE), "pow(NaN, 0) = 1");
    let (qr, _) = Decimal32::NAN.powr(Decimal32::ZERO, NE);
    assert!(qr.is_quiet_nan(), "powr(NaN, 0) = qNaN");
    let (pr, _) = Decimal32::ONE.pow(Decimal32::NAN, NE);
    assert!(equal(pr, Decimal32::ONE), "pow(1, NaN) = 1");
    let (qr, _) = Decimal32::ONE.powr(Decimal32::NAN, NE);
    assert!(qr.is_quiet_nan(), "powr(1, NaN) = qNaN");
}

// ---------------------------------------------------------------------------
// The exact family, through the shared classifier.
// ---------------------------------------------------------------------------

/// Exact rational powers deliver their exact value with no `INEXACT`,
/// in every rounding direction. These are the classifier's cases, and
/// `powr` reaches it on a strictly smaller domain than `pow` does, so
/// `pow`'s completeness proofs cover them verbatim.
#[test]
fn exact_powers_are_exact_in_every_mode() {
    let cases = [
        ("4", "0.5", "2"),
        ("2.25", "0.5", "1.5"),
        ("10", "90", "1E+90"),
        ("16", "-0.25", "0.5"),
        ("9", "0.5", "3"),
        ("100", "0.5", "10"),
        ("2", "3", "8"),
        ("0.2", "2", "0.04"),
        ("1.5", "3", "3.375"),
    ];
    for (x, y, want) in cases {
        for rm in ALL {
            let (r, s) = parse(x).powr(parse(y), rm);
            assert!(
                equal(r, parse(want)),
                "powr({x}, {y}) = {r:?}, want {want} under {rm:?}"
            );
            assert!(!s.inexact(), "powr({x}, {y}) must not raise INEXACT");
        }
    }
}

/// Irrational powers keep `INEXACT` — the guard against a classifier
/// that over-claims exactness.
#[test]
fn irrational_powers_are_inexact() {
    for (x, y) in [("2", "0.5"), ("3", "0.5"), ("2", "0.1"), ("7", "2.5")] {
        for rm in ALL {
            let (_, s) = parse(x).powr(parse(y), rm);
            assert!(s.inexact(), "powr({x}, {y}) must raise INEXACT");
        }
    }
}

/// The real `PRECISION + 1` tie: `5^11` is 8 digits ending in 5, one
/// digit past `Decimal32`'s 7, so its true value sits exactly on a
/// midpoint. Every rounding mode must land on its own side of it, and
/// the delivery is `INEXACT` in all five (a tie drops a nonzero digit).
///
/// `5^11 = 48828125`; the two neighbours are `4882812E+1` (even) and
/// `4882813E+1`.
#[test]
fn the_five_11_tie_resolves_per_mode() {
    let x = parse("5");
    let y = parse("11");
    let down = parse("4882812E+1");
    let up = parse("4882813E+1");
    let expect = [
        (NE, down, "NearestEven picks the even neighbour"),
        (NA, up, "NearestAway picks the larger magnitude"),
        (TZ, down, "TowardZero truncates"),
        (TP, up, "TowardPositive rounds up"),
        (TN, down, "TowardNegative rounds down"),
    ];
    for (rm, want, why) in expect {
        let (r, s) = x.powr(y, rm);
        assert!(equal(r, want), "powr(5, 11) under {rm:?}: {why}; got {r:?}");
        assert!(s.inexact(), "powr(5, 11) is a tie: INEXACT in every mode");
        // The tie is the classifier's, so powr and pow must agree on it.
        let (pr, ps) = x.pow(y, rm);
        assert!(
            equal(r, pr),
            "powr and pow agree on the 5^11 tie under {rm:?}"
        );
        assert_eq!(s, ps, "powr and pow agree on the tie flags under {rm:?}");
    }
}

// ---------------------------------------------------------------------------
// Preferred exponent (IEEE 754-2019 §9.2.2).
// ---------------------------------------------------------------------------

/// §9.2.2: "Q(powr(x, y)) is floor(y × Q(x))". The rule binds only on
/// exact deliveries; inexact results take the rounder's §6.3
/// disposition. These pins record the cohort `powr` actually delivers.
///
/// `powr` reaches the shared `exact::pow_exact_input` and never touches
/// the pack machinery, so the delivered quantum is whatever that
/// classifier asks for. It asks for `q_preferred = 0` unconditionally
/// (`pack_value`), and the rounder then moves the value as close to
/// quantum 0 as the 34-digit coefficient allows. Where the exact value
/// cannot sit at quantum 0, the clamp happens to land on
/// `floor(y × Q(x))` and the rule is satisfied; where `Q(x) ≠ 0` and the
/// value *can* reach quantum 0, the two disagree. See
/// [`the_ieee_9_2_2_preferred_exponent_deviation`] for that class,
/// which is inherited from `pow` rather than introduced here.
#[test]
fn exact_deliveries_pin_their_quantum() {
    // (x, y, delivered canonical string). The comment gives
    // floor(y × Q(x)) beside the delivered quantum.
    let cases = [
        // Q(4) = 0, y = 0.5 → floor = 0; delivered 2 at quantum 0. ✓
        ("4", "0.5", "2"),
        // Q(2.25) = −2, y = 0.5 → floor = −1; delivered 1.5 at −1. ✓
        ("2.25", "0.5", "1.5"),
        // Q(16) = 0, y = −0.25 → floor = 0; 0.5 cannot sit at quantum
        // 0, so the clamp lands on −1. §6.3 disposition.
        ("16", "-0.25", "0.5"),
        // Q(0.2) = −1, y = 2 → floor = −2; delivered 0.04 at −2. ✓
        ("0.2", "2", "0.04"),
        // Q(1.5) = −1, y = 3 → floor = −3; delivered 3.375 at −3. ✓
        ("1.5", "3", "3.375"),
        // Q(10) = 0, y = 90 → floor = 0; 10^90 needs a 91-digit
        // coefficient to sit at quantum 0, so the clamp lands on the
        // widest representable cohort, quantum 84.
        ("10", "90", "1.000000E+90"),
        // Q(2) = 0, y = 3 → floor = 0; delivered 8 at quantum 0. ✓
        ("2", "3", "8"),
    ];
    for (x, y, want) in cases {
        let (r, _) = parse(x).powr(parse(y), NE);
        assert_eq!(
            alloc_string(r),
            want,
            "powr({x}, {y}) cohort drifted (§9.2.2 pin)"
        );
    }
}

/// The §9.2.2 deviation, pinned as a **known** deviation so it cannot
/// drift unnoticed and so the next reader finds it stated rather than
/// buried.
///
/// `exact::pow_exact_input` hands the rounder `q_preferred = 0`
/// unconditionally instead of `floor(y × Q(x))`. When `Q(x) ≠ 0` and
/// the exact value can reach quantum 0, the delivered cohort is
/// therefore wider than §9.2.2 asks for. This predates `powr`: `pow`
/// misses the rule identically wherever its own integer fast path does
/// not apply. The parent file pins that with a `y = 256` / `y = 257`
/// pair straddling `pow`'s fast-path boundary; `Decimal32`'s exponent
/// range cannot host such a witness, so only the deviation itself is
/// pinned here (see the note at the end of this test).
///
/// Fixing this means changing `pack_value`'s preferred quantum for
/// every consumer of the shared classifier (`pow` and `cbrt` included).
/// That is a deliberate non-goal of this slice: the D3 brief forbids
/// adjusting the pack machinery, and the change is `pow`'s to make.
#[test]
fn the_ieee_9_2_2_preferred_exponent_deviation() {
    // Q(x) ≠ 0 and the value reaches quantum 0: §9.2.2 wants
    // floor(y × Q(x)), the classifier delivers quantum 0.
    let deviating = [
        // Q = 2, y = 2 → floor = 4; §9.2.2 wants "1E+4".
        ("1E+2", "2", "10000"),
        // Q = −1, y = −2 → floor = 2; §9.2.2 wants "1E+2".
        ("0.1", "-2", "100"),
        // Q = 2, y = 0.5 → floor = 1; §9.2.2 wants "1E+1".
        ("1E+2", "0.5", "10"),
    ];
    for (x, y, delivered) in deviating {
        let (r, _) = parse(x).powr(parse(y), NE);
        assert_eq!(
            alloc_string(r),
            delivered,
            "powr({x}, {y}): the known §9.2.2 deviation changed shape"
        );
    }

    // The parent file pins `pow`'s inconsistency across its own
    // `|y| ≤ 256` fast-path boundary. That witness is NOT expressible
    // at `Decimal32`: making the cohort split visible needs
    // `Q(x) ≠ 0`, and then `|y| ≥ 256` forces a result exponent past
    // 256, far outside this format's emax of 96. Both sides of the
    // boundary saturate to `±∞` here, so there is nothing to compare.
    // The deviation itself is pinned above; only the boundary witness
    // is format-limited.
    let (r, s) = parse("1E+1").pow(parse("256"), NE);
    assert!(
        r.is_infinite() && s.overflow(),
        "1E+1 ^ 256 leaves Decimal32"
    );
}

/// `Display` render, the cohort-sensitive view the quantum pins need.
fn alloc_string(d: Decimal32) -> std::string::String {
    std::format!("{d}")
}

// ---------------------------------------------------------------------------
// The shared-kernel differential.
// ---------------------------------------------------------------------------

/// `powr` and `pow` run the identical kernel on the domain both accept
/// (`x` finite `> 0`, `y` finite non-zero): the same classifier call,
/// the same `exp(y · ln x)` composition, budgets with identical
/// constants. Over a deterministic sweep of that domain the two must
/// therefore agree on **value and on flags**, with no exception. That
/// is the load-bearing claim, and it is asserted per comparison.
///
/// Bit identity is a *stronger* claim and it does not hold, for one
/// understood reason. `pow` opens with a square-and-multiply fast path
/// for integer `|y| ≤ 256`, committed only when no intermediate
/// multiply rounds; `powr` has no such arm (the D3 brief specifies the
/// kernel as table → classifier → general path), so an input the fast
/// path answers reaches `powr`'s classifier instead. The two agree on
/// the value and disagree on the cohort exactly when the exact result
/// is a power of ten and `Q(x) ≠ 0`: `int_pow` accumulates exponents
/// and keeps the coefficient `1`, while the classifier asks the rounder
/// for quantum 0 and widens the coefficient. Neither is wrong about the
/// value; see [`the_ieee_9_2_2_preferred_exponent_deviation`] for which
/// one §9.2.2 prefers (`pow`'s, on this family, by accident of the fast
/// path).
///
/// The three buckets are counted separately and pinned exactly, per the
/// repo's regression-guard discipline: a floor on the total would let a
/// value regression hide behind a cohort improvement.
#[test]
fn differential_against_pow_over_the_shared_domain() {
    let xs = [
        "2", "3", "7", "10", "0.5", "2.25", "4", "16", "1.5", "0.1", "123.456", "1.000001",
        "9.999999", "1E+50", "1E-50", "1",
    ];
    let ys = [
        "2", "3", "49", "-2", "-0.25", "0.5", "0.1", "300", "-3", "1.5", "-0.5", "7", "-1",
        "0.001", "-1E+3",
    ];
    // The ten (x, y) pairs where the cohort, and only the cohort,
    // differs: every one has a power-of-ten exact result, `Q(x) ≠ 0`,
    // and an integer `y` inside pow's `|y| ≤ 256` fast-path window.
    // Four pairs here against the parent's ten: `Decimal32`'s emax of
    // 96 sends every `1E+50 ^ n` and every `1E-50 ^ -n` past `n = 1`
    // to `±∞` or to zero, where both operations agree, leaving only
    // the small positive powers of ten in this bucket.
    let cohort_only: [(&str, &str); 4] =
        [("0.1", "-1"), ("0.1", "-2"), ("0.1", "-3"), ("1E-50", "-1")];

    let mut compared = 0usize;
    let mut value_divergent = 0usize;
    let mut flag_divergent = 0usize;
    let mut bit_divergent = 0usize;
    for x in xs {
        for y in ys {
            let bx = parse(x);
            let by = parse(y);
            let expected_cohort_split = cohort_only.contains(&(x, y));
            for rm in ALL {
                let (a, sa) = bx.powr(by, rm);
                let (b, sb) = bx.pow(by, rm);
                // The load-bearing pair of assertions.
                assert!(
                    equal(a, b) || (a.is_nan() && b.is_nan()),
                    "powr({x}, {y}) under {rm:?} = {a:?}, pow = {b:?}: VALUES differ"
                );
                assert_eq!(sa, sb, "powr({x}, {y}) under {rm:?}: FLAGS differ");
                if !equal(a, b) {
                    value_divergent += 1;
                }
                if sa != sb {
                    flag_divergent += 1;
                }
                if a.to_bits() != b.to_bits() {
                    bit_divergent += 1;
                    assert!(
                        expected_cohort_split,
                        "powr({x}, {y}) under {rm:?}: unlisted cohort split \
                         ({} vs {}) — the fast-path family widened",
                        alloc_string(a),
                        alloc_string(b)
                    );
                }
                compared += 1;
            }
        }
    }
    assert_eq!(compared, 16 * 15 * 5, "sweep cardinality drifted");
    assert!(compared >= 400, "the sweep must be a few hundred inputs");
    assert_eq!(
        value_divergent, 0,
        "powr and pow must never differ in value"
    );
    assert_eq!(flag_divergent, 0, "powr and pow must never differ in flags");
    assert_eq!(
        bit_divergent,
        cohort_only.len() * ALL.len(),
        "the cohort-split bucket drifted from its four pinned pairs"
    );
}

/// The differential's complement: on the inputs `powr` refuses, the two
/// operations must NOT agree. Without this the differential above could
/// pass on an accidental alias of `pow`.
#[test]
fn the_differential_does_not_extend_past_the_shared_domain() {
    let divergent = [
        ("-2", "3"),
        ("-2", "2"),
        ("0", "0"),
        ("-0", "-0"),
        ("1", "Inf"),
    ];
    for (x, y) in divergent {
        let bx = parse(x);
        let by = if y == "Inf" {
            Decimal32::INFINITY
        } else {
            parse(y)
        };
        let (a, sa) = bx.powr(by, NE);
        let (b, sb) = bx.pow(by, NE);
        assert!(a.is_quiet_nan(), "powr({x}, {y}) refuses");
        assert!(sa.invalid(), "powr({x}, {y}) signals INVALID");
        assert!(!b.is_nan(), "pow({x}, {y}) delivers a value");
        assert!(!sb.invalid(), "pow({x}, {y}) raises no INVALID");
    }
}
