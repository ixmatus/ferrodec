//! Exact-result, range-disposition, special-value, and flag gate for
//! `Decimal128`'s `exp10` (IEEE 754-2019 §9.2; ADR-0059 Track D).
//!
//! `10^x` is rational only at integer `x`: writing `x = a/b` in lowest
//! terms, unique factorization of `10 = 2·5` forces the value's 2
//! exponent and its 5 exponent each to equal `a/b`, both integers, so
//! `b = 1`. The exact family is therefore the integers, with value
//! `10^n` and coefficient 1, exactly representable for every `n` from
//! `etiny = −6176` to `emax = 6144`. The family is small enough to
//! walk exhaustively (12,321 inputs in five rounding directions), so
//! this gate does exactly that rather than sampling it.
//!
//! Past the representable range the classifier still decides, and that
//! is the load bearing half. `10^n` for `n > emax` sits exactly ON a
//! grid point at its own exponent, a distance no rung of the ADR-0059
//! ladder can grow, and the `exp` overflow gate leaves exactly one
//! such integer uncovered at each format (`n = 6145` here, since
//! `6145 · ln 10 ≈ 14149.4` stays inside the 14150 limit). Delivered
//! input side, the format rounder's §7.4 disposition answers those and
//! the underflow side alike. The `ladder_audit` battery lane runs this
//! file, so the gate gap integer below is the family's standing
//! witness: without the classifier it panics that lane by
//! construction.

#![cfg(feature = "exp-log")]

use ferrodec::{Decimal128, RoundingMode, Status};

const NE: RoundingMode = RoundingMode::NearestEven;
const NA: RoundingMode = RoundingMode::NearestAway;
const TZ: RoundingMode = RoundingMode::TowardZero;
const TP: RoundingMode = RoundingMode::TowardPositive;
const TN: RoundingMode = RoundingMode::TowardNegative;
const ALL: [RoundingMode; 5] = [NE, NA, TZ, TP, TN];

/// Largest decimal exponent of a representable `Decimal128`: `10^6144`
/// is the last power of ten inside the format.
const EMAX: i32 = 6144;
/// Smallest decimal exponent of a representable `Decimal128`: `10^-6176`
/// is the smallest positive subnormal.
const ETINY: i32 = -6176;
/// The integer the `exp` overflow gate does not catch: `6145 · ln 10 ≈
/// 14149.4`, inside the format's 14150 limit, while `10^6145` is past
/// `MAX`. The classifier's reason for existing.
const GATE_GAP: i32 = 6145;

fn parse(s: &str) -> Decimal128 {
    Decimal128::parse_str(s, NE)
        .unwrap_or_else(|_| panic!("literal parses: {s:?}"))
        .0
}

/// Value equality, cohort insensitive (the IEEE `compare` the corpus
/// gate uses).
fn equal(a: Decimal128, b: Decimal128) -> bool {
    a.partial_cmp(b).0 == Some(core::cmp::Ordering::Equal)
}

// ---------------------------------------------------------------------------
// The exact family, exhaustively, in every rounding direction.

/// Every representable member of the exact family: `10^n` for `n` from
/// `etiny` to `emax`, the whole exponent range including the subnormal
/// tail. Each must deliver exactly `1E{n}` with `Status::OK` in all
/// five directions: the value because the classifier packs the true
/// coefficient, and the clean status because §7.5 forbids `INEXACT` on
/// an exact result, and forbids `UNDERFLOW` on an exact subnormal one,
/// which the `n < −6143` half of this sweep is the witness for.
#[test]
fn exact_powers_of_ten_every_mode() {
    for n in ETINY..=EMAX {
        let x = Decimal128::from_i32(n);
        let want = parse(&format!("1e{n}"));
        for rm in ALL {
            let (r, st) = x.exp10(rm);
            assert!(equal(r, want), "exp10({n}) at {rm:?}: got {r}, want 1e{n}");
            assert_eq!(st, Status::OK, "exp10({n}) at {rm:?}: flags");
        }
    }
}

/// The independent witness for a sample of the family: `log10` takes
/// the delivered `10^n` back to `n`. A different kernel, a different
/// classifier (`exact::log10_exact`), and a different direction of the
/// same number theory, so agreement is evidence rather than tautology.
#[test]
fn log10_takes_the_family_back() {
    for n in [
        ETINY, -6143, -6000, -1000, -34, -1, 1, 2, 34, 1000, 6000, EMAX,
    ] {
        let x = Decimal128::from_i32(n);
        let (p, _) = x.exp10(NE);
        let (back, st) = p.log10(NE);
        assert!(equal(back, x), "log10(exp10({n})) = {back}, want {n}");
        assert_eq!(st, Status::OK, "log10(10^{n}) is exact");
    }
}

/// Cohort insensitivity: the classifier reads the stripped form, so an
/// integer stored at another quantum takes the exact path too, and a
/// value with a nonzero fractional digit never does.
#[test]
fn cohort_variants_of_an_integer_are_still_exact() {
    for (literal, n) in [
        ("2", 2),
        ("2.000", 2),
        ("2.0e0", 2),
        ("2E+0", 2),
        ("20e-1", 2),
        ("1e1", 10),
        ("100e-1", 10),
        ("-3.000", -3),
        ("-30e-1", -3),
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

/// Integers above `emax`, including the gate gap integer explicitly.
/// The true value `10^n` is past `MAX`, so §7.4 asks for `+∞` at both
/// nearest modes and toward `+∞`, and the largest finite magnitude
/// toward zero and toward `−∞`, always with `OVERFLOW` and `INEXACT`.
#[test]
fn above_range_integers_overflow_per_mode() {
    let mut saw_gap = false;
    for n in EMAX + 1..=EMAX + 40 {
        if n == GATE_GAP {
            saw_gap = true;
        }
        let x = Decimal128::from_i32(n);
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
                equal(r, Decimal128::MAX),
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

/// Integers below `etiny`. The true value `10^n ≤ 10^(etiny−1)` is a
/// tenth of the smallest subnormal, hence far below the half of it
/// that decides the nearest modes, so §7.4 asks for `+0` at both
/// nearest modes, toward zero and toward `−∞`, and the smallest
/// subnormal toward `+∞`, always with `UNDERFLOW` and `INEXACT`.
#[test]
fn below_range_integers_underflow_per_mode() {
    for n in (ETINY - 40)..ETINY {
        let x = Decimal128::from_i32(n);
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
            equal(r, Decimal128::MIN_POSITIVE),
            "exp10({n}) at TowardPositive: want the smallest subnormal, got {r}"
        );
        assert!(
            st.underflow() && st.inexact(),
            "exp10({n}) at TowardPositive: want UNDERFLOW + INEXACT, got {st:?}"
        );
    }
}

/// The far ends of the classifier's own decode window. `10^±99,999`
/// sits four orders of magnitude outside the format's exponent range,
/// so `pack_value` hands the rounder an exponent nothing else in the
/// crate produces: the §7.4 dispositions must still come out, and the
/// rounder's digit drop arithmetic must not wrap on the way. The band
/// between the representable range and these ends has no structure of
/// its own, so the ends plus two interior points are the coverage that
/// matters.
#[test]
fn the_decode_window_ends_deliver_the_same_dispositions() {
    for n in [10_000i32, 50_000, 99_998, 99_999] {
        let x = Decimal128::from_i32(n);
        let (r, st) = x.exp10(NE);
        assert!(
            r.is_infinite() && !r.is_sign_negative(),
            "exp10({n}) = {r}, want +inf"
        );
        assert!(st.overflow() && st.inexact(), "exp10({n}): {st:?}");
        let (r, st) = x.exp10(TZ);
        assert!(equal(r, Decimal128::MAX), "exp10({n}) at TowardZero = {r}");
        assert!(st.overflow() && st.inexact(), "exp10({n}) at TowardZero");

        let x = Decimal128::from_i32(-n);
        let (r, st) = x.exp10(NE);
        assert!(
            r.is_zero() && !r.is_sign_negative(),
            "exp10({}) = {r}, want +0",
            -n
        );
        assert!(st.underflow() && st.inexact(), "exp10({}): {st:?}", -n);
        let (r, st) = x.exp10(TP);
        assert!(
            equal(r, Decimal128::MIN_POSITIVE),
            "exp10({}) at TowardPositive = {r}",
            -n
        );
        assert!(st.underflow() && st.inexact(), "exp10({}) at TP", -n);
    }
}

/// Integers past the classifier's five digit decode window take the
/// `exp` saturation gate instead (`|n · ln 10| > 230,000`, past both
/// the 14,150 overflow and the 14,221 underflow limits), and must land
/// on the same dispositions. The controls that keep the decode limit
/// from being a silent correctness cliff.
#[test]
fn integers_past_the_decode_limit_saturate() {
    for n in [100_000i32, 999_999, 1_000_000, 2_000_000_000] {
        let x = Decimal128::from_i32(n);
        let (r, st) = x.exp10(NE);
        assert!(r.is_infinite() && !r.is_sign_negative(), "exp10({n}) = {r}");
        assert!(st.overflow() && st.inexact(), "exp10({n}): {st:?}");
        let (r, st) = x.exp10(TZ);
        assert!(equal(r, Decimal128::MAX), "exp10({n}) at TowardZero = {r}");
        assert!(st.overflow() && st.inexact(), "exp10({n}) at TowardZero");

        let x = Decimal128::from_i32(-n);
        let (r, st) = x.exp10(NE);
        assert!(r.is_zero() && !r.is_sign_negative(), "exp10({}) = {r}", -n);
        assert!(st.underflow() && st.inexact(), "exp10({}): {st:?}", -n);
        let (r, st) = x.exp10(TP);
        assert!(
            equal(r, Decimal128::MIN_POSITIVE),
            "exp10({}) at TowardPositive = {r}",
            -n
        );
        assert!(st.underflow() && st.inexact(), "exp10({}) at TP", -n);
    }
}

// ---------------------------------------------------------------------------
// Flag honesty (§7.5) and the non-integer controls.

/// Non-integer inputs have irrational `10^x` and must raise `INEXACT`
/// in every direction; the integer family must raise nothing at all.
/// Together the two halves are the §7.5 contract: the flag says "the
/// delivered value differs from the true one", never "the kernel
/// rounded something internally".
#[test]
fn inexact_flag_is_honest_in_every_mode() {
    for literal in [
        "0.5",
        "-0.5",
        "2.5",
        "1e-30",
        "-1e-30",
        "1.000000000000000000000000000000001",
        "3.141592653589793238462643383279503",
        "-1234.5678",
        "6144.5",
        "-6176.5",
        // Just off the gate gap integer on both sides: still the
        // kernel's own path, still inexact.
        "6144.999999999999999999999999999999",
        "6145.000000000000000000000000000001",
    ] {
        let x = parse(literal);
        for rm in ALL {
            let (_, st) = x.exp10(rm);
            assert!(st.inexact(), "exp10({literal}) at {rm:?}: expected INEXACT");
        }
    }
}

/// The neighbours of an integer are ordinary irrational cases and must
/// bracket the exact value strictly: `exp10` is strictly increasing, so
/// the step above `n` rounds above `10^n` under `TowardPositive` and
/// the step below rounds under it at `TowardNegative`. Small `n` only,
/// where an ulp of the input still moves the result by less than a
/// decade.
#[test]
fn neighbours_of_an_integer_straddle_the_exact_value() {
    for n in [1i32, 2, 34] {
        let x = Decimal128::from_i32(n);
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

/// The ADR-0051 1 anchor, inherited from `exp_from_extended_body`: for
/// `|x|` below the working resolution the series collapses onto 1, a
/// grid point at every precision, and the directed modes need the side,
/// which is the sign of `x` (`10^x > 1` iff `x > 0`). Every mode raises
/// `INEXACT`, since `10^x ≠ 1` for `x ≠ 0`.
#[test]
fn tiny_arguments_hug_the_one_anchor_on_the_correct_side() {
    let one = Decimal128::ONE;
    for literal in ["1e-60", "1e-3000", "1e-6100"] {
        let x = parse(literal);
        for rm in [NE, NA, TZ, TN] {
            let (r, st) = x.exp10(rm);
            assert!(equal(r, one), "exp10({literal}) at {rm:?}: want 1, got {r}");
            assert!(st.inexact(), "exp10({literal}) at {rm:?}: INEXACT");
        }
        let (r, st) = x.exp10(TP);
        assert!(
            equal(r, one.next_up().0),
            "exp10({literal}) at TowardPositive: want next_up(1), got {r}"
        );
        assert!(st.inexact(), "exp10({literal}) at TowardPositive: INEXACT");

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
            let (r, st) = x.exp10(rm);
            assert!(
                equal(r, one.next_down().0),
                "exp10(-{literal}) at {rm:?}: want next_down(1), got {r}"
            );
            assert!(st.inexact(), "exp10(-{literal}) at {rm:?}: INEXACT");
        }
    }
}

// ---------------------------------------------------------------------------
// Special values, IEEE 754-2019 §9.2.1 (`exp`'s dispositions, not the
// `expm1` family's).

#[test]
fn special_values_every_mode() {
    for rm in ALL {
        let (r, st) = Decimal128::ZERO.exp10(rm);
        assert!(equal(r, Decimal128::ONE), "exp10(+0) at {rm:?}: got {r}");
        assert_eq!(st, Status::OK, "exp10(+0) at {rm:?}: flags");

        let (r, st) = Decimal128::NEG_ZERO.exp10(rm);
        assert!(equal(r, Decimal128::ONE), "exp10(-0) at {rm:?}: got {r}");
        assert_eq!(st, Status::OK, "exp10(-0) at {rm:?}: flags");

        let (r, st) = Decimal128::NEG_INFINITY.exp10(rm);
        assert!(
            r.is_zero() && !r.is_sign_negative(),
            "exp10(-inf) at {rm:?}: want +0, got {r}"
        );
        assert_eq!(st, Status::OK, "exp10(-inf) at {rm:?}: flags");

        let (r, st) = Decimal128::INFINITY.exp10(rm);
        assert!(
            r.is_infinite() && !r.is_sign_negative(),
            "exp10(+inf) at {rm:?}"
        );
        assert_eq!(st, Status::OK, "exp10(+inf) at {rm:?}: flags");

        let (r, st) = Decimal128::NAN.exp10(rm);
        assert!(r.is_nan(), "exp10(NaN) at {rm:?}");
        assert_eq!(st, Status::OK, "exp10(NaN) at {rm:?}: flags");

        let (r, st) = Decimal128::SIGNALING_NAN.exp10(rm);
        assert!(r.is_nan() && !r.is_signaling_nan(), "exp10(sNaN) at {rm:?}");
        assert!(st.invalid(), "exp10(sNaN) at {rm:?}: flags");
    }
}
