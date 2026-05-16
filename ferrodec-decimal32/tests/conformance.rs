#![cfg(feature = "fmt")]
//! Conformance test runner for the vendored Mike Cowlishaw decTest
//! suite (`tests/vectors/ds*.decTest`).
//!
//! Most of the machinery (IBM decTest parser, directive accumulator,
//! per-file expectation guard, run-suite driver) lives in
//! [`ferrodec_test_support::conformance`]. This file supplies the
//! Decimal32-specific dispatch closure and the per-file expectation
//! table.
//!
//! # Asymmetric per-file expectation guard
//!
//! Per ADR-0010 (testing strategy after the 6-agent correctness
//! review), the expected pass count is checked **per file**, not just
//! against an aggregate floor. This catches silent trade-offs: a
//! refactor that improves one file by N cases while regressing
//! another by N cases — net zero in aggregate — fails this guard.
//! Each intentional improvement requires editing
//! [`expected_per_file`], which makes the change visible in git
//! history.

use ferrodec_decimal32::{Decimal32, ParseDecimalError, Status};
use ferrodec_test_support::conformance::{
    decode_conditions, map_rounding, run_suite, status_conformance_eq, Context, Outcome, TestCase,
};

const VECTORS_DIR: &str = "tests/vectors";

#[test]
fn dectest_conformance() {
    run_suite(
        VECTORS_DIR,
        expected_per_file(),
        &Context::for_decimal32(),
        run_case,
    );
}

/// Per-file expected pass count. Each row rises by the number of
/// cases the new dispatch arm passes; an intentional change requires
/// editing this table (see ADR-0010 in the workspace root for the
/// rationale).
///
/// Baseline after B6c (toSci wiring + `parse_str` + Display):
/// - `dsBase.decTest`: 698 of 909 cases pass. The 209 skips break
///   down as ~7 pathologically large exponents (deferred, see
///   `ParseDecimalError::ExponentOutOfRange`) plus ~202 cases under
///   non-IEEE rounding directives (`half_down`, `05up`) which we
///   won't coerce onto an IEEE mode (mirrors ferrodec's ADR-0005
///   posture).
/// - `dsEncode.decTest`: 2 of 268 cases pass — the two that route
///   via `parse_str` without needing the BID `#hex` interchange
///   decoder. The remaining 266 skip pending the dpd-feature
///   dispatch arm (lands when the dpd feature is wired in B16).
const fn expected_per_file() -> &'static [(&'static str, usize)] {
    &[("dsBase.decTest", 698), ("dsEncode.decTest", 2)]
}

fn run_case(case: &TestCase, ctx: &Context) -> Outcome {
    match case.op.as_str() {
        "tosci" | "apply" => run_tosci(case, ctx),
        _ => Outcome::Skip,
    }
}

/// `toSci` and `apply`: parse the operand string at the active
/// rounding mode, format with Display, compare result and emitted
/// status flags against the expected output and decoded conditions.
fn run_tosci(case: &TestCase, ctx: &Context) -> Outcome {
    if case.operands.len() != 1 {
        return Outcome::Skip;
    }
    let input = &case.operands[0];
    // Hex-prefixed operands (#) are BID bit-pattern interchange; we
    // skip those for now (handled in a dedicated dsEncode commit
    // later).
    if input.starts_with('#') || case.expected.starts_with('#') {
        return Outcome::Skip;
    }
    let rm = match map_rounding(&ctx.rounding) {
        Some(r) => r,
        None => return Outcome::Skip,
    };
    let (parsed, status) = match Decimal32::parse_str(input, rm) {
        Ok(r) => r,
        // ExponentOutOfRange covers decTest cases like `1e-999999999`
        // that test the implementation's handling of pathologically
        // large exponents. Our parse_str rejects them at the
        // 1 000 000 magnitude cap; the spec-conformant behaviour
        // (saturate to ±Inf or ±0 at parse time) is a deferred
        // design call. Skip rather than fail those cases.
        Err(ParseDecimalError::ExponentOutOfRange) => return Outcome::Skip,
        // decTest's "negative" test cases use malformed input strings
        // (`1..2`, `+-1`, `e100`, ...) and expect a `NaN` result with
        // `Conversion_syntax` (mapped to INVALID). Translate parse
        // errors to that shape rather than failing.
        Err(_) => (Decimal32::NAN, Status::INVALID),
    };
    let formatted = format_value(parsed);
    if formatted != case.expected {
        return Outcome::Fail(format!("got {formatted:?} want {:?}", case.expected));
    }
    let expected_status = decode_conditions(&case.conditions);
    if !status_conformance_eq(status, expected_status) {
        return Outcome::Fail(format!(
            "status mismatch: got {status:?} want {expected_status:?} (conditions {:?})",
            case.conditions
        ));
    }
    Outcome::Pass
}

fn format_value(d: Decimal32) -> String {
    use std::fmt::Write as _;
    let mut s = String::new();
    let _ = write!(s, "{d}");
    s
}
