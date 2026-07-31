//! Exact-result gate for `exp2`, `log2`, and `log10` (fd-aqs.8).
//!
//! ADR-0047 proved exactness post-hoc for `cbrt` and `pow`; its
//! Context claimed the rest of the surface irrational for every
//! non-special input, which is false for these three: `exp2(n)` is
//! exact whenever `2^n` is representable (`2^n` for `n ≥ 0`,
//! `5^{-n} · 10^n` below), `log10(10^k) = k`, and `log2(2^k) = k`.
//! The 2026-06-09 review found two consequences: a spurious INEXACT
//! on every such case (IEEE 754-2019 §7.5 forbids it), and directed
//! mode misrounds where the kernel's 50-digit approximation landed
//! on the wrong side of the exact value (`exp2(3)` at
//! `TowardNegative` returned `7.999999…`; `log2(1024)` returned
//! `9.999…9`). Pre-detection in `ferrodec-transcend::exact` repairs
//! both at once; this gate pins the exact cases, the representability
//! boundaries on each side, and the `exp2`/`pow` flag parity that the
//! ADR-0025 tautology audit could not keep as a value identity.

#![cfg(all(feature = "exp-log", feature = "pow"))]

use ferrodec::{Decimal128, RoundingMode, Status};

const NE: RoundingMode = RoundingMode::NearestEven;
const NA: RoundingMode = RoundingMode::NearestAway;
const TZ: RoundingMode = RoundingMode::TowardZero;
const TP: RoundingMode = RoundingMode::TowardPositive;
const TN: RoundingMode = RoundingMode::TowardNegative;
const ALL: [RoundingMode; 5] = [NE, NA, TZ, TP, TN];

fn parse(s: &str) -> Decimal128 {
    Decimal128::parse_str(s, NE).unwrap().0
}

fn assert_exact(got: (Decimal128, Status), want: &str, label: &str) {
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
fn exp2_exact_integers_every_mode() {
    for rm in ALL {
        assert_exact(parse("3").exp2(rm), "8", "exp2(3)");
        assert_exact(parse("10").exp2(rm), "1024", "exp2(10)");
        assert_exact(parse("-2").exp2(rm), "0.25", "exp2(-2)");
    }
    // The representability boundaries: 2^112 and 5^48 are exactly 34
    // digits; one step past each is inexact and stays on the kernel.
    assert_exact(
        parse("112").exp2(NE),
        "5192296858534827628530496329220096",
        "exp2(112)",
    );
    assert_exact(
        parse("-48").exp2(NE),
        "3.552713678800500929355621337890625E-15",
        "exp2(-48)",
    );
    let (_, st) = parse("113").exp2(NE);
    assert!(st.inexact(), "exp2(113) is inexact at 34 digits: {st:?}");
    let (_, st) = parse("-49").exp2(NE);
    assert!(st.inexact(), "exp2(-49) is inexact at 34 digits: {st:?}");
    // Non-integer control.
    let (_, st) = parse("2.5").exp2(NE);
    assert!(st.inexact(), "exp2(2.5) is irrational: {st:?}");
}

/// Pin an inexact result: value equality plus the INEXACT flag.
fn assert_rounded(got: (Decimal128, Status), want: &str, label: &str) {
    let (r, st) = got;
    let want_d = parse(want);
    assert_eq!(
        r.partial_cmp(want_d).0,
        Some(core::cmp::Ordering::Equal),
        "{label}: got {r:?}, want {want}"
    );
    assert!(st.inexact(), "{label}: expected INEXACT, got {st:?}");
}

/// The named `Decimal128` nearest-mode ties (ADR-0059 M7): `5^49` and
/// `5^50` have exactly 35 significant digits with final digit 5, so
/// `exp2(-49)` and `exp2(-50)` sit exactly on a midpoint of adjacent
/// representable values. The input-side classifier delivers the exact
/// 35-digit coefficient through the format rounder, whose own tie rule
/// decides each mode. Before M7 the approximation kernel's error chose
/// an arbitrary side: `exp2(-49)` misrounded at `NearestAway` (`…312`
/// for `…313`) and `exp2(-50)` at `NearestEven` (`…563` for the even
/// `…562`).
#[test]
fn exp2_ties_at_precision_plus_one() {
    // exp2(-49) = 5^49 · 10^-49; 5^49 = 17763568394002504646778106689453125.
    // 34-digit neighbours: …945312 (even) and …945313.
    for (rm, want) in [
        (NE, "1.776356839400250464677810668945312E-15"),
        (NA, "1.776356839400250464677810668945313E-15"),
        (TZ, "1.776356839400250464677810668945312E-15"),
        (TP, "1.776356839400250464677810668945313E-15"),
        (TN, "1.776356839400250464677810668945312E-15"),
    ] {
        assert_rounded(parse("-49").exp2(rm), want, &format!("exp2(-49) {rm:?}"));
    }
    // exp2(-50) = 5^50 · 10^-50; 5^50 = 88817841970012523233890533447265625.
    // 34-digit neighbours: …726562 (even) and …726563.
    for (rm, want) in [
        (NE, "8.881784197001252323389053344726562E-16"),
        (NA, "8.881784197001252323389053344726563E-16"),
        (TZ, "8.881784197001252323389053344726562E-16"),
        (TP, "8.881784197001252323389053344726563E-16"),
        (TN, "8.881784197001252323389053344726562E-16"),
    ] {
        assert_rounded(parse("-50").exp2(rm), want, &format!("exp2(-50) {rm:?}"));
    }
}

/// Input-side cbrt exactness (ADR-0059 M7): a perfect cube returns its
/// exact root at every rounding direction with status OK. Before M7
/// the exactness proof was post-hoc from the rounded result and
/// circular: at `TowardZero` / `TowardNegative` the kernel's 50-digit
/// approximation of `cbrt(0.027)` landed below `0.3`, the directed
/// round truncated to `0.2999…9`, the cube-back check saw a non-cube,
/// and the wrong value shipped with a spurious `INEXACT`.
#[test]
fn cbrt_exact_cubes_every_mode() {
    for rm in ALL {
        assert_exact(parse("8").cbrt(rm), "2", "cbrt(8)");
        assert_exact(parse("-8").cbrt(rm), "-2", "cbrt(-8)");
        assert_exact(parse("0.027").cbrt(rm), "0.3", "cbrt(0.027)");
        assert_exact(parse("-0.027").cbrt(rm), "-0.3", "cbrt(-0.027)");
        assert_exact(parse("1E+300").cbrt(rm), "1E+100", "cbrt(1E+300)");
        assert_exact(parse("1E-6174").cbrt(rm), "1E-2058", "cbrt(1E-6174)");
        // A 34-digit perfect cube: 9.261E+33 = (2.1E+11)³.
        assert_exact(parse("9.261E+33").cbrt(rm), "2.1E+11", "cbrt(9.261E+33)");
        // Cohort-insensitivity: 0.027000 is the same value at another
        // quantum and must take the same exact path.
        assert_exact(parse("0.027000").cbrt(rm), "0.3", "cbrt(0.027000)");
    }
    // Non-cube controls stay inexact on the kernel: a cube coefficient
    // at an exponent not divisible by 3, and a non-cube coefficient.
    let (_, st) = parse("0.27").cbrt(NE);
    assert!(st.inexact(), "cbrt(0.27) is irrational: {st:?}");
    let (_, st) = parse("9").cbrt(NE);
    assert!(st.inexact(), "cbrt(9) is irrational: {st:?}");
    // The exact path delivers the input-derived natural cohort. The
    // post-hoc era's cohort was kernel noise: 0.3000000000000000…0
    // (34 digits) here, but bare 2 for cbrt(8), depending on whether
    // the 50-digit kernel happened to land exactly.
    let (r, _) = parse("0.027").cbrt(NE);
    assert_eq!(format!("{r}"), "0.3", "cbrt(0.027) cohort");
}

/// The non-tie `PRECISION + 1` cases route through the same classifier
/// and must stay byte-identical to the (already correct) kernel: a
/// 35-digit `2^n` whose final digit is not 5 has both directed sides
/// and the nearest decision determined by that digit alone.
#[test]
fn exp2_p_plus_one_non_ties_unchanged() {
    // 2^113 = 10384593717069655257060992658440192 (35 digits, final 2).
    assert_rounded(
        parse("113").exp2(NE),
        "1.038459371706965525706099265844019E+34",
        "exp2(113) NE",
    );
    assert_rounded(
        parse("113").exp2(TP),
        "1.038459371706965525706099265844020E+34",
        "exp2(113) TP",
    );
    // 2^116 = 83076749736557242056487941267521536 (35 digits, final 6).
    assert_rounded(
        parse("116").exp2(NE),
        "8.307674973655724205648794126752154E+34",
        "exp2(116) NE",
    );
    assert_rounded(
        parse("116").exp2(TZ),
        "8.307674973655724205648794126752153E+34",
        "exp2(116) TZ",
    );
    // One past the gate on each side (36 digits): stays on the kernel,
    // still correct there — 5^51 ends in …28125, far from a midpoint.
    assert_rounded(
        parse("117").exp2(NE),
        "1.661534994731144841129758825350431E+35",
        "exp2(117) NE",
    );
    assert_rounded(
        parse("-51").exp2(NE),
        "4.440892098500626161694526672363281E-16",
        "exp2(-51) NE",
    );
}

#[test]
fn log2_exact_powers_every_mode() {
    for rm in ALL {
        assert_exact(parse("1024").log2(rm), "10", "log2(1024)");
        assert_exact(parse("0.25").log2(rm), "-2", "log2(0.25)");
    }
    assert_exact(
        parse("5192296858534827628530496329220096").log2(NE),
        "112",
        "log2(2^112)",
    );
    assert_exact(parse("9.765625E-4").log2(NE), "-10", "log2(2^-10)");
    let (_, st) = parse("3").log2(NE);
    assert!(st.inexact(), "log2(3) is irrational: {st:?}");
}

#[test]
fn log10_exact_powers_every_mode() {
    for rm in ALL {
        assert_exact(parse("1000").log10(rm), "3", "log10(1000)");
        assert_exact(parse("1E+96").log10(rm), "96", "log10(1E96)");
        assert_exact(parse("1E-95").log10(rm), "-95", "log10(1E-95)");
        // Cohort-insensitivity: 100.0 is the same value at another
        // quantum and must take the same exact path.
        assert_exact(parse("100.0").log10(rm), "2", "log10(100.0)");
    }
    let (_, st) = parse("2").log10(NE);
    assert!(st.inexact(), "log10(2) is irrational: {st:?}");
}

#[test]
fn exp2_pow_flag_parity() {
    // ADR-0047 cleared pow(2, 10); exp2(10) raised INEXACT for the
    // same value until fd-aqs.8 — an internal inconsistency the
    // metamorphic suite could not see (exp2 == pow(2, x) is value
    // tautological and was dropped by ADR-0025, but it is flag
    // discriminating).
    let (e, st_e) = parse("10").exp2(NE);
    let (p, st_p) = parse("2").pow(parse("10"), NE);
    assert_eq!(e.partial_cmp(p).0, Some(core::cmp::Ordering::Equal));
    assert_eq!(st_e, Status::OK);
    assert_eq!(st_p, Status::OK);
}
