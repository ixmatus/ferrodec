//! Special-value, exact-result, tie, anchor-arm, and flag gate for
//! `Decimal32`'s `rootn` (IEEE 754-2019 §9.2; ADR-0059 Track D group
//! D3, fd-4zo.25). The sibling mirror of the root crate's
//! `tests/transcend_exact_rootn.rs`; the two differ only in the
//! format's precision and exponent range, and in the tie family's and
//! straddling pair's per format constants.
//!
//! Four things are pinned here, and the first is the reason the file
//! exists at all:
//!
//! 1. **§9.2.1 verbatim.** Every row of the standard's `rootn` table,
//!    in every rounding direction, plus the NOTE beside it — that
//!    `rootn(−0, 2)` is `+0` while `squareRoot(−0)` is `−0`, two
//!    spellings of the same words with two mandated answers. The
//!    contrast is asserted against this crate's own `sqrt`, so a
//!    future edit to either one breaks the pair rather than silently
//!    aligning them.
//! 2. **The delegations.** `n = ±1` and `n = 2` route to §5 basic
//!    operations; the tests are differentials against those very
//!    operations, so the delegation cannot rot into a re-derivation.
//!    `rootn(x, 3) ≡ cbrt(x)` is the same discipline applied to the
//!    general path: a few hundred deterministic inputs, compared bit
//!    for bit including flags.
//! 3. **The classifier.** Exact roots at every direction with no
//!    `INEXACT` (§7.5), their directed-mode neighbours *with*
//!    `INEXACT` and on the correct sides, and the negative-order tie
//!    family `rootn(2^2k, −2) = 5^k · 10^−k` whose `PRECISION + 1`
//!    coefficient is a nearest-mode midpoint no approximation kernel
//!    can resolve.
//! 4. **The hug-at-1 arm.** For a large `|n|` the true value sits
//!    between 1 and the first rounding boundary beside it; the arm
//!    decides the side from a theorem instead of from a rung's
//!    leftover resolution. The threshold-straddling pair shows the
//!    seam is continuous: the input just inside the gate and the one
//!    just outside round identically in every direction.

#![cfg(feature = "exp-log")]

use ferrodec_decimal32::{Decimal32, RoundingMode, Status};

const NE: RoundingMode = RoundingMode::NearestEven;
const NA: RoundingMode = RoundingMode::NearestAway;
const TZ: RoundingMode = RoundingMode::TowardZero;
const TP: RoundingMode = RoundingMode::TowardPositive;
const TN: RoundingMode = RoundingMode::TowardNegative;
const ALL: [RoundingMode; 5] = [NE, NA, TZ, TP, TN];

/// The value just above 1: `1 + 10^−(PRECISION−1)`.
const ONE_UP: &str = "1.000001";
/// The value just below 1: `1 − 10^−PRECISION`.
const ONE_DOWN: &str = "0.9999999";
/// `2^22`, whose `−2`-th root `2^−11 = 5^11 · 10^−11` carries an
/// 8-digit coefficient ending in 5: a `Decimal32` nearest-mode
/// midpoint (the tie family's only member at this format).
const TIE_INPUT: &str = "4194304";
/// The two neighbours the midpoint sits between.
const TIE_DOWN: &str = "4.882812E-4";
const TIE_UP: &str = "4.882813E-4";
/// At seven digits the hug-at-1 gate at `n = i32::MAX` admits a
/// bound of 10.7 (`5·10^−9 · (2^31 − 1)`), which every operand in
/// the two decades adjacent to 1 clears — so this format takes its
/// straddling pair in the third regime of the bound instead:
/// `(|adj| + 1)·2.303` puts `1000` (adj 3, bound 9.21) inside the
/// gate and `10000` (adj 4, bound 11.5) outside it.
const STRADDLE_IN: &str = "1000";
const STRADDLE_OUT: &str = "10000";
/// Exponent span for the deterministic `cbrt` differential sweep.
#[cfg(feature = "pow")]
const SWEEP_EXP: i32 = 60;

fn parse(s: &str) -> Decimal32 {
    Decimal32::parse_str(s, NE)
        .unwrap_or_else(|_| panic!("literal parses: {s:?}"))
        .0
}

/// Value equality, cohort insensitive (IEEE `compare`).
fn equal(a: Decimal32, b: Decimal32) -> bool {
    a.partial_cmp(b).0 == Some(core::cmp::Ordering::Equal)
}

// ---------------------------------------------------------------------------
// 1. IEEE 754-2019 §9.2.1, row by row.

/// "rootn(±0, n) is ±∞ and signals the divideByZero exception for odd
/// n < 0" and "rootn(±0, n) is +∞ ... for even n < 0".
#[test]
fn zero_negative_order_is_infinity_with_div_by_zero() {
    for rm in ALL {
        for (zero, neg) in [(Decimal32::ZERO, false), (Decimal32::NEG_ZERO, true)] {
            for n in [-1, -3, -7, -1001, i32::MAX.wrapping_neg()] {
                let (r, st) = zero.rootn(n, rm);
                assert!(r.is_infinite(), "rootn(±0, {n}) at {rm:?}: want ∞, got {r}");
                assert_eq!(
                    r.is_sign_negative(),
                    neg,
                    "rootn(±0, {n}) at {rm:?}: odd n < 0 keeps the zero's sign"
                );
                assert_eq!(st, Status::DIV_BY_ZERO, "rootn(±0, {n}) at {rm:?}: flags");
            }
            for n in [-2, -4, -100, i32::MIN] {
                let (r, st) = zero.rootn(n, rm);
                assert!(r.is_infinite(), "rootn(±0, {n}) at {rm:?}: want ∞");
                assert!(
                    !r.is_sign_negative(),
                    "rootn(±0, {n}) at {rm:?}: even n < 0 delivers +∞"
                );
                assert_eq!(st, Status::DIV_BY_ZERO, "rootn(±0, {n}) at {rm:?}: flags");
            }
        }
    }
}

/// "rootn(±0, n) is +0 for even n > 0" and "rootn(±0, n) is ±0 for
/// odd n > 0".
#[test]
fn zero_positive_order_is_zero() {
    for rm in ALL {
        for (zero, neg) in [(Decimal32::ZERO, false), (Decimal32::NEG_ZERO, true)] {
            for n in [1, 3, 7, 1001, i32::MAX] {
                let (r, st) = zero.rootn(n, rm);
                assert!(r.is_zero(), "rootn(±0, {n}) at {rm:?}: want zero");
                assert_eq!(
                    r.is_sign_negative(),
                    neg,
                    "rootn(±0, {n}) at {rm:?}: odd n > 0 keeps the sign"
                );
                assert_eq!(st, Status::OK, "rootn(±0, {n}) at {rm:?}: flags");
            }
            for n in [2, 4, 100, i32::MAX - 1] {
                let (r, st) = zero.rootn(n, rm);
                assert!(r.is_zero(), "rootn(±0, {n}) at {rm:?}: want zero");
                assert!(
                    !r.is_sign_negative(),
                    "rootn(±0, {n}) at {rm:?}: even n > 0 delivers +0"
                );
                assert_eq!(st, Status::OK, "rootn(±0, {n}) at {rm:?}: flags");
            }
        }
    }
}

/// The standard's NOTE: `rootn(−0, 2)` is `+0`, while
/// `squareRoot(−0)` is `−0` (§5.4.1 preserves a zero's sign). Two
/// spellings of "the square root of minus zero", two mandated
/// answers — asserted against this crate's own `sqrt` so neither can
/// drift onto the other.
#[test]
fn rootn_neg_zero_two_differs_from_sqrt_neg_zero() {
    for rm in ALL {
        let (r, st) = Decimal32::NEG_ZERO.rootn(2, rm);
        assert!(r.is_zero() && !r.is_sign_negative(), "rootn(−0, 2) is +0");
        assert_eq!(st, Status::OK);

        let (s, sst) = Decimal32::NEG_ZERO.sqrt(rm);
        assert!(s.is_zero() && s.is_sign_negative(), "sqrt(−0) is −0");
        assert_eq!(sst, Status::OK);

        assert_ne!(
            r.is_sign_negative(),
            s.is_sign_negative(),
            "the NOTE: rootn(−0, 2) and squareRoot(−0) differ in sign at {rm:?}"
        );
        // The odd order is the one that agrees with the sign rule.
        let (odd, _) = Decimal32::NEG_ZERO.rootn(3, rm);
        assert!(
            odd.is_zero() && odd.is_sign_negative(),
            "rootn(−0, 3) is −0"
        );
    }
}

/// "rootn(+∞, n) is +∞ for n > 0"; "rootn(−∞, n) is −∞ for odd
/// n > 0"; "rootn(+∞, n) is +0 for n < 0"; "rootn(−∞, n) is −0 for
/// odd n < 0".
#[test]
fn infinity_rows() {
    for rm in ALL {
        for n in [1, 2, 3, 100, i32::MAX] {
            let (r, st) = Decimal32::INFINITY.rootn(n, rm);
            assert!(r.is_infinite() && !r.is_sign_negative(), "rootn(+∞, {n})");
            assert_eq!(st, Status::OK);
        }
        for n in [-1, -2, -3, -100, i32::MIN] {
            let (r, st) = Decimal32::INFINITY.rootn(n, rm);
            assert!(r.is_zero() && !r.is_sign_negative(), "rootn(+∞, {n}) is +0");
            assert_eq!(st, Status::OK);
        }
        for n in [1, 3, 7, i32::MAX] {
            let (r, st) = Decimal32::NEG_INFINITY.rootn(n, rm);
            assert!(r.is_infinite() && r.is_sign_negative(), "rootn(−∞, {n})");
            assert_eq!(st, Status::OK);
        }
        for n in [-1, -3, -7, -1001] {
            let (r, st) = Decimal32::NEG_INFINITY.rootn(n, rm);
            assert!(r.is_zero() && r.is_sign_negative(), "rootn(−∞, {n}) is −0");
            assert_eq!(st, Status::OK);
        }
    }
}

/// "rootn(−x, n) is qNaN and signals the invalid operation exception
/// for even n" — both signs of `n`, and `−∞` as well as a negative
/// finite operand.
#[test]
fn negative_operand_even_order_is_invalid() {
    for rm in ALL {
        for x in [
            parse("-4"),
            parse("-1"),
            parse("-1E-101"),
            parse("-9.999999E+96"),
            Decimal32::NEG_INFINITY,
        ] {
            for n in [2, 4, 100, i32::MAX - 1, -2, -4, -100, i32::MIN] {
                let (r, st) = x.rootn(n, rm);
                assert!(r.is_nan(), "rootn({x}, {n}) at {rm:?}: want qNaN, got {r}");
                assert!(!r.is_signaling_nan(), "the NaN must be quiet");
                assert_eq!(st, Status::INVALID, "rootn({x}, {n}) at {rm:?}: flags");
            }
        }
    }
}

/// NaN propagation per the crate convention: a quiet NaN passes
/// through with clean flags, a signaling NaN is quieted and raises
/// `INVALID`. `rootn` has no `pow(x, ±0) = 1`-style row that consumes
/// a NaN, so this holds for every `n`, `n = 0` included.
#[test]
fn nan_propagates() {
    for rm in ALL {
        for n in [0, 1, 2, -1, -2, 3, i32::MAX, i32::MIN] {
            let (r, st) = Decimal32::NAN.rootn(n, rm);
            assert!(r.is_nan() && !r.is_signaling_nan());
            assert_eq!(st, Status::OK, "qNaN propagates with no flag (n = {n})");

            let (r, st) = Decimal32::SIGNALING_NAN.rootn(n, rm);
            assert!(r.is_nan() && !r.is_signaling_nan(), "sNaN is quieted");
            assert_eq!(st, Status::INVALID, "sNaN raises INVALID (n = {n})");
        }
    }
}

/// `n = 0` is absent from the standard's table, which leaves the case
/// to the implementation. This kernel delivers a quiet NaN with
/// `INVALID`, matching MPFR's `rootn`.
#[test]
fn zero_order_is_invalid() {
    for rm in ALL {
        for x in [
            parse("8"),
            parse("-8"),
            parse("1"),
            Decimal32::ZERO,
            Decimal32::NEG_ZERO,
            Decimal32::INFINITY,
            Decimal32::NEG_INFINITY,
        ] {
            let (r, st) = x.rootn(0, rm);
            assert!(r.is_nan(), "rootn({x}, 0) at {rm:?}: want qNaN, got {r}");
            assert_eq!(st, Status::INVALID, "rootn({x}, 0) at {rm:?}: flags");
        }
    }
}

// ---------------------------------------------------------------------------
// 2. The delegations, as differentials against the operations they
//    delegate to.

/// `rootn(x, 1)` is `x` itself, cohort included (§9.2.2's preferred
/// exponent `floor(Q(x)/1) = Q(x)`), with no flag raised.
#[test]
fn order_one_is_the_identity() {
    for s in [
        "7.5",
        "-7.5",
        "1E+30",
        "1.000E+27",
        "0.001",
        "1E-101",
        "9.999999E+96",
    ] {
        let x = parse(s);
        for rm in ALL {
            let (r, st) = x.rootn(1, rm);
            assert_eq!(r.to_bits(), x.to_bits(), "rootn({s}, 1) at {rm:?} is x");
            assert_eq!(st, Status::OK, "rootn({s}, 1) at {rm:?}: no flag");
        }
    }
}

/// `rootn(x, −1)` is `1 ÷ x`, bit for bit and flag for flag: the
/// delegation to the §5.4.1 division, not a re-derivation of it.
#[test]
fn order_minus_one_is_the_reciprocal() {
    for s in [
        "7.5",
        "-7.5",
        "2",
        "1E+30",
        "0.001",
        "3",
        "1E-95",
        "9.999999E+96",
    ] {
        let x = parse(s);
        for rm in ALL {
            let (r, st) = x.rootn(-1, rm);
            let (want, want_st) = Decimal32::ONE.div(x, rm);
            assert_eq!(
                r.to_bits(),
                want.to_bits(),
                "rootn({s}, −1) at {rm:?} = {r}, 1/x = {want}"
            );
            assert_eq!(st, want_st, "rootn({s}, −1) at {rm:?}: flags");
        }
    }
}

/// `rootn(x, 2)` is `sqrt(x)`, bit for bit and flag for flag —
/// including the preferred exponent `floor(Q(x)/2)`, which §5.4.1 and
/// §9.2.2 agree on here, and the exact cases (perfect squares), which
/// the delegation carries without consulting the classifier.
#[test]
fn order_two_is_the_square_root() {
    for s in [
        "4",
        "2",
        "0.25",
        "1E+30",
        "1E+31",
        "1E-101",
        "9",
        "1024",
        "0",
        "6.25",
        "9.999999E+96",
    ] {
        let x = parse(s);
        for rm in ALL {
            let (r, st) = x.rootn(2, rm);
            let (want, want_st) = x.sqrt(rm);
            assert_eq!(
                r.to_bits(),
                want.to_bits(),
                "rootn({s}, 2) at {rm:?} = {r}, sqrt = {want}"
            );
            assert_eq!(st, want_st, "rootn({s}, 2) at {rm:?}: flags");
        }
    }
}

/// A deterministic xorshift sweep, its only job being reproducibility.
#[cfg(feature = "pow")]
fn sweep_inputs(count: usize) -> Vec<Decimal32> {
    let mut state: u64 = 0x243F_6A88_85A3_08D3;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let hi = next() % 1_000_000_000_000_000_000;
        let lo = next() % 1_000_000_000_000_000;
        let exp = (next() % (2 * SWEEP_EXP as u64 + 1)) as i32 - SWEEP_EXP;
        let sign = if i % 2 == 0 { "" } else { "-" };
        out.push(parse(&format!("{sign}{hi}{lo:015}E{exp}")));
    }
    out
}

/// `rootn(x, 3)` IS `cbrt(x)` — same value, same flags, same cohort —
/// across a few hundred deterministic inputs of both signs and the
/// whole exponent span. The two kernels compose the same `exp`/`ln`
/// pipeline through different entry points, so agreement is a real
/// differential rather than a tautology: `cbrt` divides by a literal
/// 3 and classifies with `cbrt_exact_input`, `rootn` divides by `|n|`
/// and classifies with `rootn_exact_input`.
// `cbrt` lives behind this format's `pow` feature (its module
// neighbour), so the differential is gated on it while the rest of
// the file needs only `exp-log`.
#[cfg(feature = "pow")]
#[test]
fn order_three_is_cbrt() {
    for rm in ALL {
        for x in sweep_inputs(400) {
            let (a, sa) = x.rootn(3, rm);
            let (b, sb) = x.cbrt(rm);
            assert_eq!(
                a.to_bits(),
                b.to_bits(),
                "rootn({x}, 3) = {a} but cbrt({x}) = {b} at {rm:?}"
            );
            assert_eq!(sa, sb, "rootn({x}, 3) flags differ from cbrt at {rm:?}");
        }
    }
}

// ---------------------------------------------------------------------------
// 3. The classifier: exact roots, their neighbours, and the ties.

/// Exact roots deliver the true value in every direction with clean
/// flags — §7.5 forbids `INEXACT` on an exact result. The negative
/// orders are here too (`s = 1` is the extra condition their bail
/// proof turns on).
#[test]
fn exact_roots_every_mode() {
    let cases: [(&str, i32, &str); 12] = [
        ("8", 3, "2"),
        ("-8", 3, "-2"),
        ("0.027", 3, "0.3"),
        ("-0.027", 3, "-0.3"),
        ("1E+30", 5, "1E+6"),
        ("32", 5, "2"),
        ("1024", 10, "2"),
        ("2.25", 2, "1.5"),
        ("1E-30", 5, "1E-6"),
        ("32", -5, "0.5"),
        ("1E+30", -5, "1E-6"),
        ("1", 7, "1"),
    ];
    for (s, n, want) in cases {
        let x = parse(s);
        let w = parse(want);
        for rm in ALL {
            let (r, st) = x.rootn(n, rm);
            assert!(equal(r, w), "rootn({s}, {n}) at {rm:?} = {r}, want {want}");
            assert_eq!(st, Status::OK, "rootn({s}, {n}) at {rm:?}: §7.5 flags");
        }
    }
}

/// `rootn(±1, n) = ±1` for every order the format can name, both
/// signs of `n`: the classifier answers `|x| = 1` outright, which the
/// hug-at-1 arm's strict side theorem depends on.
#[test]
fn one_is_exact_at_every_order() {
    for n in [2, 3, 5, 1_000_000, i32::MAX, -2, -3, -5, i32::MIN] {
        for rm in ALL {
            let (r, st) = Decimal32::ONE.rootn(n, rm);
            assert!(equal(r, Decimal32::ONE), "rootn(1, {n}) at {rm:?} = {r}");
            assert_eq!(st, Status::OK);
        }
        if n % 2 != 0 {
            for rm in ALL {
                let (r, st) = Decimal32::NEG_ONE.rootn(n, rm);
                assert!(equal(r, Decimal32::NEG_ONE), "rootn(−1, {n}) at {rm:?}");
                assert_eq!(st, Status::OK);
            }
        }
    }
}

/// The directed-mode neighbours of an exact case: perturbing the
/// input one quantum puts the true root strictly on one side of the
/// exact value, and every direction must follow that side. This is
/// the fd-aqs.5 hazard the input-side classifier exists to close —
/// before it, the kernel's own error decided the side and
/// `rootn(0.027, 3)` shipped `0.2999…9` at `TowardZero` — checked
/// here from both sides of the exact value rather than only at it.
///
/// The step is `d(x^(1/3)) = δ/(3x^(2/3)) = δ/0.27 ≈ 3.7δ`, and one
/// quantum of `0.027` is `10^−8` while the spacing at `0.3` is
/// `10^−7`: the perturbed root sits about `0.37` ULP from `0.3`, so
/// the nearest modes stay at `0.3` and only the direction facing the
/// residual moves.
#[test]
fn directed_neighbours_of_exact_roots() {
    let exact = parse("0.3");
    let (up, _) = exact.next_up();
    let (down, _) = exact.next_down();
    let (below_in, _) = parse("0.027").next_down();
    let (above_in, _) = parse("0.027").next_up();

    for (x, above_root) in [(below_in, false), (above_in, true)] {
        for rm in ALL {
            let (r, st) = x.rootn(3, rm);
            assert!(st.inexact(), "rootn({x}, 3) at {rm:?} must be INEXACT");
            let (cmp, _) = r.partial_cmp(exact);
            let ord = cmp.expect("finite comparison");
            if above_root {
                assert!(
                    ord != core::cmp::Ordering::Less,
                    "rootn({x}, 3) at {rm:?} = {r} must not fall below 0.3"
                );
            } else {
                assert!(
                    ord != core::cmp::Ordering::Greater,
                    "rootn({x}, 3) at {rm:?} = {r} must not rise above 0.3"
                );
            }
            let want = match (rm, above_root) {
                (TP, true) => up,
                (TZ | TN, false) => down,
                _ => exact,
            };
            assert!(equal(r, want), "rootn({x}, 3) at {rm:?} = {r}, want {want}");
        }
    }
}

/// The negative-order tie family: `rootn(2^2k, −2) = 5^k · 10^−k`,
/// whose `PRECISION + 1` digit coefficient ends in 5 and is therefore
/// a nearest-mode midpoint. The approximation kernel cannot resolve a
/// value that IS the boundary; the classifier hands the exact
/// coefficient to the format rounder, whose own tie rule then decides
/// — `NearestEven` to the even neighbour, `NearestAway` away from
/// zero, the directed modes to their own sides, all `INEXACT`.
///
/// Positive orders have no ties at all: the input carries the `b`-th
/// power of the result's coefficient, so a `PRECISION + 1` digit
/// result would need a `2·PRECISION + 1` digit input.
#[test]
fn negative_order_tie_resolves_by_the_mode() {
    let x = parse(TIE_INPUT);
    let down = parse(TIE_DOWN);
    let up = parse(TIE_UP);
    for (rm, want) in [(NE, down), (NA, up), (TZ, down), (TP, up), (TN, down)] {
        let (r, st) = x.rootn(-2, rm);
        assert!(
            equal(r, want),
            "rootn(2^22, −2) at {rm:?} = {r}, want {want}"
        );
        assert!(st.inexact(), "a tie is inexact");
    }
}

/// Quantum pins. The delegated arms deliver §9.2.2's preferred
/// exponent `floor(Q(x)/n)` exactly. The classified arm delivers the
/// §6.3 quantum the shared kernel rounder produces for every §9.2
/// operation in this crate (preferred quantum 0), which coincides
/// with §9.2.2 whenever `floor(Q(x)/n) ≤ 0` and diverges upward
/// otherwise — `rootn(1E+30, 5)` is `1000000`, not `1E+6`. Same
/// value, different cohort; pinned here so the divergence is a
/// recorded fact rather than a surprise.
#[test]
fn quantum_pins() {
    // n = 1: Q untouched.
    let x = parse("1.500E+3");
    let (r, _) = x.rootn(1, NE);
    assert!(r.same_quantum(x), "rootn(x, 1) keeps Q(x)");

    // n = 2: sqrt's own preferred exponent, which is §9.2.2's.
    for s in ["1E+30", "4.00", "9"] {
        let x = parse(s);
        let (r, _) = x.rootn(2, NE);
        let (want, _) = x.sqrt(NE);
        assert!(r.same_quantum(want), "rootn({s}, 2) matches sqrt's quantum");
    }

    // n = −1: Q(1) − Q(x) = −Q(x), which is floor(Q(x)/−1).
    let x = parse("1E+6");
    let (r, _) = x.rootn(-1, NE);
    let (want, _) = Decimal32::ONE.div(x, NE);
    assert!(r.same_quantum(want));

    // The classified arm, negative preferred quantum: §9.2.2's value
    // is reached, because moving toward quantum 0 would change the
    // value.
    let (r, _) = parse("1E-30").rootn(5, NE);
    assert!(
        r.same_quantum(parse("1E-6")),
        "rootn(1E−30, 5) lands at Q = −6 = floor(−30/5)"
    );

    // The classified arm, positive preferred quantum: the recorded
    // divergence.
    let (r, _) = parse("1E+30").rootn(5, NE);
    assert!(equal(r, parse("1E+6")), "the value is right");
    assert!(
        r.same_quantum(parse("1000000")),
        "delivered at Q = 0, where §9.2.2 asks for Q = 6"
    );
    assert!(!r.same_quantum(parse("1E+6")), "the divergence, pinned");
}

// ---------------------------------------------------------------------------
// 4. The hug-at-1 anchor arm.

/// The arm's four corners: the two representable neighbours of 1 as
/// inputs, each at `n = i32::MAX` and `n = i32::MIN`, in all five
/// directions. The true value is `1 + ln(x)/n`, which for these
/// operands sits ~10^−16 from 1 — nine decades inside the nearest
/// rounding boundary — so every direction's answer follows the side
/// theorem `rootn(x, n) > 1 iff (x > 1) XOR (n < 0)`, and the
/// directed mode facing the residual takes 1's neighbour on that
/// side.
#[test]
fn hug_at_one_corners() {
    let one = Decimal32::ONE;
    let (up, _) = one.next_up();
    let (down, _) = one.next_down();

    for (s, x_above) in [(ONE_UP, true), (ONE_DOWN, false)] {
        let x = parse(s);
        for n in [i32::MAX, i32::MIN] {
            let above = x_above != (n < 0);
            for rm in ALL {
                let (r, st) = x.rootn(n, rm);
                assert!(st.inexact(), "rootn({s}, {n}) at {rm:?} must be INEXACT");
                let want = match (rm, above) {
                    (TP, true) => up,
                    (TN | TZ, false) => down,
                    _ => one,
                };
                assert!(
                    equal(r, want),
                    "rootn({s}, {n}) at {rm:?} = {r}, want {want} (side above = {above})"
                );
            }
        }
    }
}

/// The seam is continuous: the input just inside the arm's gate and
/// the one just outside round identically in every direction. Inside,
/// the ADR-0051 residual channel decides from the side theorem;
/// outside, the general `exp(ln|x|/n)` pipeline decides from its own
/// working value. Both true values lie strictly between 1 and the
/// same rounding boundary, so agreement is the property that says the
/// gate's threshold is a seam and not a cliff.
#[test]
fn hug_at_one_seam_is_continuous() {
    let inside = parse(STRADDLE_IN);
    let outside = parse(STRADDLE_OUT);
    let one = Decimal32::ONE;
    let (up, _) = one.next_up();
    for rm in ALL {
        let (a, sa) = inside.rootn(i32::MAX, rm);
        let (b, sb) = outside.rootn(i32::MAX, rm);
        assert_eq!(
            a.to_bits(),
            b.to_bits(),
            "the seam at {rm:?}: inside = {a}, outside = {b}"
        );
        assert_eq!(sa, sb, "the seam at {rm:?}: flags");
        let want = if rm == TP { up } else { one };
        assert!(equal(a, want), "seam value at {rm:?} = {a}, want {want}");
        assert!(sa.inexact());
    }
}

/// Away from 1 the arm stays shut and the general path answers, with
/// a result genuinely off the grid point rather than snapped to it.
/// `rootn(2, 100) = 2^0.01 ≈ 1.0069555`: its bound `|x| − 1 = 1`
/// divided by 100 is `10^−2`, more than thirty orders outside the
/// `5·10^−9` gate, and the value is a whole quantum off 1 at every
/// format. The side theorem still names the side, and the negative
/// order flips it.
#[test]
fn away_from_one_the_arm_stays_shut() {
    let (r, st) = parse("2").rootn(100, NE);
    assert!(st.inexact());
    assert!(!equal(r, Decimal32::ONE), "the value is off the grid point");
    let (cmp, _) = r.partial_cmp(Decimal32::ONE);
    assert_eq!(cmp, Some(core::cmp::Ordering::Greater));
    // Its reciprocal order lands strictly below 1, by the same theorem.
    let (r, _) = parse("2").rootn(-100, NE);
    let (cmp, _) = r.partial_cmp(Decimal32::ONE);
    assert_eq!(cmp, Some(core::cmp::Ordering::Less));
}

/// The general path at `|n| ≥ 3` against an independent composition:
/// `rootn(x, n)` versus `rootn(rootn(x, a), b)` for `n = a·b`. The two
/// route through different divisors and different numbers of
/// roundings, so agreement to within a quantum is evidence the
/// divisor path is doing what it claims.
#[test]
fn composition_agrees_within_a_quantum() {
    for (s, a, b) in [("7", 2i32, 3i32), ("1234.5", 3, 5), ("0.5", 5, 7)] {
        let x = parse(s);
        let (direct, _) = x.rootn(a * b, NE);
        let (inner, _) = x.rootn(a, NE);
        let (composed, _) = inner.rootn(b, NE);
        let (lo, _) = direct.next_down();
        let (hi, _) = direct.next_up();
        let (c1, _) = composed.partial_cmp(lo);
        let (c2, _) = composed.partial_cmp(hi);
        assert!(
            c1 != Some(core::cmp::Ordering::Less) && c2 != Some(core::cmp::Ordering::Greater),
            "rootn({s}, {}) = {direct}, composed = {composed}",
            a * b
        );
    }
}
