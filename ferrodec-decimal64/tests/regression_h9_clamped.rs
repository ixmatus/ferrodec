#![cfg(all(feature = "fmt", feature = "binary-float"))]
//! H9 regression: `Status::CLAMPED` (IEEE 754-2019 §7.4,
//! informational) is raised at the in-operation clamp sites the
//! Phase 1 review named. Agent 1 F7 (round.rs §6.3 pad + zero-exponent
//! clamp), Agent 2 M6 (div finite/Inf -> +-0 at Etiny). The flag is
//! informational: the conformance harness masks it
//! (`status_conformance_eq`), so per-file pass counts are unaffected;
//! these direct unit checks are the regression guard.

use ferrodec_decimal64::{Decimal64, RoundingMode};

const RM: RoundingMode = RoundingMode::NearestEven;

fn p(s: &str) -> Decimal64 {
    Decimal64::parse_str(s, RM).unwrap().0
}

#[test]
fn div_finite_by_infinity_is_clamped_zero() {
    // decTest dddiv788: -1000 / Inf -> -0E-398 Clamped.
    let (r, s) = p("-1000").div(Decimal64::INFINITY, RM);
    assert!(r.is_zero() && r.is_sign_negative());
    assert!(s.clamped(), "x / Inf clamps the exponent to Etiny");
}

#[test]
fn div_zero_by_infinity_is_clamped_zero() {
    let (r, s) = Decimal64::ZERO.div(Decimal64::INFINITY, RM);
    assert!(r.is_zero());
    assert!(s.clamped());
}

#[test]
fn mul_overflowing_preferred_exponent_pads_and_clamps() {
    // 1E+369 (in range, quantum 369) * 1E+5: the value 1E+374 is
    // representable but its minimal quantum 374 exceeds the format
    // max (369), so finalise_finite's §6.3 branch pads the
    // coefficient with trailing zeros and clamps the exponent.
    let (r, s) = p("1E+369").mul(p("1E+5"), RM);
    assert!(r.is_finite() && !r.is_zero());
    assert_eq!(
        r.partial_cmp(p("1E+374")).0,
        Some(core::cmp::Ordering::Equal),
        "value is exact; only the quantum was clamped"
    );
    assert!(s.clamped());
    assert!(!s.overflow(), "§6.3 pad is not an overflow");
}

// NOTE on residual scope: decTest also marks Clamped on cases whose
// operands are pre-normalised by `parse_str` (e.g. dddiv497
// `0E+380 / 1000E-13`, ddrem422/424). There the §7.4 condition is an
// artifact of GDA's extended-precision ideal-exponent bookkeeping,
// not an in-operation representational clamp in our BID cohort model;
// the value is exact and the conformance harness filters Clamped, so
// full ideal-exponent accounting is deferred (no value error, no
// conformance impact). See KNOWN_ISSUES.md.

#[test]
fn ordinary_arithmetic_is_not_clamped() {
    // Guard: the flag must not leak onto in-range results.
    let (_, s) = p("2").mul(p("3"), RM);
    assert!(!s.clamped());
    let (_, s) = p("1").div(p("4"), RM);
    assert!(!s.clamped());
    let (_, s) = p("10").div(Decimal64::INFINITY, RM);
    // (sanity: this IS clamped; kept distinct from the above)
    assert!(s.clamped());
}
