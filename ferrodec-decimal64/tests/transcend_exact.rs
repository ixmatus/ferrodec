//! Exact-result gate for `exp2`, `log2`, and `log10` at `Decimal64`
//! (fd-aqs.8) — the sibling mirror of the root crate's
//! `tests/transcend_exact.rs`, pinning the 16-digit representability
//! boundaries (`2^53` and `5^22` are exactly 16 digits).

#![cfg(all(feature = "exp-log", feature = "pow"))]

use ferrodec_decimal64::{Decimal64, RoundingMode, Status};

const NE: RoundingMode = RoundingMode::NearestEven;
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
