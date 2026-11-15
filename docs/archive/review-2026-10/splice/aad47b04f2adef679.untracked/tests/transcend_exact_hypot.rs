//! Exact-result, special-value, band-structure, symmetry, and flag
//! gate for `Decimal128`'s `hypot` (IEEE 754-2019 §9.2; ADR-0060
//! Track D D3).
//!
//! `hypot` is the first §9.2 operation in this crate with a *rich*
//! exact set: every scaled Pythagorean pair belongs to it, at every
//! cohort and every decade. The classifier decides that set from the
//! operands alone — the aligned integer `S = A² + B²` is a perfect
//! square exactly when the true value is rational (Niven) — and this
//! file is its standing witness, together with the two band
//! constants the ADR's "constant bookkeeping error" failure mode
//! attacks.
//!
//! Four things are gated here that no corpus of sampled vectors can
//! reach:
//!
//! 1. **The quantum.** §9.2.2 pins `Q(hypot(x, y))` at
//!    `min(Q(x), Q(y))` on every exact delivery, which is a cohort
//!    property invisible to a value-only comparison.
//! 2. **The planted tie.** A hypotenuse of exactly `P + 1` digits
//!    ending in 5 *is* a rounding boundary; the approximation kernel
//!    cannot resolve it at any width, and only the classifier's exact
//!    delivery makes the five modes come out right.
//! 3. **The anchor band's MAX edge.** At `|w| = MAX` the true value
//!    sits above the largest finite value but below the §7.4 nearest
//!    overflow threshold, so `TowardPositive` must overflow while
//!    every other direction must not — a one-input distinction
//!    between two §7.4 dispositions.
//! 4. **Symmetry.** `hypot(x, y)` and `hypot(y, x)` must agree
//!    bit-for-bit, which is a property of the operand *ordering*
//!    logic rather than of the arithmetic.
//!
//! The correctly-rounded claim in the kernel band is additionally
//! checked here against an independent exact oracle: the true value
//! is `sqrt(S · 10^(2q))` with `S` an exact `u128` integer, and
//! `ferrodec_test_support::oracle::sqrt` rounds it with big-integer
//! arithmetic and a freshly transcribed §4.3.3 decision table. The
//! oracle's GDA ideal exponent for square root, `floor(2q/2) = q`, is
//! exactly §9.2.2's preferred quantum for `hypot`, so that sweep
//! gates the cohort as well as the value.

#![cfg(feature = "exp-log")]

use ferrodec::{Decimal128, RoundingMode, Status};
use ferrodec_test_support::conformance::status_conformance_eq;
use ferrodec_test_support::oracle::{self, parse_decimal, Expect, Format, Rounded};

const NE: RoundingMode = RoundingMode::NearestEven;
const NA: RoundingMode = RoundingMode::NearestAway;
const TZ: RoundingMode = RoundingMode::TowardZero;
const TP: RoundingMode = RoundingMode::TowardPositive;
const TN: RoundingMode = RoundingMode::TowardNegative;
const ALL: [RoundingMode; 5] = [NE, NA, TZ, TP, TN];

/// `Decimal128` precision, and the anchor band's ratio exponent
/// `δ₀ = ⌈(P + 2)/2⌉` derived from it. An off-by-one in `δ₀` is the
/// ADR-0060 failure mode this file's band tests attack.
const PRECISION: u32 = 34;
const DELTA0: i32 = 18;

/// The largest finite `Decimal128`.
const MAX: &str = "9.999999999999999999999999999999999E+6144";

fn parse(s: &str) -> Decimal128 {
    Decimal128::parse_str(s, NE)
        .unwrap_or_else(|_| panic!("literal parses: {s:?}"))
        .0
}

/// Value equality, cohort insensitive (the IEEE `compare`).
fn equal(a: Decimal128, b: Decimal128) -> bool {
    a.partial_cmp(b).0 == Some(core::cmp::Ordering::Equal)
}

/// Bit-for-bit equality, cohort included. What the quantum pins and
/// the symmetry differential need.
fn identical(a: Decimal128, b: Decimal128) -> bool {
    a.to_bits() == b.to_bits()
}

#[test]
fn the_band_constant_matches_its_formula() {
    assert_eq!(
        DELTA0,
        (PRECISION as i32 + 2).div_euclid(2) + (PRECISION as i32 + 2).rem_euclid(2),
        "δ₀ must be ⌈(P + 2)/2⌉"
    );
}

// ---------------------------------------------------------------------------
// §9.2.1, every row, both operand orders, every rounding direction.

/// "`hypot(±0, ±0)` is `+0`" — positive whatever the operand signs,
/// exact, exception free. §9.2.2 additionally pins the quantum at
/// `min(Q(x), Q(y))`, which the third pair exercises.
#[test]
fn zero_zero_is_positive_zero() {
    for (a, b, want) in [
        ("0", "0", "0"),
        ("-0", "0", "0"),
        ("0", "-0", "0"),
        ("-0", "-0", "0"),
        ("0", "-0.00", "0.00"),
        ("0.000", "0", "0.000"),
    ] {
        for rm in ALL {
            for (p, q) in [(a, b), (b, a)] {
                let (r, st) = parse(p).hypot(parse(q), rm);
                assert!(r.is_zero(), "hypot({p}, {q}) @{rm:?}: not zero");
                assert!(
                    !r.is_sign_negative(),
                    "hypot({p}, {q}) @{rm:?}: sign must be positive"
                );
                assert!(
                    identical(r, parse(want)),
                    "hypot({p}, {q}) @{rm:?}: quantum, got {r:e} want {want}"
                );
                assert_eq!(st, Status::OK, "hypot({p}, {q}) @{rm:?}: flags");
            }
        }
    }
}

/// "`hypot(±∞, qNaN)` is `+∞`" and "`hypot(qNaN, ±∞)` is `+∞`" — the
/// standard's explicit exception to NaN propagation, tested in both
/// orders because the exception is what makes the operand order
/// irrelevant here.
#[test]
fn infinity_beats_quiet_nan_both_orders() {
    for inf in [Decimal128::INFINITY, Decimal128::NEG_INFINITY] {
        for rm in ALL {
            for (p, q) in [(inf, Decimal128::NAN), (Decimal128::NAN, inf)] {
                let (r, st) = p.hypot(q, rm);
                assert!(
                    r.is_infinite() && !r.is_sign_negative(),
                    "hypot with ∞ and qNaN @{rm:?}: got {r}"
                );
                assert_eq!(st, Status::OK, "hypot with ∞ and qNaN @{rm:?}: flags");
            }
        }
    }
}

/// Any `±∞` operand gives `+∞`: against finite values of either sign,
/// against zeros, and against the other infinity.
#[test]
fn any_infinity_gives_positive_infinity() {
    let others = [
        Decimal128::INFINITY,
        Decimal128::NEG_INFINITY,
        Decimal128::ZERO,
        Decimal128::NEG_ZERO,
        parse("1"),
        parse("-1"),
        parse(MAX),
        parse("1E-6176"),
    ];
    for inf in [Decimal128::INFINITY, Decimal128::NEG_INFINITY] {
        for other in others {
            for rm in ALL {
                for (p, q) in [(inf, other), (other, inf)] {
                    let (r, st) = p.hypot(q, rm);
                    assert!(
                        r.is_infinite() && !r.is_sign_negative(),
                        "hypot({p}, {q}) @{rm:?}: got {r}"
                    );
                    assert_eq!(st, Status::OK, "hypot({p}, {q}) @{rm:?}: flags");
                }
            }
        }
    }
}

/// A signaling NaN anywhere quiets to a NaN and raises `INVALID`, and
/// it outranks the infinity rule above. §9.2.1 states the infinity
/// exception for a *quiet* NaN operand; §6.2 makes a signaling NaN
/// signal `INVALID` for every general-computational operation, and
/// §7.2 gives that precedence. This crate's other two-operand kernels
/// (`atan2`, `pow`) resolve the same collision in the same order, and
/// this test is the pin that keeps them agreeing.
#[test]
fn signaling_nan_outranks_the_infinity_rule() {
    let s = Decimal128::SIGNALING_NAN;
    let others = [
        Decimal128::INFINITY,
        Decimal128::NEG_INFINITY,
        Decimal128::NAN,
        Decimal128::SIGNALING_NAN,
        Decimal128::ZERO,
        parse("3"),
    ];
    for other in others {
        for rm in ALL {
            for (p, q) in [(s, other), (other, s)] {
                let (r, st) = p.hypot(q, rm);
                assert!(r.is_nan(), "hypot(sNaN, …) @{rm:?}: got {r}");
                assert!(
                    !r.is_signaling_nan(),
                    "hypot(sNaN, …) @{rm:?}: result must be quiet"
                );
                assert_eq!(st, Status::INVALID, "hypot(sNaN, …) @{rm:?}: flags");
            }
        }
    }
}

/// A quiet NaN with a finite other operand propagates, exception free.
#[test]
fn quiet_nan_with_finite_propagates() {
    for other in [
        Decimal128::ZERO,
        Decimal128::NEG_ZERO,
        parse("3"),
        parse("-3"),
        parse(MAX),
    ] {
        for rm in ALL {
            for (p, q) in [(Decimal128::NAN, other), (other, Decimal128::NAN)] {
                let (r, st) = p.hypot(q, rm);
                assert!(r.is_nan(), "hypot(qNaN, {other}) @{rm:?}: got {r}");
                assert_eq!(st, Status::OK, "hypot(qNaN, {other}) @{rm:?}: flags");
            }
        }
    }
}

/// `hypot(x, ±0) = |x|` exactly, both operand orders, with the
/// §9.2.2 quantum `min(Q(x), Q(0))` — so a zero with a *smaller*
/// quantum than `x` re-expresses the magnitude at that quantum, which
/// the `("3", "0.000", "3.000")` row is the witness for. No exception:
/// §7.5 forbids `INEXACT` on an exact result.
#[test]
fn zero_operand_delivers_the_exact_magnitude() {
    for (x, z, want) in [
        ("3", "0", "3"),
        ("-3", "0", "3"),
        ("3", "-0", "3"),
        ("-3", "-0", "3"),
        ("3.00", "0", "3.00"),
        ("3", "0.000", "3.000"),
        ("-1.25E+300", "0E+300", "1.25E+300"),
        ("1E-6176", "0", "1E-6176"),
        (MAX, "0", MAX),
    ] {
        for rm in ALL {
            for (p, q) in [(x, z), (z, x)] {
                let (r, st) = parse(p).hypot(parse(q), rm);
                assert!(
                    identical(r, parse(want)),
                    "hypot({p}, {q}) @{rm:?}: got {r:e}, want {want}"
                );
                assert_eq!(st, Status::OK, "hypot({p}, {q}) @{rm:?}: flags");
            }
        }
    }
}

/// The result is always positive: neither operand's sign reaches it.
#[test]
fn the_result_never_carries_an_operand_sign() {
    for (a, b) in [("3", "4"), ("3", "5"), ("1", "1E-30"), ("2.5", "7.5")] {
        for sa in ["", "-"] {
            for sb in ["", "-"] {
                for rm in ALL {
                    let p = format!("{sa}{a}");
                    let q = format!("{sb}{b}");
                    let (r, _) = parse(&p).hypot(parse(&q), rm);
                    assert!(
                        !r.is_sign_negative(),
                        "hypot({p}, {q}) @{rm:?}: got a negative {r}"
                    );
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The exact family: scaled Pythagorean pairs, with the §9.2.2 quantum.

/// Pythagorean pairs deliver the exact hypotenuse, bit-for-bit
/// (quantum included) and exception free, in every rounding direction
/// and both operand orders. The rows cover the primitive triples, a
/// shared decade shift, a fractional cohort, a trailing-zero cohort
/// (`3.00 / 4.00`, whose §9.2.2 quantum is `−2`), and the
/// `S` trailing-zero case `6² + 8² = 100` whose stripped root is `1`
/// with `k = 2` — the classifier's even-`k` arm.
#[test]
fn exact_pythagorean_family() {
    for (a, b, want) in [
        ("3", "4", "5"),
        ("6", "8", "10"),
        ("5", "12", "13"),
        ("8", "15", "17"),
        ("7", "24", "25"),
        ("20", "21", "29"),
        ("9", "40", "41"),
        ("0.3", "0.4", "0.5"),
        ("3.00", "4.00", "5.00"),
        ("0.005", "0.012", "0.013"),
        ("3E+10", "4E+10", "5E+10"),
        ("3E-10", "4E-10", "5E-10"),
        ("3E+6000", "4E+6000", "5E+6000"),
        ("3E-6176", "4E-6176", "5E-6176"),
        // Mixed quanta: the alignment shift is what the classifier's
        // integer widths are derived for.
        ("30", "0.4E+2", "50"),
        ("3", "0.004E+3", "5"),
        // A 33-digit leg pair: the widest exact family still inside
        // the format.
        (
            "600000000000000000000000000000000",
            "800000000000000000000000000000000",
            "1000000000000000000000000000000000",
        ),
    ] {
        for rm in ALL {
            for (p, q) in [(a, b), (b, a)] {
                let (r, st) = parse(p).hypot(parse(q), rm);
                assert!(
                    identical(r, parse(want)),
                    "hypot({p}, {q}) @{rm:?}: got {r:e}, want {want}"
                );
                assert_eq!(st, Status::OK, "hypot({p}, {q}) @{rm:?}: flags");
            }
        }
    }
}

/// §9.2.2's quantum is *preferred*, not mandatory: when
/// `min(Q(x), Q(y))` is further below the value than `P` digits can
/// express, the rounder moves as far toward it as the precision
/// allows and the result stays exact. `hypot(1E+6144, 0E-6176)` is the
/// extreme row — a preferred quantum 12 320 decades below the value —
/// and must deliver the exact magnitude padded to the full 34 digits
/// with a clean status, not an overflow and not an `INEXACT`.
#[test]
fn an_unreachable_preferred_quantum_clamps_without_losing_exactness() {
    for (a, b, want) in [
        (
            "1E+6144",
            "0E-6176",
            "1.000000000000000000000000000000000E+6144",
        ),
        (MAX, "0E-6176", MAX),
        (
            "3E+10",
            "0E-6176",
            "3.000000000000000000000000000000000E+10",
        ),
        ("1E-6176", "0E+6111", "1E-6176"),
        // A zero result takes any quantum, so the minimum applies in
        // full.
        ("0E+6111", "0E-6176", "0E-6176"),
    ] {
        for rm in ALL {
            for (p, q) in [(a, b), (b, a)] {
                let (r, st) = parse(p).hypot(parse(q), rm);
                assert!(
                    identical(r, parse(want)),
                    "hypot({p}, {q}) @{rm:?}: got {r:e}, want {want}"
                );
                assert_eq!(st, Status::OK, "hypot({p}, {q}) @{rm:?}: flags {st:?}");
            }
        }
    }
}

/// §9.2.2 in isolation: the delivered quantum of an exact result is
/// `min(Q(x), Q(y))` and nothing else. `hypot(6, 8) = 10` is the
/// sharpest row — the natural cohort of the classifier's own
/// `W · 10^(q + k/2)` form is `1E+1`, and only the preferred quantum
/// brings it back to `10`.
#[test]
fn preferred_exponent_is_the_minimum_quantum() {
    for (a, b, want) in [
        ("6", "8", "10"),
        ("6.0", "8", "10.0"),
        ("6", "8.00", "10.00"),
        ("60", "80", "100"),
        ("3.00", "4.0", "5.00"),
        ("3", "4.000000", "5.000000"),
    ] {
        for rm in ALL {
            for (p, q) in [(a, b), (b, a)] {
                let (r, _) = parse(p).hypot(parse(q), rm);
                assert!(
                    identical(r, parse(want)),
                    "hypot({p}, {q}) @{rm:?}: quantum, got {r:e}, want {want}"
                );
            }
        }
    }
}

/// The planted nearest-mode tie, the one input class the
/// approximation kernel provably cannot resolve: with
/// `k = 2000…001` (34 digits, odd) the legs `3k` and `4k` are
/// representable and the hypotenuse `5k` has exactly `P + 1 = 35`
/// digits ending in 5 — it *is* the midpoint between two adjacent
/// `Decimal128` values. Only the classifier's exact delivery makes
/// the five directions come out right; a kernel error of any sign
/// would pick an arbitrary side.
#[test]
fn the_planted_tie_resolves_by_the_rounders_own_rule() {
    let a = "6000000000000000000000000000000003";
    let b = "8000000000000000000000000000000004";
    let down = "1.000000000000000000000000000000000E+34";
    let up = "1.000000000000000000000000000000001E+34";
    for (rm, want) in [(NE, down), (NA, up), (TZ, down), (TP, up), (TN, down)] {
        for (p, q) in [(a, b), (b, a)] {
            let (r, st) = parse(p).hypot(parse(q), rm);
            assert!(
                identical(r, parse(want)),
                "tie hypot({p}, {q}) @{rm:?}: got {r:e}, want {want}"
            );
            assert_eq!(
                st,
                Status::INEXACT,
                "tie hypot({p}, {q}) @{rm:?}: a tie is inexact in every direction"
            );
        }
    }
}

/// An exact value past the format's range is delivered by the §7.4
/// disposition, not by the exact path: `hypot(6E+6144, 8E+6144)` is
/// exactly `1E+6145`, one decade above `MAX`. The classifier hands
/// the true value to the rounder and every direction answers
/// correctly, exactly as `exp10`'s above-range powers of ten do.
#[test]
fn an_exact_value_past_the_range_takes_the_overflow_disposition() {
    for (rm, want_inf) in [(NE, true), (NA, true), (TZ, false), (TP, true), (TN, false)] {
        for (p, q) in [("6E+6144", "8E+6144"), ("8E+6144", "6E+6144")] {
            let (r, st) = parse(p).hypot(parse(q), rm);
            if want_inf {
                assert!(r.is_infinite(), "hypot({p}, {q}) @{rm:?}: got {r}");
            } else {
                assert!(
                    identical(r, parse(MAX)),
                    "hypot({p}, {q}) @{rm:?}: got {r:e}, want MAX"
                );
            }
            assert!(
                st.overflow() && st.inexact(),
                "hypot({p}, {q}) @{rm:?}: flags {st:?}"
            );
        }
    }
}

/// Non-Pythagorean neighbours of the exact family: irrational by
/// Niven, so `INEXACT` in all five directions and never on a grid
/// point. The pairs sit one leg away from a triple, which is where a
/// classifier that over-claimed would break first.
#[test]
fn non_pythagorean_neighbours_are_inexact_everywhere() {
    for (a, b) in [
        ("3", "5"),
        ("3", "3"),
        ("4", "4"),
        ("5", "13"),
        ("6", "7"),
        ("20", "22"),
        ("0.3", "0.5"),
        ("3E+10", "5E+10"),
        ("1", "2"),
    ] {
        for rm in ALL {
            for (p, q) in [(a, b), (b, a)] {
                let (_, st) = parse(p).hypot(parse(q), rm);
                assert_eq!(
                    st,
                    Status::INEXACT,
                    "hypot({p}, {q}) @{rm:?}: an irrational value must be inexact"
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The ADR-0060 near-sharp families — the planted corpus the future
// exact integer adjudicator will be exercised against.
//
// These are NOT ordinary samples. `S = k² + 1` puts the true value
// just above the grid point `k` (offset ~`1/(2k)`), and `S = m⁴ + m²`
// (the `S = k² + k` shape at `k = m²`) puts it just above `m²`. Both
// approach a representable value from above as the scale grows, so
// they are the constructions that come closest to attaining the
// ADR-0060 Engine B floor. Keep them: the adjudicator slice needs
// real planted inputs, and these are them.

/// `S = k² + 1`: `hypot(k, 1)` hugs the grid point `k` from above.
/// Inexact in every direction, and the two directed modes must
/// straddle — `TowardPositive` strictly above `TowardZero` — which is
/// the observable form of "the kernel resolved the side".
#[test]
fn near_sharp_k_squared_plus_one() {
    for k in [
        "3",
        "10",
        "99",
        "1000",
        "9999999999999999",
        "1000000000000000000000000000000000",
        "3E+3000",
        "7E-3000",
    ] {
        let one = parse("1");
        let mut sides = [Decimal128::ZERO; 5];
        for (i, rm) in ALL.into_iter().enumerate() {
            let (r, st) = parse(k).hypot(one, rm);
            assert_eq!(st, Status::INEXACT, "hypot({k}, 1) @{rm:?}: flags");
            let (r2, st2) = one.hypot(parse(k), rm);
            assert!(
                identical(r, r2) && st == st2,
                "hypot({k}, 1) @{rm:?}: asymmetric"
            );
            sides[i] = r;
        }
        let (tz, tp) = (sides[2], sides[3]);
        assert!(
            tz.partial_cmp(tp).0 == Some(core::cmp::Ordering::Less),
            "hypot({k}, 1): TowardZero {tz:e} must sit strictly below TowardPositive {tp:e}"
        );
        // TowardNegative agrees with TowardZero on a positive result,
        // and the nearest modes land on one of the two sides.
        assert!(identical(sides[4], tz), "hypot({k}, 1): TN ≠ TZ");
        assert!(
            identical(sides[0], tz) || identical(sides[0], tp),
            "hypot({k}, 1): NE off the bracket"
        );
        assert!(
            identical(sides[1], tz) || identical(sides[1], tp),
            "hypot({k}, 1): NA off the bracket"
        );
    }
}

/// `S = k² + k` at `k = m²`, i.e. `hypot(m², m)`: `S = m⁴ + m²` and
/// the true value `m·sqrt(m² + 1)` hugs the grid point `m²` from
/// above by ~`1/2`. The second near-attaining family of ADR-0060.
#[test]
fn near_sharp_k_squared_plus_k() {
    for m in ["3", "10", "1000", "99999999", "1E+8", "1E+1000"] {
        let m_d = parse(m);
        let (m_sq, sq_st) = m_d.mul(m_d, NE);
        assert_eq!(sq_st, Status::OK, "m² must be exact for the construction");
        for rm in ALL {
            let (r, st) = m_sq.hypot(m_d, rm);
            assert_eq!(st, Status::INEXACT, "hypot({m}², {m}) @{rm:?}: flags");
            let (r2, st2) = m_d.hypot(m_sq, rm);
            assert!(
                identical(r, r2) && st == st2,
                "hypot({m}², {m}) @{rm:?}: asymmetric"
            );
        }
        let (tz, _) = m_sq.hypot(m_d, TZ);
        let (tp, _) = m_sq.hypot(m_d, TP);
        assert!(
            tz.partial_cmp(tp).0 == Some(core::cmp::Ordering::Less),
            "hypot({m}², {m}): the directed modes must straddle"
        );
        // The true value is strictly above m², so every direction is
        // at least m².
        assert!(
            tz.partial_cmp(m_sq).0 != Some(core::cmp::Ordering::Less),
            "hypot({m}², {m}): TowardZero fell below the anchor m²"
        );
    }
}

// ---------------------------------------------------------------------------
// The anchor band.

/// The gate straddle. With `δ₀ = 18`, a second operand at adjusted
/// exponent `≤ adj(w) − 19` is inside the anchor band and one at
/// `adj(w) − 18` is outside it, in the kernel band. Both treatments
/// must deliver the same answer over the overlap — that agreement is
/// the whole reason the band is allowed to short-circuit the ladder.
#[test]
fn the_two_bands_agree_across_the_gate() {
    for w in ["1", "3", "9.999999999999999999999999999999999", "7E+3000"] {
        let w_d = parse(w);
        for rm in ALL {
            // Just outside the band (kernel side) and just inside it.
            let (outside, st_o) = w_d.hypot(parse(&format!("1E{}", -DELTA0)), rm);
            let (inside, st_i) = w_d.hypot(parse(&format!("1E{}", -DELTA0 - 1)), rm);
            // Both hug `|w|`, so both round to the same neighbour of
            // it at this precision.
            assert!(
                identical(outside, inside),
                "hypot({w}, 1E-{DELTA0}) vs 1E-{}: @{rm:?} the bands disagree \
                 ({outside:e} vs {inside:e})",
                DELTA0 + 1
            );
            assert_eq!(st_o, st_i, "hypot({w}, …) @{rm:?}: band flags disagree");
            assert_eq!(st_o, Status::INEXACT, "hypot({w}, …) @{rm:?}: flags");
        }
    }
}

/// Deep ratios: the band's reason for existing. The true value's
/// offset above `|w|` (`~10^-1200` relative at the first row) is
/// below every fixed rung of the ladder, so only the ADR-0051 side
/// theorem decides it. `TowardPositive` must still step up, and the
/// other four directions must stay on `|w|`.
#[test]
fn deep_ratios_ride_the_side_theorem() {
    for (w, z) in [
        ("1E+300", "1E-300"),
        ("1", "1E-6100"),
        ("1.5", "1E-3000"),
        (MAX, "1E-6000"),
        ("1E-6100", "1E-6176"),
    ] {
        let w_d = parse(w);
        let z_d = parse(z);
        for rm in [NE, NA, TZ, TN] {
            let (r, st) = w_d.hypot(z_d, rm);
            assert!(
                equal(r, w_d),
                "hypot({w}, {z}) @{rm:?}: must stay on |w|, got {r:e}"
            );
            assert!(st.inexact(), "hypot({w}, {z}) @{rm:?}: flags {st:?}");
        }
        let (r, st) = w_d.hypot(z_d, TP);
        assert!(
            !equal(r, w_d),
            "hypot({w}, {z}) @TowardPositive: must step above |w|, got {r:e}"
        );
        assert!(
            st.inexact(),
            "hypot({w}, {z}) @TowardPositive: flags {st:?}"
        );
        // Symmetry across the band, both orders.
        for rm in ALL {
            let (a, sa) = w_d.hypot(z_d, rm);
            let (b, sb) = z_d.hypot(w_d, rm);
            assert!(
                identical(a, b) && sa == sb,
                "hypot({w}, {z}) @{rm:?}: asymmetric"
            );
        }
    }
}

/// **The MAX edge**, exhaustively over the five directions. With
/// `|w| = MAX` the true value sits above the largest finite value but
/// strictly below `MAX + ½ ulp`, the §7.4 threshold that decides
/// overflow under a nearest mode. So `TowardPositive` — whose
/// unbounded-range rounding is `MAX + 1 ulp = 10^6145`, past the
/// format — must deliver `+∞` with `OVERFLOW | INEXACT`, and every
/// other direction must deliver `MAX` with `INEXACT` and **no**
/// `OVERFLOW`. One input, two different §7.4 dispositions; getting
/// this wrong in either direction is invisible to a value-only check.
#[test]
fn the_max_edge_splits_the_overflow_disposition() {
    let max = parse(MAX);
    for z in ["1E-6000", "1E-6176", "1", "1E+3000"] {
        let z_d = parse(z);
        for rm in [NE, NA, TZ, TN] {
            for (p, q) in [(max, z_d), (z_d, max)] {
                let (r, st) = p.hypot(q, rm);
                assert!(
                    identical(r, max),
                    "hypot(MAX, {z}) @{rm:?}: got {r:e}, want MAX"
                );
                assert!(
                    st.inexact() && !st.overflow(),
                    "hypot(MAX, {z}) @{rm:?}: want INEXACT without OVERFLOW, got {st:?}"
                );
            }
        }
        for (p, q) in [(max, z_d), (z_d, max)] {
            let (r, st) = p.hypot(q, TP);
            assert!(
                r.is_infinite() && !r.is_sign_negative(),
                "hypot(MAX, {z}) @TowardPositive: got {r:e}, want +∞"
            );
            assert!(
                st.overflow() && st.inexact(),
                "hypot(MAX, {z}) @TowardPositive: want OVERFLOW | INEXACT, got {st:?}"
            );
        }
    }
}

/// The subnormal tail: an exact Pythagorean pair at the smallest
/// quantum stays exact (§7.5 forbids `UNDERFLOW` on an exact
/// subnormal result), and an inexact one underflows.
#[test]
fn the_subnormal_tail() {
    for rm in ALL {
        let (r, st) = parse("3E-6176").hypot(parse("4E-6176"), rm);
        assert!(identical(r, parse("5E-6176")), "@{rm:?}: got {r:e}");
        assert_eq!(st, Status::OK, "exact subnormal @{rm:?}: flags {st:?}");

        let (_, st) = parse("1E-6176").hypot(parse("1E-6176"), rm);
        assert!(
            st.inexact() && st.underflow(),
            "inexact subnormal @{rm:?}: flags {st:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Symmetry, on a deterministic sweep.

/// `hypot(x, y) ≡ hypot(y, x)`, bit-for-bit and flag-for-flag, over a
/// deterministic sweep that crosses both bands, every rounding
/// direction, both signs, and a wide spread of cohorts. Symmetry is a
/// property of the operand *ordering* logic — the kernel canonicalises
/// on magnitude with a quantum tie-break so equal-magnitude operands
/// at different cohorts still pick the same one — so a sweep is the
/// right shape of witness here, not a handful of rows.
#[test]
fn symmetry_differential_sweep() {
    let coefs: [u128; 8] = [
        1,
        3,
        7,
        99,
        123_456_789,
        5_000_000_000_000_000_000,
        9_999_999_999_999_999_999_999_999_999_999_999,
        1_000_000_000_000_000_000_000_000_000_000_000,
    ];
    let exps: [i32; 7] = [-6176, -300, -34, 0, 17, 300, 6100];
    let mut pairs = 0usize;
    for (i, &cx) in coefs.iter().enumerate() {
        for (j, &cy) in coefs.iter().enumerate() {
            // Walk the exponent table with an offset per coefficient
            // pair so the sweep crosses the anchor gate as well as
            // sitting inside the kernel band.
            let ex = exps[(i + j) % exps.len()];
            let ey = exps[(i + 2 * j + 3) % exps.len()];
            let Ok((x, _)) = Decimal128::parse_str(&format!("{cx}E{ex}"), NE) else {
                continue;
            };
            let Ok((y, _)) = Decimal128::parse_str(&format!("{cy}E{ey}"), NE) else {
                continue;
            };
            for sx in [false, true] {
                for sy in [false, true] {
                    let a = if sx { x.neg() } else { x };
                    let b = if sy { y.neg() } else { y };
                    for rm in ALL {
                        let (r1, s1) = a.hypot(b, rm);
                        let (r2, s2) = b.hypot(a, rm);
                        assert!(
                            identical(r1, r2),
                            "hypot({a:e}, {b:e}) @{rm:?}: {r1:e} vs swapped {r2:e}"
                        );
                        assert!(
                            s1 == s2,
                            "hypot({a:e}, {b:e}) @{rm:?}: flags {s1:?} vs {s2:?}"
                        );
                        pairs += 1;
                    }
                }
            }
        }
    }
    assert!(pairs >= 1_000, "sweep collapsed to {pairs} comparisons");
}

/// Equal magnitudes at *different* cohorts are the sharp case for the
/// ordering tie-break: `hypot(1, 1.0)` and `hypot(1.0, 1)` must pick
/// the same operand as the anchor, or the two calls diverge in the
/// last digit.
#[test]
fn equal_magnitudes_at_different_cohorts_stay_symmetric() {
    for (a, b) in [
        ("1", "1.0"),
        ("1", "1.000000000000000000000000000000000"),
        ("3E+10", "30000000000"),
        ("0.5", "0.50"),
        (MAX, "9999999999999999999999999999999999E+6111"),
    ] {
        for rm in ALL {
            let (r1, s1) = parse(a).hypot(parse(b), rm);
            let (r2, s2) = parse(b).hypot(parse(a), rm);
            assert!(
                identical(r1, r2) && s1 == s2,
                "hypot({a}, {b}) @{rm:?}: {r1:e}/{s1:?} vs {r2:e}/{s2:?}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// The independent exact oracle.

/// The exact correctly-rounded `hypot` for a finite operand pair,
/// computed without touching the kernel under audit: align the two
/// magnitudes on the common quantum `q = min(Qx, Qy)`, form the exact
/// integer `S = A² + B²` in `u128`, and hand `S · 10^(2q)` to the
/// big-integer square-root oracle. That oracle's GDA ideal exponent
/// for square root is `floor(2q/2) = q`, which is precisely §9.2.2's
/// preferred quantum for `hypot`, so the comparison gates the cohort
/// as well as the value.
///
/// The `u128` window (`A, B < 10^19`, so `S < 2·10^38 < u128::MAX`) is
/// what keeps the oracle arithmetic plain: no multiword code is shared
/// with the implementation being checked.
fn oracle_hypot(cx: u128, qx: i32, cy: u128, qy: i32, rm: RoundingMode) -> Rounded {
    let q = qx.min(qy);
    let a = cx * 10u128.pow((qx - q) as u32);
    let b = cy * 10u128.pow((qy - q) as u32);
    assert!(
        a < 10u128.pow(19) && b < 10u128.pow(19),
        "sweep operand escaped the u128 oracle window"
    );
    let s = a * a + b * b;
    let dec = parse_decimal(&format!("{s}E{}", 2 * q)).expect("exact sum parses");
    oracle::sqrt(&dec, Format::DECIMAL128, rm)
}

fn matches_oracle(got: Decimal128, want: &Expect) -> bool {
    match want {
        // The sweep feeds only finite nonzero operands, so the oracle
        // never predicts a NaN; treating one as a failure keeps that
        // premise honest instead of silently passing.
        Expect::Nan => false,
        Expect::Infinity { neg } => got.is_infinite() && got.is_sign_negative() == *neg,
        Expect::Finite { neg, coeff, exp } => {
            got.is_finite() && {
                let (n, c, e) = oracle::decode_decimal128(got.to_bits());
                n == *neg && c == *coeff && e == *exp
            }
        }
    }
}

/// The kernel band, checked bit-for-bit and flag-for-flag against the
/// exact oracle across a deterministic sweep. This is the file's
/// correctness gate proper: everything above pins structure, and this
/// pins the rounding itself against an independent computation (exact
/// big-integer square root, with a freshly transcribed §4.3.3
/// decision table).
#[test]
fn kernel_band_matches_the_exact_oracle() {
    // Coefficients stay below `10^16` so the widest alignment shift in
    // the sweep (`dq = 3`) keeps the aligned integers under `10^19`
    // and `S = A² + B² < 2·10^38` inside `u128`.
    let coefs: [u128; 10] = [
        1,
        2,
        3,
        7,
        99,
        1_000,
        123_457,
        999_999_999,
        123_456_789_012_345,
        9_999_999_999_999_999,
    ];
    let exps: [i32; 5] = [-40, -7, 0, 11, 200];
    let mut checked = 0usize;
    for &cx in &coefs {
        for &cy in &coefs {
            for &ex in &exps {
                for dq in [0i32, 1, 3] {
                    let ey = ex + dq;
                    for rm in ALL {
                        let x = parse(&format!("{cx}E{ex}"));
                        let y = parse(&format!("{cy}E{ey}"));
                        let (got, gs) = x.hypot(y, rm);
                        let want = oracle_hypot(cx, ex, cy, ey, rm);
                        assert!(
                            matches_oracle(got, &want.value),
                            "hypot({cx}E{ex}, {cy}E{ey}) @{rm:?}: got {got:e}, \
                             oracle {}",
                            want.decimal_string()
                        );
                        assert!(
                            status_conformance_eq(gs, want.status),
                            "hypot({cx}E{ex}, {cy}E{ey}) @{rm:?}: flags {gs:?}, \
                             oracle {:?}",
                            want.status
                        );
                        checked += 1;
                    }
                }
            }
        }
    }
    assert_eq!(checked, 7_500, "oracle sweep row count drifted");
}
