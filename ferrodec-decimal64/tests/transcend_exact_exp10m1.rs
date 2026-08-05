//! Integer-argument, special-value, and flag gate for `Decimal64`'s
//! `exp10_m1` (IEEE 754-2019 §9.2 `exp10m1`; ADR-0059 Track D). The
//! sibling mirror of the root crate's
//! `tests/transcend_exact_exp10m1.rs`.
//!
//! `exp10m1(x) = 10^x − 1` takes a rational value only at an integer
//! argument: `1 + r = 10^x` rational forces `x = a/b` with `b = 1` by
//! unique factorization, so the exact family is the *nines patterns*
//! `9`, `99`, `999`, … above zero and `−0.9`, `−0.99`, … below it,
//! bounded by the format's digit width. Nines never end in 5, so the
//! family carries no nearest-mode tie.
//!
//! The integers past that width are the reason this gate is
//! exhaustive rather than a sample. `10^n ⊖ 1` keeps every digit of
//! `10^n` once `n` passes the working width, which puts the working
//! value exactly ON the grid point `1·10^n`: a distance no rung
//! grows, so without the input-side classifier the three directed
//! modes would be decided by the sign of the kernel's own noise
//! across thousands of inputs, and the gap integer between the
//! overflow gate and the true overflow boundary (`n = 385` here)
//! would carry the wrong §7.4 flags. `exact::exp10m1_integer`
//! answers the whole integer family instead: exactly inside the digit
//! width, and beyond it through an all nines proxy whose soundness is
//! total digit knowledge (the true value's expansion is `|n|` nines,
//! so the proxy hands the rounder the true value's own kept digits,
//! round digit, and sticky).
//!
//! The expectations below are constructed independently of the
//! kernel: nines literals through the exact range, and past it the
//! two rounded neighbours `10^n` and the `PRECISION` nines value,
//! written out as literals.

#![cfg(feature = "exp-log")]

use ferrodec_decimal64::{Decimal64, RoundingMode, Status};

const NE: RoundingMode = RoundingMode::NearestEven;
const NA: RoundingMode = RoundingMode::NearestAway;
const TZ: RoundingMode = RoundingMode::TowardZero;
const TP: RoundingMode = RoundingMode::TowardPositive;
const TN: RoundingMode = RoundingMode::TowardNegative;
const ALL: [RoundingMode; 5] = [NE, NA, TZ, TP, TN];

/// `Decimal64` significand width: the exact family's `|n|` ceiling.
const PRECISION: u32 = 16;
/// `Decimal64`'s largest exponent: `10^E_MAX` is representable,
/// `10^(E_MAX + 1)` is not. `E_MAX + 1` is also the gap integer
/// between the kernel's overflow gate (which fires above
/// `x ≈ 385.4`) and the true overflow boundary.
const E_MAX: i32 = 384;

fn parse(s: &str) -> Decimal64 {
    Decimal64::parse_str(s, NE)
        .unwrap_or_else(|_| panic!("literal parses: {s:?}"))
        .0
}

fn equal(a: Decimal64, b: Decimal64) -> bool {
    a.partial_cmp(b).0 == Some(core::cmp::Ordering::Equal)
}

/// The `k` nines integer `10^k − 1` as a decimal literal.
fn nines(k: u32) -> String {
    "9".repeat(k as usize)
}

/// The two rounded neighbours of `10^n − 1` for `n > PRECISION`, as
/// literals: the value rounded away from zero (`10^n`) and the value
/// rounded toward zero (the `PRECISION` nines scaled to `10^n`'s
/// decade). Independent of the kernel and of the classifier.
fn positive_neighbours(n: i32) -> (Decimal64, Decimal64) {
    let away = parse(&format!("1e{n}"));
    let toward = parse(&format!("{}e{}", nines(PRECISION), n - PRECISION as i32));
    (away, toward)
}

/// The negative mirror, constant in `m`: the true value
/// `−(1 − 10^−m)` for `m > PRECISION` lies strictly between `−1` and
/// its toward-zero neighbour `−(1 − 10^−PRECISION)`, and strictly
/// above their midpoint, so those two literals are the only possible
/// answers.
fn negative_neighbours() -> (Decimal64, Decimal64) {
    let away = parse("-1");
    let toward = parse(&format!("-{}e-{PRECISION}", nines(PRECISION)));
    (away, toward)
}

/// Assert the delivery at a single integer argument in all five
/// directions, against independently constructed expectations.
fn check_integer(n: i32) {
    let x = parse(&format!("{n}"));
    let m = n.unsigned_abs();
    if m <= PRECISION {
        // The exact family: the `|n|` nines integer above zero, the
        // `|n|` nines fraction below it. §7.5 forbids INEXACT here.
        let want = if n < 0 {
            parse(&format!("-{}e-{m}", nines(m)))
        } else {
            parse(&nines(m))
        };
        for rm in ALL {
            let (r, st) = x.exp10_m1(rm);
            assert!(
                equal(r, want),
                "exp10_m1({n}) at {rm:?}: got {r}, want {want}"
            );
            assert_eq!(st, Status::OK, "exp10_m1({n}) at {rm:?}: flags");
        }
        return;
    }
    let (away, toward) = if n < 0 {
        negative_neighbours()
    } else {
        positive_neighbours(n)
    };
    // Away from zero at the nearest modes (the dropped digits are all
    // nines, so the round digit is 9 with a nonzero sticky), and at
    // the directed mode that moves away from zero.
    let away_modes = if n < 0 { [NE, NA, TN] } else { [NE, NA, TP] };
    for rm in away_modes {
        let (r, st) = x.exp10_m1(rm);
        assert!(
            equal(r, away),
            "exp10_m1({n}) at {rm:?}: got {r}, want {away}"
        );
        assert!(st.inexact(), "exp10_m1({n}) at {rm:?}: expected INEXACT");
        assert!(!st.overflow(), "exp10_m1({n}) at {rm:?}: spurious OVERFLOW");
        assert!(
            !st.underflow(),
            "exp10_m1({n}) at {rm:?}: spurious UNDERFLOW"
        );
    }
    let toward_modes = if n < 0 { [TZ, TP] } else { [TZ, TN] };
    for rm in toward_modes {
        let (r, st) = x.exp10_m1(rm);
        assert!(
            equal(r, toward),
            "exp10_m1({n}) at {rm:?}: got {r}, want {toward}"
        );
        assert!(st.inexact(), "exp10_m1({n}) at {rm:?}: expected INEXACT");
        assert!(!st.overflow(), "exp10_m1({n}) at {rm:?}: spurious OVERFLOW");
    }
}

// ---------------------------------------------------------------------------
// The integer family.

/// Every integer in `[−3(PRECISION+1), 3(PRECISION+1)]`, in all five
/// rounding directions: the exact nines patterns inside the digit
/// width, and the proxy's two neighbours outside it. The span reaches
/// three times past the exact family's edge, so it covers the
/// `PRECISION + 1` boundary (empty sticky), the first proxy input
/// (`PRECISION + 2`), and a long tail of proxy inputs whose dropped
/// digits are all nines.
#[test]
fn integer_family_exhaustive_every_mode() {
    let span = 3 * (PRECISION as i32 + 1);
    for n in -span..=span {
        if n == 0 {
            continue; // exp10m1(±0) = ±0, a special case, tested below
        }
        check_integer(n);
    }
}

/// Coarse integer rows far outside the exhaustive span, where the
/// working value would be hopelessly grid-stuck: both sides of the
/// decade scale, the format's largest representable decade, an
/// `etiny`-scale negative, and the decode limit's own crossing. The
/// last pair is the classifier's bail proof made live: `n = 99,999`
/// is answered by the classifier and `n = 100,000` by the kernel's
/// gates (`|u| = |n| · ln 10` clears both), and the two must agree
/// mode for mode.
#[test]
fn coarse_integer_rows_every_mode() {
    for n in [
        200i32, 380, E_MAX, -200, -1000,
        -398, // etiny scale: the result still sits just above −1
        -99_999, -100_000,
    ] {
        check_integer(n);
    }
}

/// The rung-2 misround family witness. At `n = 200` the working value
/// `10^200 ⊖ 1` is exactly the grid point `1e200` at both fixed rungs
/// (rung 1 absorbs the `1` from `n = 50`, rung 2 from `n = 110`), so
/// before the classifier the directed modes were decided by the sign
/// of the kernel's noise. `TowardZero` is the row that pins it: the
/// true value `10^200 − 1` is one unit below `1e200`, so the answer
/// is the 16 nines value at that decade, written out here in full.
#[test]
fn rung2_misround_family_witness_at_200() {
    let x = parse("200");
    let want_toward = parse("9.999999999999999e199");
    let want_away = parse("1e200");

    let (r, st) = x.exp10_m1(TZ);
    assert!(
        equal(r, want_toward),
        "exp10_m1(200) at TowardZero: got {r}, want {want_toward}"
    );
    assert!(st.inexact(), "exp10_m1(200) at TowardZero: INEXACT");
    let (r, _) = x.exp10_m1(TN);
    assert!(
        equal(r, want_toward),
        "exp10_m1(200) at TowardNegative: got {r}, want {want_toward}"
    );
    for rm in [NE, NA, TP] {
        let (r, _) = x.exp10_m1(rm);
        assert!(
            equal(r, want_away),
            "exp10_m1(200) at {rm:?}: got {r}, want {want_away}"
        );
    }
}

/// The independent witness for the exact family: the delivered value
/// plus one reproduces `10^n` exactly, once in `u128` integers and
/// once through the format's own `add`. Neither route runs the
/// classifier's arithmetic.
#[test]
fn exact_family_reconstructs_ten_to_the_n() {
    let one = Decimal64::ONE;
    for n in 1..=PRECISION {
        let v: u128 = nines(n).parse().expect("nines fit u128");
        assert_eq!(v + 1, 10u128.pow(n), "the {n} nines integer plus one");

        let (r, _) = parse(&format!("{n}")).exp10_m1(NE);
        let (sum, _) = r.add(one, NE);
        assert!(
            equal(sum, parse(&format!("1e{n}"))),
            "exp10_m1({n}) + 1 is 10^{n}, got {sum}"
        );
    }
    for m in 1..=PRECISION {
        let (r, _) = parse(&format!("-{m}")).exp10_m1(NE);
        let (sum, _) = r.add(one, NE);
        assert!(
            equal(sum, parse(&format!("1e-{m}"))),
            "exp10_m1(-{m}) + 1 is 10^-{m}, got {sum}"
        );
    }
}

// ---------------------------------------------------------------------------
// Overflow, per IEEE 754-2019 §7.4.

/// Past `E_MAX` the integer family overflows, and the disposition is
/// the rounder's own: `+∞` at the nearest modes and toward `+∞`, the
/// largest finite magnitude toward zero and toward `−∞`.
///
/// `n = E_MAX + 1` is the delicate row, and it is the gap integer the
/// overflow gate does not catch. Its true value `10^(E_MAX+1) − 1`
/// exceeds `MAX`, but what §7.4 asks is whether the value *rounded
/// with an unbounded exponent range* exceeds it: under `TowardZero`
/// and `TowardNegative` that rounding is exactly `MAX`, so those two
/// directions deliver `MAX` **without** `OVERFLOW` (`INEXACT` only),
/// while the other three overflow. One decade further up
/// (`n = E_MAX + 2`) every direction overflows. Delivering this from
/// the absorbed proxy `10^n` instead would raise `OVERFLOW` in all
/// five.
#[test]
fn overflow_rows_follow_the_74_disposition() {
    let max = parse(&format!(
        "{}e{}",
        nines(PRECISION),
        E_MAX - PRECISION as i32 + 1
    ));

    // The gap integer: TowardZero / TowardNegative land exactly on MAX.
    let x = parse(&format!("{}", E_MAX + 1));
    for rm in [TZ, TN] {
        let (r, st) = x.exp10_m1(rm);
        assert!(
            equal(r, max),
            "exp10_m1({}) at {rm:?}: got {r}, want MAX",
            E_MAX + 1
        );
        assert!(st.inexact(), "exp10_m1({}) at {rm:?}: INEXACT", E_MAX + 1);
        assert!(
            !st.overflow(),
            "exp10_m1({}) at {rm:?}: the unbounded-range rounding is MAX \
             itself, so §7.4 raises no OVERFLOW",
            E_MAX + 1
        );
    }
    for rm in [NE, NA, TP] {
        let (r, st) = x.exp10_m1(rm);
        assert!(
            r.is_infinite() && !r.is_sign_negative(),
            "exp10_m1({}) at {rm:?}: want +inf, got {r}",
            E_MAX + 1
        );
        assert!(
            st.overflow() && st.inexact(),
            "exp10_m1({}) at {rm:?}: want OVERFLOW | INEXACT",
            E_MAX + 1
        );
    }

    // Further out, every direction overflows; the decode limit's two
    // sides (99,999 classified, 100,000 gated) must agree here too.
    for n in [E_MAX + 2, 10_000, 99_999, 100_000] {
        for rm in [NE, NA, TP] {
            let (r, st) = parse(&format!("{n}")).exp10_m1(rm);
            assert!(
                r.is_infinite() && !r.is_sign_negative(),
                "exp10_m1({n}) at {rm:?}: want +inf, got {r}"
            );
            assert!(
                st.overflow() && st.inexact(),
                "exp10_m1({n}) at {rm:?}: want OVERFLOW | INEXACT"
            );
        }
        for rm in [TZ, TN] {
            let (r, st) = parse(&format!("{n}")).exp10_m1(rm);
            assert!(equal(r, max), "exp10_m1({n}) at {rm:?}: got {r}, want MAX");
            assert!(
                st.overflow() && st.inexact(),
                "exp10_m1({n}) at {rm:?}: want OVERFLOW | INEXACT"
            );
        }
    }
}

/// A tiny positive argument lands the result in the subnormal range:
/// `10^x − 1 ≈ x · ln 10`, so `UNDERFLOW` accompanies `INEXACT`
/// there, and a normal-range result must not raise it.
#[test]
fn subnormal_results_raise_underflow() {
    let (r, st) = parse("1e-395").exp10_m1(NE);
    assert!(r.is_subnormal(), "exp10_m1(1e-395) is subnormal, got {r}");
    assert!(
        st.underflow() && st.inexact(),
        "exp10_m1(1e-395): want UNDERFLOW | INEXACT"
    );

    let (r, st) = parse("1e-30").exp10_m1(NE);
    assert!(!r.is_subnormal(), "exp10_m1(1e-30) is normal, got {r}");
    assert!(st.inexact(), "exp10_m1(1e-30): INEXACT");
    assert!(
        !st.underflow(),
        "exp10_m1(1e-30): no spurious UNDERFLOW on a normal result"
    );
}

// ---------------------------------------------------------------------------
// The −1 band: the gate and the collapse seam.

/// Non-integer arguments far below zero, where the true value hugs
/// `−1` from above. Two regimes meet here and must agree: the
/// ADR-0051 collapse seam (the working subtraction rounds to exactly
/// `−1`, roughly `x ∈ (−52.1, −46.5)`) and the `−1` band gate below
/// it (`x · ln 10 < −120`). In both, the true value sits strictly
/// between `−1` and its toward-zero neighbour and above their
/// midpoint, so the nearest modes and `TowardNegative` deliver `−1`
/// and the other two the `PRECISION` nines fraction, always
/// `INEXACT`.
#[test]
fn minus_one_band_and_collapse_seam() {
    let (away, toward) = negative_neighbours();
    for literal in [
        "-40.5",   // above the collapse threshold: a live working value
        "-47.5",   // inside the collapse band
        "-52.5",   // just past the gate threshold (x · ln 10 < −120)
        "-500.5",  // deep in the gated band
        "-1.5e10", // far past the decode limit, still gated
    ] {
        let x = parse(literal);
        for rm in [NE, NA, TN] {
            let (r, st) = x.exp10_m1(rm);
            assert!(
                equal(r, away),
                "exp10_m1({literal}) at {rm:?}: got {r}, want -1"
            );
            assert!(st.inexact(), "exp10_m1({literal}) at {rm:?}: INEXACT");
        }
        for rm in [TZ, TP] {
            let (r, st) = x.exp10_m1(rm);
            assert!(
                equal(r, toward),
                "exp10_m1({literal}) at {rm:?}: got {r}, want {toward}"
            );
            assert!(st.inexact(), "exp10_m1({literal}) at {rm:?}: INEXACT");
        }
    }
}

// ---------------------------------------------------------------------------
// Flag honesty (§7.5) and the classifier's controls.

/// Generic finite arguments have irrational values and must raise
/// `INEXACT` in every direction; the exact family must raise nothing
/// at all, at every quantum of its cohort (the classifier reads the
/// stripped form).
#[test]
fn inexact_flag_is_honest_in_every_mode() {
    for literal in [
        "0.5",
        "-0.5",
        "1e-20",
        "-1e-20",
        "2.5",
        "123456.5",
        "-0.0009765625",
        "1234.567890123456",
        // Integers past the exact family: delivered by the all nines
        // proxy, INEXACT like everything else on this list.
        "35",
        "-35",
        "300",
    ] {
        let x = parse(literal);
        for rm in ALL {
            let (_, st) = x.exp10_m1(rm);
            assert!(
                st.inexact(),
                "exp10_m1({literal}) at {rm:?}: expected INEXACT"
            );
        }
    }
    for literal in [
        "2", "2.000", "9", "16", "1.6e1", "-2", "-2.000", "-16", "-1.6e1",
    ] {
        let x = parse(literal);
        for rm in ALL {
            let (_, st) = x.exp10_m1(rm);
            assert_eq!(
                st,
                Status::OK,
                "exp10_m1({literal}) at {rm:?}: exact result must raise nothing (§7.5)"
            );
        }
    }
}

/// The controls that keep the classifier from being vacuously green:
/// one ulp on either side of an integer argument is a non-integer, so
/// it stays on the kernel, raises `INEXACT`, and lands on the correct
/// side of the integer's own value.
#[test]
fn neighbours_of_the_integers_stay_on_the_kernel() {
    for n in [2i32, 8, 16] {
        let x = parse(&format!("{n}"));
        let want = parse(&nines(n as u32));

        let (up, _) = x.next_up();
        let (r, st) = up.exp10_m1(TP);
        assert!(st.inexact(), "exp10_m1(next_up({n})): expected INEXACT");
        assert!(
            r.partial_cmp(want).0 == Some(core::cmp::Ordering::Greater),
            "exp10_m1(next_up({n})) at TowardPositive must exceed 10^{n} − 1, got {r}"
        );

        let (dn, _) = x.next_down();
        let (r, st) = dn.exp10_m1(TN);
        assert!(st.inexact(), "exp10_m1(next_down({n})): expected INEXACT");
        assert!(
            r.partial_cmp(want).0 == Some(core::cmp::Ordering::Less),
            "exp10_m1(next_down({n})) at TowardNegative must fall below 10^{n} − 1, got {r}"
        );
    }
}

// ---------------------------------------------------------------------------
// Special values, IEEE 754-2019 §9.2.1.

#[test]
fn special_values_every_mode() {
    for rm in ALL {
        let (r, st) = Decimal64::ZERO.exp10_m1(rm);
        assert!(
            r.is_zero() && !r.is_sign_negative(),
            "exp10_m1(+0) at {rm:?}"
        );
        assert_eq!(st, Status::OK, "exp10_m1(+0) at {rm:?}: flags");

        let (r, st) = Decimal64::NEG_ZERO.exp10_m1(rm);
        assert!(
            r.is_zero() && r.is_sign_negative(),
            "exp10_m1(-0) at {rm:?}"
        );
        assert_eq!(st, Status::OK, "exp10_m1(-0) at {rm:?}: flags");

        // `exp10m1(−∞) = −1` exactly: the limit is attained, so no
        // exception is raised in any direction.
        let (r, st) = Decimal64::NEG_INFINITY.exp10_m1(rm);
        assert!(
            equal(r, Decimal64::NEG_ONE),
            "exp10_m1(-inf) at {rm:?}: want -1, got {r}"
        );
        assert_eq!(st, Status::OK, "exp10_m1(-inf) at {rm:?}: flags");

        let (r, st) = Decimal64::INFINITY.exp10_m1(rm);
        assert!(
            r.is_infinite() && !r.is_sign_negative(),
            "exp10_m1(+inf) at {rm:?}"
        );
        assert_eq!(st, Status::OK, "exp10_m1(+inf) at {rm:?}: flags");

        let (r, st) = Decimal64::NAN.exp10_m1(rm);
        assert!(r.is_nan(), "exp10_m1(NaN) at {rm:?}");
        assert_eq!(st, Status::OK, "exp10_m1(NaN) at {rm:?}: flags");

        let (r, st) = Decimal64::SIGNALING_NAN.exp10_m1(rm);
        assert!(
            r.is_nan() && !r.is_signaling_nan(),
            "exp10_m1(sNaN) at {rm:?}"
        );
        assert!(st.invalid(), "exp10_m1(sNaN) at {rm:?}: flags");
    }
}
