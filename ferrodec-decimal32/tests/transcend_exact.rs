//! Exact-result gate for `exp2`, `log2`, and `log10` at `Decimal32`
//! (fd-aqs.8) — the sibling mirror of the root crate's
//! `tests/transcend_exact.rs`, pinning the 7-digit representability
//! boundaries (`2^23` and `5^10` are exactly 7 digits). `exp2(3)` at
//! `TowardNegative` is the 2026-06-09 review's witness (`7.999999`
//! before the fix). `exp2(-11)` stays on the kernel path: `5^11` is
//! 8 digits, so the result is inexact at this precision — that is
//! the ADR-0033 exhaustive sweep's exact-NE-tie case, unchanged.

#![cfg(all(feature = "exp-log", feature = "pow"))]

use ferrodec_decimal32::{Decimal32, RoundingMode, Status};

const NE: RoundingMode = RoundingMode::NearestEven;
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

    assert_exact(parse("1024").log2(TN), "10", "log2(1024) TowardNegative");
    assert_exact(parse("8388608").log2(NE), "23", "log2(2^23)");
    assert_exact(parse("1E+90").log10(TN), "90", "log10(1E90)");
    assert_exact(parse("1E-90").log10(NE), "-90", "log10(1E-90)");
    let (_, st) = parse("3").log2(NE);
    assert!(st.inexact(), "log2(3) is irrational: {st:?}");
}
