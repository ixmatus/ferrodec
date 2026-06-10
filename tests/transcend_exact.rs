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
