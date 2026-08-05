//! Exact-result, range-disposition, special-value, and flag gate for
//! `Decimal32`'s `exp10` (IEEE 754-2019 §9.2; ADR-0059 Track D). The
//! sibling mirror of the root crate's `tests/transcend_exact_exp10.rs`;
//! the two differ only in the format's exponent range and in the one
//! integer the `exp` overflow gate leaves uncovered.
//!
//! `10^x` is rational only at integer `x` (unique factorization of
//! `10 = 2·5` forces the denominator of a representable `x = a/b` to
//! be 1), so the exact family is the integers, with value `10^n` and
//! coefficient 1, representable for every `n` from `etiny = −101` to
//! `emax = 96`. 198 inputs in five directions is trivially exhaustive.
//!
//! Past the range the classifier still decides, and that is the load
//! bearing half: `10^n` for `n > emax` sits exactly ON a grid point at
//! its own exponent, which no rung of the ADR-0059 ladder can move off,
//! and `Decimal32`'s overflow gate leaves exactly one such integer
//! uncovered (`n = 97`, since `97 · ln 10 ≈ 223.4` stays inside the 224
//! limit). The `ladder_audit` battery lane runs this file, so that
//! integer is the family's standing witness.

#![cfg(feature = "exp-log")]

use ferrodec_decimal32::{Decimal32, RoundingMode, Status};

const NE: RoundingMode = RoundingMode::NearestEven;
const NA: RoundingMode = RoundingMode::NearestAway;
const TZ: RoundingMode = RoundingMode::TowardZero;
const TP: RoundingMode = RoundingMode::TowardPositive;
const TN: RoundingMode = RoundingMode::TowardNegative;
const ALL: [RoundingMode; 5] = [NE, NA, TZ, TP, TN];

/// Largest decimal exponent of a representable `Decimal32`.
const EMAX: i32 = 96;
/// Smallest decimal exponent of a representable `Decimal32`: `10^-101`
/// is the smallest positive subnormal.
const ETINY: i32 = -101;
/// The integer the `exp` overflow gate does not catch: `97 · ln 10 ≈
/// 223.4`, inside the format's 224 limit, while `10^97` is past `MAX`.
const GATE_GAP: i32 = 97;

fn parse(s: &str) -> Decimal32 {
    Decimal32::parse_str(s, NE)
        .unwrap_or_else(|_| panic!("literal parses: {s:?}"))
        .0
}

fn int(n: i32) -> Decimal32 {
    Decimal32::try_new(n, 0).expect("small integer is representable")
}

/// Value equality, cohort insensitive (the IEEE `compare` the corpus
/// gate uses).
fn equal(a: Decimal32, b: Decimal32) -> bool {
    a.partial_cmp(b).0 == Some(core::cmp::Ordering::Equal)
}

// ---------------------------------------------------------------------------
// The exact family, exhaustively, in every rounding direction.

/// Every representable member: `10^n` for `n` from `etiny` to `emax`,
/// the whole exponent range including the subnormal tail. Exactly
/// `1E{n}` with `Status::OK` in all five directions. §7.5 forbids
/// `INEXACT` on an exact result, and `UNDERFLOW` on an exact subnormal
/// one, which the `n < −95` half of this sweep witnesses.
#[test]
fn exact_powers_of_ten_every_mode() {
    for n in ETINY..=EMAX {
        let x = int(n);
        let want = parse(&format!("1e{n}"));
        for rm in ALL {
            let (r, st) = x.exp10(rm);
            assert!(equal(r, want), "exp10({n}) at {rm:?}: got {r}, want 1e{n}");
            assert_eq!(st, Status::OK, "exp10({n}) at {rm:?}: flags");
        }
    }
}

/// The independent witness: `log10` takes the delivered `10^n` back to
/// `n` through a different kernel and a different classifier.
#[test]
fn log10_takes_the_family_back() {
    for n in [ETINY, -95, -50, -7, -1, 1, 2, 7, 50, EMAX] {
        let x = int(n);
        let (p, _) = x.exp10(NE);
        let (back, st) = p.log10(NE);
        assert!(equal(back, x), "log10(exp10({n})) = {back}, want {n}");
        assert_eq!(st, Status::OK, "log10(10^{n}) is exact");
    }
}

/// Cohort insensitivity: the classifier reads the stripped form, so an
/// integer stored at another quantum takes the exact path too.
#[test]
fn cohort_variants_of_an_integer_are_still_exact() {
    for (literal, n) in [
        ("2", 2),
        ("2.000", 2),
        ("2E+0", 2),
        ("20e-1", 2),
        ("1e1", 10),
        ("100e-1", 10),
        ("-3.000", -3),
    ] {
        let x = parse(literal);
        let want = parse(&format!("1e{n}"));
        for rm in ALL {
            let (r, st) = x.exp10(rm);
            assert!(equal(r, want), "exp10({literal}) at {rm:?}: got {r}");
            assert_eq!(st, Status::OK, "exp10({literal}) at {rm:?}: flags");
        }
    }
}

// ---------------------------------------------------------------------------
// Beyond the representable range: the §7.4 dispositions, per mode.

/// Integers above `emax`, the gate gap integer included: `+∞` at both
/// nearest modes and toward `+∞`, `MAX` toward zero and toward `−∞`,
/// always `OVERFLOW + INEXACT`.
#[test]
fn above_range_integers_overflow_per_mode() {
    let mut saw_gap = false;
    for n in EMAX + 1..=EMAX + 40 {
        if n == GATE_GAP {
            saw_gap = true;
        }
        let x = int(n);
        for rm in [NE, NA, TP] {
            let (r, st) = x.exp10(rm);
            assert!(
                r.is_infinite() && !r.is_sign_negative(),
                "exp10({n}) at {rm:?}: want +inf, got {r}"
            );
            assert!(
                st.overflow() && st.inexact(),
                "exp10({n}) at {rm:?}: want OVERFLOW + INEXACT, got {st:?}"
            );
        }
        for rm in [TZ, TN] {
            let (r, st) = x.exp10(rm);
            assert!(
                equal(r, Decimal32::MAX),
                "exp10({n}) at {rm:?}: want MAX, got {r}"
            );
            assert!(
                st.overflow() && st.inexact(),
                "exp10({n}) at {rm:?}: want OVERFLOW + INEXACT, got {st:?}"
            );
        }
    }
    assert!(
        saw_gap,
        "the gate gap integer {GATE_GAP} must be inside the swept band"
    );
}

/// Integers below `etiny`: the true value is a tenth of the smallest
/// subnormal, so `+0` at both nearest modes, toward zero and toward
/// `−∞`, and the smallest subnormal toward `+∞`, always
/// `UNDERFLOW + INEXACT`.
#[test]
fn below_range_integers_underflow_per_mode() {
    for n in (ETINY - 40)..ETINY {
        let x = int(n);
        for rm in [NE, NA, TZ, TN] {
            let (r, st) = x.exp10(rm);
            assert!(
                r.is_zero() && !r.is_sign_negative(),
                "exp10({n}) at {rm:?}: want +0, got {r}"
            );
            assert!(
                st.underflow() && st.inexact(),
                "exp10({n}) at {rm:?}: want UNDERFLOW + INEXACT, got {st:?}"
            );
        }
        let (r, st) = x.exp10(TP);
        assert!(
            equal(r, Decimal32::MIN_POSITIVE),
            "exp10({n}) at TowardPositive: want the smallest subnormal, got {r}"
        );
        assert!(
            st.underflow() && st.inexact(),
            "exp10({n}) at TowardPositive: want UNDERFLOW + INEXACT, got {st:?}"
        );
    }
}

/// The far ends of the classifier's own decode window. `10^±99,999`
/// sits hundreds of orders of magnitude outside the format's exponent
/// range, so `pack_value` hands the rounder an exponent nothing else in
/// the crate produces: the §7.4 dispositions must still come out, and
/// the rounder's digit drop arithmetic must not wrap on the way.
#[test]
fn the_decode_window_ends_deliver_the_same_dispositions() {
    for n in [10_000i32, 50_000, 99_998, 99_999] {
        let x = int(n);
        let (r, st) = x.exp10(NE);
        assert!(
            r.is_infinite() && !r.is_sign_negative(),
            "exp10({n}) = {r}, want +inf"
        );
        assert!(st.overflow() && st.inexact(), "exp10({n}): {st:?}");
        let (r, st) = x.exp10(TZ);
        assert!(equal(r, Decimal32::MAX), "exp10({n}) at TowardZero = {r}");
        assert!(st.overflow() && st.inexact(), "exp10({n}) at TowardZero");

        let x = int(-n);
        let (r, st) = x.exp10(NE);
        assert!(
            r.is_zero() && !r.is_sign_negative(),
            "exp10({}) = {r}, want +0",
            -n
        );
        assert!(st.underflow() && st.inexact(), "exp10({}): {st:?}", -n);
        let (r, st) = x.exp10(TP);
        assert!(
            equal(r, Decimal32::MIN_POSITIVE),
            "exp10({}) at TowardPositive = {r}",
            -n
        );
        assert!(st.underflow() && st.inexact(), "exp10({}) at TP", -n);
    }
}

/// Integers past the classifier's five digit decode window take the
/// `exp` saturation gate instead (`|n · ln 10| > 230,000`, past both
/// the 224 overflow and the 235 underflow limits) and must land on the
/// same dispositions.
#[test]
fn integers_past_the_decode_limit_saturate() {
    for literal in ["100000", "999999", "1e6", "2e9"] {
        let x = parse(literal);
        let (r, st) = x.exp10(NE);
        assert!(
            r.is_infinite() && !r.is_sign_negative(),
            "exp10({literal}) = {r}"
        );
        assert!(st.overflow() && st.inexact(), "exp10({literal}): {st:?}");
        let (r, st) = x.exp10(TZ);
        assert!(
            equal(r, Decimal32::MAX),
            "exp10({literal}) at TowardZero = {r}"
        );
        assert!(st.overflow() && st.inexact(), "exp10({literal}) at TZ");

        let x = parse(&format!("-{literal}"));
        let (r, st) = x.exp10(NE);
        assert!(
            r.is_zero() && !r.is_sign_negative(),
            "exp10(-{literal}) = {r}"
        );
        assert!(st.underflow() && st.inexact(), "exp10(-{literal}): {st:?}");
        let (r, st) = x.exp10(TP);
        assert!(
            equal(r, Decimal32::MIN_POSITIVE),
            "exp10(-{literal}) at TowardPositive = {r}"
        );
        assert!(st.underflow() && st.inexact(), "exp10(-{literal}) at TP");
    }
}

// ---------------------------------------------------------------------------
// Flag honesty (§7.5) and the non-integer controls.

/// Non-integer inputs have irrational `10^x` and must raise `INEXACT`
/// in every direction; the integer family must raise nothing at all.
#[test]
fn inexact_flag_is_honest_in_every_mode() {
    for literal in [
        "0.5",
        "-0.5",
        "2.5",
        "1e-30",
        "-1e-30",
        "1.000001",
        "3.141593",
        "-1234.567",
        "96.5",
        "-101.5",
        "96.99999",
        "97.00001",
    ] {
        let x = parse(literal);
        for rm in ALL {
            let (_, st) = x.exp10(rm);
            assert!(st.inexact(), "exp10({literal}) at {rm:?}: expected INEXACT");
        }
    }
}

/// The neighbours of an integer straddle the exact value: `exp10` is
/// strictly increasing, so the step above rounds above `10^n` toward
/// `+∞` and the step below rounds under it toward `−∞`.
#[test]
fn neighbours_of_an_integer_straddle_the_exact_value() {
    for n in [1i32, 2, 7] {
        let x = int(n);
        let want = parse(&format!("1e{n}"));

        let (up, _) = x.next_up();
        let (r, st) = up.exp10(TP);
        assert!(
            r.partial_cmp(want).0 == Some(core::cmp::Ordering::Greater),
            "exp10(next_up({n})) at TowardPositive must exceed 10^{n}, got {r}"
        );
        assert!(st.inexact(), "exp10(next_up({n})): expected INEXACT");

        let (dn, _) = x.next_down();
        let (r, st) = dn.exp10(TN);
        assert!(
            r.partial_cmp(want).0 == Some(core::cmp::Ordering::Less),
            "exp10(next_down({n})) at TowardNegative must fall below 10^{n}, got {r}"
        );
        assert!(st.inexact(), "exp10(next_down({n})): expected INEXACT");
    }
}

/// The ADR-0051 1 anchor, inherited from the shared `exp` core: below
/// the working resolution the series collapses onto 1, a grid point at
/// every precision, and the directed modes take their side from the
/// sign of `x` (`10^x > 1` iff `x > 0`).
#[test]
fn tiny_arguments_hug_the_one_anchor_on_the_correct_side() {
    let one = Decimal32::ONE;
    for literal in ["1e-60", "1e-80", "1e-99"] {
        let x = parse(literal);
        for rm in [NE, NA, TZ, TN] {
            let (r, st) = x.exp10(rm);
            assert!(equal(r, one), "exp10({literal}) at {rm:?}: want 1, got {r}");
            assert!(st.inexact(), "exp10({literal}) at {rm:?}: INEXACT");
        }
        let (r, _) = x.exp10(TP);
        assert!(
            equal(r, one.next_up().0),
            "exp10({literal}) at TowardPositive: want next_up(1), got {r}"
        );

        let x = parse(&format!("-{literal}"));
        for rm in [NE, NA, TP] {
            let (r, st) = x.exp10(rm);
            assert!(
                equal(r, one),
                "exp10(-{literal}) at {rm:?}: want 1, got {r}"
            );
            assert!(st.inexact(), "exp10(-{literal}) at {rm:?}: INEXACT");
        }
        for rm in [TZ, TN] {
            let (r, _) = x.exp10(rm);
            assert!(
                equal(r, one.next_down().0),
                "exp10(-{literal}) at {rm:?}: want next_down(1), got {r}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Special values, IEEE 754-2019 §9.2.1 (`exp`'s dispositions).

#[test]
fn special_values_every_mode() {
    for rm in ALL {
        let (r, st) = Decimal32::ZERO.exp10(rm);
        assert!(equal(r, Decimal32::ONE), "exp10(+0) at {rm:?}: got {r}");
        assert_eq!(st, Status::OK, "exp10(+0) at {rm:?}: flags");

        let (r, st) = Decimal32::NEG_ZERO.exp10(rm);
        assert!(equal(r, Decimal32::ONE), "exp10(-0) at {rm:?}: got {r}");
        assert_eq!(st, Status::OK, "exp10(-0) at {rm:?}: flags");

        let (r, st) = Decimal32::NEG_INFINITY.exp10(rm);
        assert!(
            r.is_zero() && !r.is_sign_negative(),
            "exp10(-inf) at {rm:?}: want +0, got {r}"
        );
        assert_eq!(st, Status::OK, "exp10(-inf) at {rm:?}: flags");

        let (r, st) = Decimal32::INFINITY.exp10(rm);
        assert!(
            r.is_infinite() && !r.is_sign_negative(),
            "exp10(+inf) at {rm:?}"
        );
        assert_eq!(st, Status::OK, "exp10(+inf) at {rm:?}: flags");

        let (r, st) = Decimal32::NAN.exp10(rm);
        assert!(r.is_nan(), "exp10(NaN) at {rm:?}");
        assert_eq!(st, Status::OK, "exp10(NaN) at {rm:?}: flags");

        let (r, st) = Decimal32::SIGNALING_NAN.exp10(rm);
        assert!(r.is_nan() && !r.is_signaling_nan(), "exp10(sNaN) at {rm:?}");
        assert!(st.invalid(), "exp10(sNaN) at {rm:?}: flags");
    }
}
