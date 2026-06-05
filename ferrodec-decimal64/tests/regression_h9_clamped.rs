#![cfg(all(feature = "fmt", feature = "binary-float"))]
//! H9 / fd-61r regression: `Status::CLAMPED` (IEEE 754-2019 §7.4,
//! informational) is raised at every in-operation clamp site the BID
//! cohort model can detect. The original H9 sites: round.rs §6.3 pad +
//! zero-exponent clamp (Agent 1 F7), div finite/Inf -> +-0 at Etiny
//! (Agent 2 M6). fd-61r (ADR-0048) extended this to zero results whose
//! ideal exponent fell outside the quantum range, and unmasked CLAMPED in
//! the conformance runner so it is compared there too; these direct unit
//! checks remain as a focused guard.

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

// NOTE on the BID-structural residual (fd-61r / ADR-0048): one small
// class of Clamped cases cannot be raised in the BID cohort model. When
// an operand's own exponent exceeds the format quantum range it is
// normalised into a padded cohort at parse (`1E+384` is stored at qmax),
// losing the pre-clamp exponent decNumber keeps in a wide working
// exponent. The downstream operation then has no signal that the result
// was clamped (ddadd380 `1E+384 + 1E+384`, ddrem424 `1E+384 % 3E+383`).
// The conformance runner detects these by re-parsing operands and skips
// them, tallied as the structural-CLAMPED category in KNOWN_ISSUES.md.
// The zero-result cases the old note listed here (dddiv497
// `0E+380 / 1000E-13`) are now raised, exercised below.

#[test]
fn zero_result_with_out_of_range_ideal_is_clamped() {
    // fd-61r: a zero whose ideal quantum falls outside [Qmin, Qmax] is
    // delivered at the boundary and raises Clamped. dddiv497:
    // 0E+380 / 1000E-13 -> 0E+369 Clamped (ideal 393 > Qmax 369).
    let (r, s) = p("0E+380").div(p("1000E-13"), RM);
    assert!(r.is_zero());
    assert!(s.clamped(), "zero with ideal exponent above Qmax clamps");
    // A product that underflows to zero, ideal below Etiny (ddmul755 shape).
    let (r, s) = p("1e-277").mul(p("1e-311"), RM);
    assert!(r.is_zero());
    assert!(s.clamped() && s.underflow());
}

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
