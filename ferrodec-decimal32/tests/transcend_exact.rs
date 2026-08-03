//! Exact-result gate for `exp2`, `log2`, and `log10` at `Decimal32`
//! (fd-aqs.8) — the sibling mirror of the root crate's
//! `tests/transcend_exact.rs`, pinning the 7-digit representability
//! boundaries (`2^23` and `5^10` are exactly 7 digits). `exp2(3)` at
//! `TowardNegative` is the 2026-06-09 review's witness (`7.999999`
//! before the fix). `exp2(-11)` is the format's one nearest-mode tie
//! (`5^11 = 48828125`, exactly 8 digits ending in 5, the ADR-0033
//! exhaustive sweep's exact-NE-tie case); since ADR-0059 M7 the
//! input-side classifier delivers its exact coefficient through the
//! format rounder, which keeps the pinned `NearestEven` value and
//! repairs the `NearestAway` side the kernel misrounded.

#![cfg(all(feature = "exp-log", feature = "pow"))]

use ferrodec_decimal32::{Decimal32, RoundingMode, Status};

const NE: RoundingMode = RoundingMode::NearestEven;
const NA: RoundingMode = RoundingMode::NearestAway;
const TZ: RoundingMode = RoundingMode::TowardZero;
const TP: RoundingMode = RoundingMode::TowardPositive;
const TN: RoundingMode = RoundingMode::TowardNegative;

fn parse(s: &str) -> Decimal32 {
    Decimal32::parse_str(s, NE).unwrap().0
}

fn assert_exact(got: (Decimal32, Status), want: &str, label: &str) {
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
    assert_exact(parse("23").exp2(NE), "8388608", "exp2(23)");
    assert_exact(parse("-10").exp2(NE), "9.765625E-4", "exp2(-10)");
    let (_, st) = parse("24").exp2(NE);
    assert!(st.inexact(), "exp2(24) is inexact at 7 digits: {st:?}");
    let (r, st) = parse("-11").exp2(NE);
    assert!(st.inexact(), "exp2(-11) is inexact at 7 digits: {st:?}");
    assert_eq!(
        r.partial_cmp(parse("4.882812E-4")).0,
        Some(core::cmp::Ordering::Equal),
        "exp2(-11) NE tie resolves to the even significand"
    );

    // The full tie table for exp2(-11) = 5^11 · 10^-11 (ADR-0059 M7):
    // 7-digit neighbours 4882812 (even) and 4882813. NearestAway
    // misrounded to the even (lower) neighbour before M7.
    for (rm, want) in [
        (NA, "4.882813E-4"),
        (TZ, "4.882812E-4"),
        (TP, "4.882813E-4"),
        (TN, "4.882812E-4"),
    ] {
        let (r, st) = parse("-11").exp2(rm);
        assert_eq!(
            r.partial_cmp(parse(want)).0,
            Some(core::cmp::Ordering::Equal),
            "exp2(-11) {rm:?}: got {r:?}, want {want}"
        );
        assert!(
            st.inexact(),
            "exp2(-11) {rm:?}: expected INEXACT, got {st:?}"
        );
    }

    assert_exact(parse("1024").log2(TN), "10", "log2(1024) TowardNegative");
    assert_exact(parse("8388608").log2(NE), "23", "log2(2^23)");
    assert_exact(parse("1E+90").log10(TN), "90", "log10(1E90)");
    assert_exact(parse("1E-90").log10(NE), "-90", "log10(1E-90)");
    let (_, st) = parse("3").log2(NE);
    assert!(st.inexact(), "log2(3) is irrational: {st:?}");
}

/// Input-side cbrt exactness (ADR-0059 M7): a perfect cube returns its
/// exact root at every rounding direction with status OK. Before M7,
/// `cbrt(0.027)` at `TowardZero` / `TowardNegative` shipped `0.2999999`
/// with a spurious `INEXACT` (the post-hoc proof could not fire on a
/// misrounded kernel result — it was circular).
#[test]
fn cbrt_exact_cubes_every_mode() {
    for rm in [NE, NA, TZ, TP, TN] {
        assert_exact(parse("8").cbrt(rm), "2", "cbrt(8)");
        assert_exact(parse("-0.027").cbrt(rm), "-0.3", "cbrt(-0.027)");
        assert_exact(parse("1E+30").cbrt(rm), "1E+10", "cbrt(1E+30)");
        // A 7-digit perfect cube: 4.913E+6 = 170³.
        assert_exact(parse("4913000").cbrt(rm), "170", "cbrt(4913000)");
    }
    let (_, st) = parse("9").cbrt(NE);
    assert!(st.inexact(), "cbrt(9) is irrational: {st:?}");
}

/// Input-side pow exactness and ties (ADR-0059 M7): exact rational
/// powers at every mode with status OK (`pow(4, 0.5)` shipped
/// `1.999999` + `INEXACT` at the directed-down modes before), and the
/// `pow(2, -11)` tie fixed at `NearestAway`.
#[test]
fn pow_exact_and_ties() {
    for rm in [NE, NA, TZ, TP, TN] {
        assert_exact(parse("4").pow(parse("0.5"), rm), "2", "pow(4, 0.5)");
        assert_exact(parse("2.25").pow(parse("0.5"), rm), "1.5", "pow(2.25, 0.5)");
        assert_exact(parse("10").pow(parse("90"), rm), "1E+90", "pow(10, 90)");
    }
    let (r, st) = parse("2").pow(parse("-11"), NA);
    assert_eq!(
        r.partial_cmp(parse("4.882813E-4")).0,
        Some(core::cmp::Ordering::Equal),
        "pow(2, -11) NA resolves the tie away from zero: got {r:?}"
    );
    assert!(st.inexact(), "pow(2, -11) NA: expected INEXACT, got {st:?}");
    let (_, st) = parse("7").pow(parse("0.5"), NE);
    assert!(st.inexact(), "pow(7, 0.5) is irrational: {st:?}");
}
