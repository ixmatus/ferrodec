//! Exact-result, tie, special-value, and quantum gate for `Decimal64`'s
//! `rsqrt` (IEEE 754-2019 §9.2 `rSqrt`; ADR-0059 Track D group D3,
//! under the ADR-0060 phase gate). The sibling mirror of the root
//! crate's `tests/transcend_exact_rsqrt.rs`; the two differ only in the
//! format's precision and exponent range, which move the exact family's
//! width ceilings and the tie rows.
//!
//! Writing the stripped input as `x = 2^A · 5^B · s` with
//! `gcd(s, 10) = 1`, `1/√x` is rational iff `s = 1` and both `A` and
//! `B` are even, and the value is then `2^−A/2 · 5^−B/2` folded to a
//! decimal. At 16 digits the two branches run out at `5^22` and `2^11`,
//! and `5^23` / `5^24` are the two 17-digit nearest mode midpoints —
//! the same two-per-format pattern `exp2`'s classifier records at
//! `exp2(-23)` / `exp2(-24)`.
//!
//! §9.2.2's preferred exponent `−⌊Q(x)/2⌋` is *not* what the kernel
//! delivers: every exact classifier packs through `pack_value`, which
//! asks the rounder for quantum 0. The delta is recorded, not repaired
//! (see the root file's header); both columns are pinned below.

#![cfg(feature = "exp-log")]

use core::cmp::Ordering;
use ferrodec_decimal64::{Decimal64, RoundingMode, Status};

const NE: RoundingMode = RoundingMode::NearestEven;
const NA: RoundingMode = RoundingMode::NearestAway;
const TZ: RoundingMode = RoundingMode::TowardZero;
const TP: RoundingMode = RoundingMode::TowardPositive;
const TN: RoundingMode = RoundingMode::TowardNegative;
const ALL: [RoundingMode; 5] = [NE, NA, TZ, TP, TN];

/// `PRECISION + 1` at this format, the tie width.
const TIE_WIDTH: usize = 17;

/// The two tie rows, `(d, input = 2^2d, midpoint, smaller, larger)`.
const TIE_ROWS: [(u32, &str, &str, &str, &str); 2] = [
    (
        23,
        "70368744177664",
        "1.1920928955078125E-7",
        "1.192092895507812E-7",
        "1.192092895507813E-7",
    ),
    (
        24,
        "281474976710656",
        "5.9604644775390625E-8",
        "5.960464477539062E-8",
        "5.960464477539063E-8",
    ),
];

fn parse(s: &str) -> Decimal64 {
    Decimal64::parse_str(s, NE)
        .unwrap_or_else(|_| panic!("test literal parses: {s:?}"))
        .0
}

fn parse_rm(s: &str, rm: RoundingMode) -> Decimal64 {
    Decimal64::parse_str(s, rm)
        .unwrap_or_else(|_| panic!("test literal parses: {s:?}"))
        .0
}

fn eq(got: Decimal64, want: Decimal64) -> bool {
    got.partial_cmp(want).0 == Some(Ordering::Equal)
}

/// The stored quantum exponent of a finite datum.
fn quantum(d: Decimal64) -> i32 {
    i32::from(d.decode().expect("finite datum decodes").exponent)
}

/// The significant digits of a `d.dddEsnn` literal, as a bare string.
fn digits_of(literal: &str) -> String {
    literal
        .split('E')
        .next()
        .expect("scientific literal")
        .replace('.', "")
}

fn assert_exact(got: (Decimal64, Status), want: Decimal64, label: &str) {
    let (r, st) = got;
    assert!(eq(r, want), "{label}: got {r}, want {want}");
    assert_eq!(
        st,
        Status::OK,
        "{label}: exact result must be OK, got {st:?}"
    );
    assert!(!st.inexact(), "{label}: §7.5 forbids INEXACT here");
}

/// IEEE 754-2019 §9.2.1, every row, every rounding direction.
#[test]
fn specials_per_section_9_2_1() {
    for rm in ALL {
        let (r, st) = Decimal64::INFINITY.rsqrt(rm);
        assert!(
            r.is_zero() && !r.is_sign_negative(),
            "rsqrt(+inf) [{rm:?}] = {r}"
        );
        assert_eq!(st, Status::OK, "rsqrt(+inf) [{rm:?}] status {st:?}");

        let (r, st) = Decimal64::ZERO.rsqrt(rm);
        assert!(
            r.is_infinite() && !r.is_sign_negative(),
            "rsqrt(+0) [{rm:?}] = {r}"
        );
        assert!(st.div_by_zero(), "rsqrt(+0) [{rm:?}] status {st:?}");
        assert!(!st.inexact(), "rsqrt(+0) [{rm:?}] is not inexact");

        let (r, st) = Decimal64::NEG_ZERO.rsqrt(rm);
        assert!(
            r.is_infinite() && r.is_sign_negative(),
            "rsqrt(-0) [{rm:?}] = {r}"
        );
        assert!(st.div_by_zero(), "rsqrt(-0) [{rm:?}] status {st:?}");

        for label in ["-4", "-1e-390", "-9.999999999999999e384", "-0.5"] {
            let (r, st) = parse(label).rsqrt(rm);
            assert!(r.is_nan(), "rsqrt({label}) [{rm:?}] = {r}, want NaN");
            assert!(st.invalid(), "rsqrt({label}) [{rm:?}] status {st:?}");
        }
        let (r, st) = Decimal64::NEG_INFINITY.rsqrt(rm);
        assert!(r.is_nan(), "rsqrt(-inf) [{rm:?}] = {r}");
        assert!(st.invalid(), "rsqrt(-inf) [{rm:?}] status {st:?}");

        let (r, st) = Decimal64::NAN.rsqrt(rm);
        assert!(r.is_nan() && st.is_ok(), "rsqrt(NaN) [{rm:?}] = {r} {st:?}");
        let (r, st) = Decimal64::SIGNALING_NAN.rsqrt(rm);
        assert!(
            r.is_nan() && st.invalid(),
            "rsqrt(sNaN) [{rm:?}] = {r} {st:?}"
        );
    }
}

/// The named exact values the public rustdoc quotes, every direction.
#[test]
fn documented_exact_values() {
    let cases: [(&str, &str); 13] = [
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

/// The power-of-ten family across the whole exponent range, both
/// directions: `2k ≤ 398` on the input side going down, `2k ≤ 384`
/// going up. Every result stays normal, which is the executable form
/// of the "no subnormal edge" claim at this format.
#[test]
fn power_of_ten_family_across_the_exponent_range() {
    for k in 1..=199i32 {
        let x = parse(&format!("1E-{}", 2 * k));
        let (r, st) = x.rsqrt(NE);
        assert!(
            eq(r, parse(&format!("1E+{k}"))),
            "rsqrt(1E-{}) = {r}, want 1E+{k}",
            2 * k
        );
        assert_eq!(st, Status::OK, "rsqrt(1E-{}) status {st:?}", 2 * k);
        assert!(!r.is_subnormal() && r.is_finite());
    }
    for k in 1..=192i32 {
        let x = parse(&format!("1E+{}", 2 * k));
        let (r, st) = x.rsqrt(NE);
        assert!(
            eq(r, parse(&format!("1E-{k}"))),
            "rsqrt(1E+{}) = {r}, want 1E-{k}",
            2 * k
        );
        assert_eq!(st, Status::OK, "rsqrt(1E+{}) status {st:?}", 2 * k);
        assert!(!r.is_subnormal() && r.is_finite());
    }
    for (input, want) in [("1E-398", "1E+199"), ("1E+384", "1E-192")] {
        for rm in ALL {
            assert_exact(
                parse(input).rsqrt(rm),
                parse(want),
                &format!("rsqrt({input}) [{rm:?}]"),
            );
        }
    }
}

/// Both exact branches, exhaustively at this width: `5^d` while
/// `5^d ≤ 10^16` (`d ≤ 22`), `2^d` while the input `5^(2d)` fits
/// (`d ≤ 11`). Where `r²` is itself representable the format's own
/// multiplication closes the loop exactly.
#[test]
fn power_families_exhaustive_at_this_width() {
    let one = parse("1");
    for d in 0..=22u32 {
        let input = 1u128 << (2 * d);
        let coef = 5u128.pow(d);
        let x = parse(&format!("{input}"));
        let want = parse(&format!("{coef}E-{d}"));
        for rm in ALL {
            assert_exact(x.rsqrt(rm), want, &format!("rsqrt(2^{}) [{rm:?}]", 2 * d));
        }
        if 2 * d <= 22 {
            assert_exact_round_trip(x, want, one, &format!("2^{}", 2 * d));
        }
    }
    for d in 0..=11u32 {
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

/// `x · r² = 1` through the format's own multiplication, exactly.
fn assert_exact_round_trip(x: Decimal64, r: Decimal64, one: Decimal64, label: &str) {
    let (sq, st_sq) = r.mul(r, NE);
    assert!(!st_sq.inexact(), "rsqrt({label})² is not exact: {st_sq:?}");
    let (round_trip, st) = x.mul(sq, NE);
    assert!(
        eq(round_trip, one) && !st.inexact(),
        "{label} · rsqrt({label})² = {round_trip} ({st:?}), want exactly 1"
    );
}

/// Both tie rows, literal, every mode.
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
            assert!(
                eq(r, parse_rm(mid, rm)),
                "rsqrt(2^{}) [{rm:?}] disagrees with parse_str on the midpoint",
                2 * d
            );
        }
        let mid_i = 5u128.pow(d);
        assert_eq!(
            mid_i.to_string(),
            digits_of(mid),
            "the tie literal is 5^{d}"
        );
        assert_eq!(mid_i % 10, 5, "a midpoint's last digit is 5");
        assert_eq!(
            mid_i.to_string().len(),
            TIE_WIDTH,
            "5^{d} spans PRECISION + 1 digits"
        );
        let down_i: u128 = digits_of(down_s).parse().expect("neighbour is a u128");
        let up_i: u128 = digits_of(up_s).parse().expect("neighbour is a u128");
        assert_eq!(
            mid_i - down_i * 10,
            up_i * 10 - mid_i,
            "the {d} row sits exactly halfway between the neighbours"
        );
        assert_eq!(
            (1u128 << (2 * d)).to_string(),
            input,
            "the tie input literal is 2^{}",
            2 * d
        );
    }
}

/// Quantum pins: column one is what the kernel delivers, column two is
/// §9.2.2's preferred exponent `−⌊Q(x)/2⌋`. The delta is recorded, not
/// repaired.
#[test]
fn quantum_pins_record_the_section_9_2_2_delta() {
    let rows: [(&str, i32, i32); 9] = [
        ("1", 0, 0),
        ("1E+72", -36, -36),
        ("4", -1, 0),
        ("0.04", 0, 1),
        ("6.25", -1, 1),
        ("100", -1, 0),
        ("16", -2, 0),
        ("1024", -5, 0),
        ("1E-100", 35, 50),
    ];
    for (label, delivered, preferred) in rows {
        let x = parse(label);
        let qx = quantum(x);
        assert_eq!(
            -qx.div_euclid(2),
            preferred,
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
    let (r, st) = parse("2").rsqrt(NE);
    assert!(st.inexact());
    assert_eq!(quantum(r), -16, "an inexact rsqrt fills the precision");
}

/// A deterministic sweep across the exponent range: `x · rsqrt(x)² ≈ 1`
/// within a coarse bound, results finite and normal, and `INEXACT` in
/// every direction (every coefficient carries a factor coprime to ten,
/// which the classifier's `s ≠ 1` bail rules out).
#[test]
fn sweep_round_trips_and_flags_inexact() {
    let coefficients: [&str; 6] = ["3", "7", "21", "1.5", "2.7", "123456789"];
    let tolerance = parse("1E-13");
    let one = parse("1");
    let mut checked = 0usize;
    for coef in coefficients {
        for exp in (-380i32..=380).step_by(7) {
            let x = parse(&format!("{coef}E{exp}"));
            if x.is_zero() || !x.is_finite() {
                continue;
            }
            let (r, st) = x.rsqrt(NE);
            assert!(
                r.is_finite() && !r.is_zero() && !r.is_subnormal(),
                "rsqrt({coef}E{exp}) = {r} left the normal range"
            );
            for rm in ALL {
                let (_, st) = x.rsqrt(rm);
                assert!(st.inexact(), "rsqrt({coef}E{exp}) [{rm:?}]: {st:?}");
            }
            assert!(st.inexact());
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
    assert!(checked > 500, "sweep covered only {checked} inputs");
}

/// One step off an exact family member is not exact.
#[test]
fn near_misses_are_declined() {
    for label in [
        "3", "9", "36", "0.09", "49", "2", "8", "32", "0.5", "0.125", "5", "125", "0.2", "10",
        "1000", "0.1", "1E-397",
    ] {
        let x = parse(label);
        for rm in ALL {
            let (_, st) = x.rsqrt(rm);
            assert!(st.inexact(), "rsqrt({label}) [{rm:?}] is not exact: {st:?}");
        }
    }
}
