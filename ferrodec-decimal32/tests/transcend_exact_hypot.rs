//! Exact-result, special-value, band-structure, symmetry, and flag
//! gate for `Decimal32`'s `hypot` (IEEE 754-2019 §9.2; ADR-0060
//! Track D D3). The `Decimal128` sibling
//! (`tests/transcend_exact_hypot.rs` in the parent crate) carries the
//! full rationale; this file is the same gate at `P = 7`, where the
//! anchor band constant is `δ₀ = ⌈(P + 2)/2⌉ = 5` and the planted
//! nearest-mode tie is an 8-digit hypotenuse ending in 5.
//!
//! Re-deriving the format-specific constants rather than scaling the
//! `Decimal128` ones is deliberate: ADR-0060's named failure mode is
//! a constant bookkeeping error, and a per-format derivation is what
//! catches one.

#![cfg(feature = "exp-log")]

use ferrodec_decimal32::{Decimal32, RoundingMode, Status};
use ferrodec_test_support::conformance::status_conformance_eq;
use ferrodec_test_support::oracle::{self, parse_decimal, Expect, Format, Rounded};

const NE: RoundingMode = RoundingMode::NearestEven;
const NA: RoundingMode = RoundingMode::NearestAway;
const TZ: RoundingMode = RoundingMode::TowardZero;
const TP: RoundingMode = RoundingMode::TowardPositive;
const TN: RoundingMode = RoundingMode::TowardNegative;
const ALL: [RoundingMode; 5] = [NE, NA, TZ, TP, TN];

const PRECISION: u32 = 7;
const DELTA0: i32 = 5;

/// The largest finite `Decimal32`.
const MAX: &str = "9.999999E+96";

fn parse(s: &str) -> Decimal32 {
    Decimal32::parse_str(s, NE)
        .unwrap_or_else(|_| panic!("literal parses: {s:?}"))
        .0
}

fn equal(a: Decimal32, b: Decimal32) -> bool {
    a.partial_cmp(b).0 == Some(core::cmp::Ordering::Equal)
}

fn identical(a: Decimal32, b: Decimal32) -> bool {
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

#[test]
fn infinity_beats_quiet_nan_both_orders() {
    for inf in [Decimal32::INFINITY, Decimal32::NEG_INFINITY] {
        for rm in ALL {
            for (p, q) in [(inf, Decimal32::NAN), (Decimal32::NAN, inf)] {
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

#[test]
fn any_infinity_gives_positive_infinity() {
    let others = [
        Decimal32::INFINITY,
        Decimal32::NEG_INFINITY,
        Decimal32::ZERO,
        Decimal32::NEG_ZERO,
        parse("1"),
        parse("-1"),
        parse(MAX),
        parse("1E-101"),
    ];
    for inf in [Decimal32::INFINITY, Decimal32::NEG_INFINITY] {
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

/// §9.2.1's infinity exception is written for a *quiet* NaN operand;
/// §6.2 / §7.2 give a signaling NaN precedence over it. `atan2` and
/// `pow` resolve the same collision in the same order here.
#[test]
fn signaling_nan_outranks_the_infinity_rule() {
    let s = Decimal32::SIGNALING_NAN;
    let others = [
        Decimal32::INFINITY,
        Decimal32::NEG_INFINITY,
        Decimal32::NAN,
        Decimal32::SIGNALING_NAN,
        Decimal32::ZERO,
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

#[test]
fn quiet_nan_with_finite_propagates() {
    for other in [
        Decimal32::ZERO,
        Decimal32::NEG_ZERO,
        parse("3"),
        parse("-3"),
        parse(MAX),
    ] {
        for rm in ALL {
            for (p, q) in [(Decimal32::NAN, other), (other, Decimal32::NAN)] {
                let (r, st) = p.hypot(q, rm);
                assert!(r.is_nan(), "hypot(qNaN, {other}) @{rm:?}: got {r}");
                assert_eq!(st, Status::OK, "hypot(qNaN, {other}) @{rm:?}: flags");
            }
        }
    }
}

#[test]
fn zero_operand_delivers_the_exact_magnitude() {
    for (x, z, want) in [
        ("3", "0", "3"),
        ("-3", "0", "3"),
        ("3", "-0", "3"),
        ("-3", "-0", "3"),
        ("3.00", "0", "3.00"),
        ("3", "0.000", "3.000"),
        ("-1.25E+50", "0E+50", "1.25E+50"),
        ("1E-101", "0", "1E-101"),
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
// The exact family.

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
        ("3E+50", "4E+50", "5E+50"),
        ("3E-101", "4E-101", "5E-101"),
        ("30", "0.4E+2", "50"),
        ("3", "0.004E+3", "5"),
        // The widest exact family still inside the format: 6-digit
        // legs, 7-digit hypotenuse.
        ("600000", "800000", "1000000"),
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
/// allows and the result stays exact.
#[test]
fn an_unreachable_preferred_quantum_clamps_without_losing_exactness() {
    for (a, b, want) in [
        ("1E+96", "0E-101", "1.000000E+96"),
        (MAX, "0E-101", MAX),
        ("3E+10", "0E-101", "3.000000E+10"),
        ("1E-101", "0E+90", "1E-101"),
        ("0E+90", "0E-101", "0E-101"),
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

/// The planted nearest-mode tie at `P = 7`: `k = 2000001` (7 digits,
/// odd), legs `3k` and `4k` (7 digits each), hypotenuse
/// `5k = 10000005` — 8 digits ending in 5, i.e. exactly the midpoint
/// between two adjacent `Decimal32` values. Only the exact classifier
/// can resolve a value that *is* a rounding boundary.
#[test]
fn the_planted_tie_resolves_by_the_rounders_own_rule() {
    let a = "6000003";
    let b = "8000004";
    let down = "1.000000E+7";
    let up = "1.000001E+7";
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

/// `hypot(6E+96, 8E+96)` is exactly `1E+97`, one decade past `MAX`:
/// the classifier hands the true value to the rounder and the §7.4
/// disposition answers every direction.
#[test]
fn an_exact_value_past_the_range_takes_the_overflow_disposition() {
    for (rm, want_inf) in [(NE, true), (NA, true), (TZ, false), (TP, true), (TN, false)] {
        for (p, q) in [("6E+96", "8E+96"), ("8E+96", "6E+96")] {
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
// The ADR-0060 near-sharp families (the adjudicator's planted corpus).

/// `S = k² + 1`: `hypot(k, 1)` hugs the grid point `k` from above.
#[test]
fn near_sharp_k_squared_plus_one() {
    for k in ["3", "10", "99", "1000", "9999999", "3E+50", "7E-50"] {
        let one = parse("1");
        let mut sides = [Decimal32::ZERO; 5];
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

/// `S = k² + k` at `k = m²`, i.e. `hypot(m², m)`.
#[test]
fn near_sharp_k_squared_plus_k() {
    for m in ["3", "10", "999", "1E+3", "1E+40"] {
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
        assert!(
            tz.partial_cmp(m_sq).0 != Some(core::cmp::Ordering::Less),
            "hypot({m}², {m}): TowardZero fell below the anchor m²"
        );
    }
}

// ---------------------------------------------------------------------------
// The anchor band.

#[test]
fn the_two_bands_agree_across_the_gate() {
    for w in ["1", "3", "9.999999", "7E+50"] {
        let w_d = parse(w);
        for rm in ALL {
            let (outside, st_o) = w_d.hypot(parse(&format!("1E{}", -DELTA0)), rm);
            let (inside, st_i) = w_d.hypot(parse(&format!("1E{}", -DELTA0 - 1)), rm);
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

#[test]
fn deep_ratios_ride_the_side_theorem() {
    for (w, z) in [
        ("1E+50", "1E-50"),
        ("1", "1E-95"),
        ("1.5", "1E-40"),
        (MAX, "1E-50"),
        ("1E-95", "1E-101"),
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

/// The MAX edge: `TowardPositive` overflows to `+∞`, every other
/// direction stays on `MAX` with `INEXACT` and no `OVERFLOW`.
#[test]
fn the_max_edge_splits_the_overflow_disposition() {
    let max = parse(MAX);
    for z in ["1E-50", "1E-101", "1", "1E+40"] {
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

#[test]
fn the_subnormal_tail() {
    for rm in ALL {
        let (r, st) = parse("3E-101").hypot(parse("4E-101"), rm);
        assert!(identical(r, parse("5E-101")), "@{rm:?}: got {r:e}");
        assert_eq!(st, Status::OK, "exact subnormal @{rm:?}: flags {st:?}");

        let (_, st) = parse("1E-101").hypot(parse("1E-101"), rm);
        assert!(
            st.inexact() && st.underflow(),
            "inexact subnormal @{rm:?}: flags {st:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Symmetry.

#[test]
fn symmetry_differential_sweep() {
    let coefs: [u32; 8] = [1, 3, 7, 99, 123_457, 5_000_000, 9_999_999, 1_000_000];
    let exps: [i32; 7] = [-101, -50, -7, 0, 5, 30, 85];
    let mut pairs = 0usize;
    for (i, &cx) in coefs.iter().enumerate() {
        for (j, &cy) in coefs.iter().enumerate() {
            let ex = exps[(i + j) % exps.len()];
            let ey = exps[(i + 2 * j + 3) % exps.len()];
            let Ok((x, _)) = Decimal32::parse_str(&format!("{cx}E{ex}"), NE) else {
                continue;
            };
            let Ok((y, _)) = Decimal32::parse_str(&format!("{cy}E{ey}"), NE) else {
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

#[test]
fn equal_magnitudes_at_different_cohorts_stay_symmetric() {
    for (a, b) in [
        ("1", "1.0"),
        ("1", "1.000000"),
        ("3E+3", "3000"),
        ("0.5", "0.50"),
        (MAX, "9999999E+90"),
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
    oracle::sqrt(&dec, Format::DECIMAL32, rm)
}

fn matches_oracle(got: Decimal32, want: &Expect) -> bool {
    match want {
        Expect::Nan => false,
        Expect::Infinity { neg } => got.is_infinite() && got.is_sign_negative() == *neg,
        Expect::Finite { neg, coeff, exp } => {
            got.is_finite() && {
                let (n, c, e) = oracle::decode_decimal32(got.to_bits());
                n == *neg && c == *coeff && e == *exp
            }
        }
    }
}

/// The kernel band checked bit-for-bit and flag-for-flag against the
/// exact big-integer square-root oracle, whose GDA ideal exponent
/// `floor(2q/2) = q` is precisely §9.2.2's preferred quantum for
/// `hypot`, so the cohort is gated too.
#[test]
fn kernel_band_matches_the_exact_oracle() {
    let coefs: [u128; 10] = [1, 2, 3, 7, 99, 1_000, 12_347, 123_457, 999_999, 9_999_999];
    let exps: [i32; 5] = [-40, -7, 0, 11, 50];
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
