//! Exact-result gate for `exp2`, `log2`, and `log10` at `Decimal64`
//! (fd-aqs.8) — the sibling mirror of the root crate's
//! `tests/transcend_exact.rs`, pinning the 16-digit representability
//! boundaries (`2^53` and `5^22` are exactly 16 digits).

#![cfg(all(feature = "exp-log", feature = "pow"))]

use ferrodec_decimal64::{Decimal64, RoundingMode, Status};

const NE: RoundingMode = RoundingMode::NearestEven;
const NA: RoundingMode = RoundingMode::NearestAway;
const TZ: RoundingMode = RoundingMode::TowardZero;
const TP: RoundingMode = RoundingMode::TowardPositive;
const TN: RoundingMode = RoundingMode::TowardNegative;

fn parse(s: &str) -> Decimal64 {
    Decimal64::parse_str(s, NE).unwrap().0
}

fn assert_exact(got: (Decimal64, Status), want: &str, label: &str) {
    let (r, st) = got;
    let want_d = parse(want);
    assert_eq!(
        r.partial_cmp(want_d).0,
        Some(core::cmp::Ordering::Equal),
        "{label}: got {r:?}, want {want}"
    );
    assert_eq!(
        st,
        Status::OK,
        "{label}: exact result must be OK, got {st:?}"
    );
}

#[test]
fn exact_cases_and_boundaries() {
    assert_exact(parse("3").exp2(TN), "8", "exp2(3) TowardNegative");
    assert_exact(parse("53").exp2(NE), "9007199254740992", "exp2(53)");
    assert_exact(parse("-22").exp2(NE), "2.384185791015625E-7", "exp2(-22)");
    let (_, st) = parse("54").exp2(NE);
    assert!(st.inexact(), "exp2(54) is inexact at 16 digits: {st:?}");
    let (_, st) = parse("-23").exp2(NE);
    assert!(st.inexact(), "exp2(-23) is inexact at 16 digits: {st:?}");

    assert_exact(parse("1024").log2(TN), "10", "log2(1024) TowardNegative");
    assert_exact(parse("9007199254740992").log2(NE), "53", "log2(2^53)");
    assert_exact(parse("1E+300").log10(TN), "300", "log10(1E300)");
    assert_exact(parse("1E-300").log10(NE), "-300", "log10(1E-300)");
    let (_, st) = parse("3").log2(NE);
    assert!(st.inexact(), "log2(3) is irrational: {st:?}");
}

/// Pin an inexact result: value equality plus the INEXACT flag.
fn assert_rounded(got: (Decimal64, Status), want: &str, label: &str) {
    let (r, st) = got;
    let want_d = parse(want);
    assert_eq!(
        r.partial_cmp(want_d).0,
        Some(core::cmp::Ordering::Equal),
        "{label}: got {r:?}, want {want}"
    );
    assert!(st.inexact(), "{label}: expected INEXACT, got {st:?}");
}

/// The named `Decimal64` nearest-mode ties (ADR-0059 M7): `5^23` and
/// `5^24` have exactly 17 significant digits with final digit 5, so
/// `exp2(-23)` and `exp2(-24)` sit exactly on a midpoint of adjacent
/// representable values. The input-side classifier delivers the exact
/// 17-digit coefficient through the format rounder. Before M7 both
/// misrounded at `NearestAway` (the kernel's error landed below the
/// midpoint, giving the toward-zero neighbour).
#[test]
fn exp2_ties_at_precision_plus_one() {
    // exp2(-23) = 5^23 · 10^-23; 5^23 = 11920928955078125.
    // 16-digit neighbours: …507812 (even) and …507813.
    for (rm, want) in [
        (NE, "1.192092895507812E-7"),
        (NA, "1.192092895507813E-7"),
        (TZ, "1.192092895507812E-7"),
        (TP, "1.192092895507813E-7"),
        (TN, "1.192092895507812E-7"),
    ] {
        assert_rounded(parse("-23").exp2(rm), want, &format!("exp2(-23) {rm:?}"));
    }
    // exp2(-24) = 5^24 · 10^-24; 5^24 = 59604644775390625.
    // 16-digit neighbours: …539062 (even) and …539063.
    for (rm, want) in [
        (NE, "5.960464477539062E-8"),
        (NA, "5.960464477539063E-8"),
        (TZ, "5.960464477539062E-8"),
        (TP, "5.960464477539063E-8"),
        (TN, "5.960464477539062E-8"),
    ] {
        assert_rounded(parse("-24").exp2(rm), want, &format!("exp2(-24) {rm:?}"));
    }
    // Non-tie PRECISION + 1 control (final digit 4): byte-identical to
    // the previously-correct kernel. 2^54 = 18014398509481984.
    assert_rounded(parse("54").exp2(NE), "1.801439850948198E+16", "exp2(54) NE");
    assert_rounded(parse("54").exp2(TP), "1.801439850948199E+16", "exp2(54) TP");
    // One past the gate (18 digits): stays on the kernel. 5^25 ends in
    // …953125, far from a midpoint.
    assert_rounded(
        parse("-25").exp2(NE),
        "2.980232238769531E-8",
        "exp2(-25) NE",
    );
}

/// Input-side cbrt exactness (ADR-0059 M7): a perfect cube returns its
/// exact root at every rounding direction with status OK. Before M7,
/// `cbrt(0.027)` at `TowardZero` / `TowardNegative` shipped `0.2999…9`
/// with a spurious `INEXACT` (the post-hoc proof could not fire on a
/// misrounded kernel result — it was circular).
#[test]
fn cbrt_exact_cubes_every_mode() {
    for rm in [NE, NA, TZ, TP, TN] {
        assert_exact(parse("8").cbrt(rm), "2", "cbrt(8)");
        assert_exact(parse("-0.027").cbrt(rm), "-0.3", "cbrt(-0.027)");
        assert_exact(parse("1E+300").cbrt(rm), "1E+100", "cbrt(1E+300)");
        // A 16-digit perfect cube: 3.375E+15 = (1.5E+5)³.
        assert_exact(parse("3.375E+15").cbrt(rm), "1.5E+5", "cbrt(3.375E+15)");
    }
    let (_, st) = parse("9").cbrt(NE);
    assert!(st.inexact(), "cbrt(9) is irrational: {st:?}");
}

/// Input-side pow exactness and ties (ADR-0059 M7): exact rational
/// powers at every mode with status OK (`pow(4, 0.5)` shipped
/// `1.999…9` + `INEXACT` at the directed-down modes before), and the
/// `pow(2, -23)` tie fixed at `NearestAway`.
#[test]
fn pow_exact_and_ties() {
    for rm in [NE, NA, TZ, TP, TN] {
        assert_exact(parse("4").pow(parse("0.5"), rm), "2", "pow(4, 0.5)");
        assert_exact(parse("2.25").pow(parse("0.5"), rm), "1.5", "pow(2.25, 0.5)");
        assert_exact(parse("10").pow(parse("300"), rm), "1E+300", "pow(10, 300)");
    }
    assert_rounded(
        parse("2").pow(parse("-23"), NA),
        "1.192092895507813E-7",
        "pow(2, -23) NA",
    );
    let (_, st) = parse("7").pow(parse("0.5"), NE);
    assert!(st.inexact(), "pow(7, 0.5) is irrational: {st:?}");
}
