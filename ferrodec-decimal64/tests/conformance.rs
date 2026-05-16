#![cfg(feature = "fmt")]
//! Conformance test runner for the vendored Mike Cowlishaw decTest
//! suite (`tests/vectors/dd*.decTest`).
//!
//! Most of the machinery (IBM decTest parser, directive accumulator,
//! per-file expectation guard, run-suite driver) lives in
//! [`ferrodec_test_support::conformance`]. This file supplies the
//! Decimal64-specific dispatch closure and the per-file expectation
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

use ferrodec_decimal64::{Decimal64, ParseDecimalError, Status};
use ferrodec_test_support::conformance::{
    decode_conditions, map_rounding, run_suite, status_conformance_eq, Context, Outcome, TestCase,
};

const VECTORS_DIR: &str = "tests/vectors";

#[test]
fn dectest_conformance() {
    run_suite(
        VECTORS_DIR,
        expected_per_file(),
        &Context::for_decimal64(),
        run_case,
    );
}

/// Per-file expected pass count. Each row rises by the number of
/// cases the new dispatch arm passes; an intentional change requires
/// editing this table (see ADR-0010 in the workspace root).
///
/// Baseline after C7 + C8 (toSci wiring + `parse_str` + Display +
/// IEEE 754 exponent clamping): the harness dispatches `tosci` and
/// `apply`. Files with non-zero passes:
/// * `ddBase.decTest`: 708 of 945 pass. Skips are extreme exponents
///   (deferred), non-IEEE rounding directives, and a few
///   format-precision conditional cases.
/// * `ddAdd.decTest`: 2 of 1091 pass — both are toSci-only edge
///   cases that route via parse / format without exercising add.
/// * `ddFMA.decTest`: 2 of 1378 pass — same shape.
///
/// All other files are 0 pending their dispatch arms in C9+.
const fn expected_per_file() -> &'static [(&'static str, usize)] {
    &[
        // F1: `add` + `subtract` dispatch wired. `add` rises from 2
        // (toSci-only edges) to 973 of 1091 after the fd-d47
        // dynamic-alignment correctness fix in ops/addsub.rs cleared
        // the `ddadd64xx` / `ddadd713xx` boundary round-half-even
        // family; the remaining ~118 skips are unrepresentable
        // operands (extreme exponents past the parser cap, `#`-hex
        // BID interchange), never failures. `subtract` is 514 of 516
        // (2 `#`-hex skips). Exact-match per-file counts per ADR-0010
        // / feedback_regression_guard_exact_match.
        ("ddAdd.decTest", 973),
        ("ddBase.decTest", 708),
        // F2: `multiply` / `divide` wired. No correctness bug
        // surfaced (the H3 typed-BiasedExp work already made them
        // conformant): `ddMultiply.decTest` 444 of 446 (2 `#`-hex
        // skips), `ddDivide.decTest` 702 of 717 (15 skips, extreme
        // exponents / `#`-hex). `ddDivideInt.decTest` is a distinct
        // operation, not wired here.
        ("ddDivide.decTest", 702),
        ("ddEncode.decTest", 0),
        // F3: `fma` wired. Rises 2 → 1318 of 1378 after the fd-d47
        // FMA-side fix in `h2_borrow_and_extend` (the
        // `ddfma364xx` power-of-ten borrow-extend collapse, the FMA
        // analogue of the addsub boundary family). The H3 case
        // `ddfma2504` is among the passers. 60 skips are
        // unrepresentable operands / `#`-hex.
        ("ddFMA.decTest", 1318),
        ("ddMultiply.decTest", 444),
        ("ddSubtract.decTest", 514),
    ]
}

fn run_case(case: &TestCase, ctx: &Context) -> Outcome {
    match case.op.as_str() {
        "tosci" | "apply" => run_tosci(case, ctx),
        "add" | "subtract" | "multiply" | "divide" => run_binary(case, ctx),
        "fma" => run_ternary(case, ctx),
        _ => Outcome::Skip,
    }
}

/// `fma`: parse all three operands, run `a.fma(b, c)`, compare the
/// formatted result and conformance-masked status. Same
/// skip-not-fail policy as `run_binary`.
fn run_ternary(case: &TestCase, ctx: &Context) -> Outcome {
    if case.operands.len() != 3 || case.expected.starts_with('#') {
        return Outcome::Skip;
    }
    let rm = match map_rounding(&ctx.rounding) {
        Some(r) => r,
        None => return Outcome::Skip,
    };
    let (a, b, c) = match (
        parse_operand(&case.operands[0], rm),
        parse_operand(&case.operands[1], rm),
        parse_operand(&case.operands[2], rm),
    ) {
        (Some(a), Some(b), Some(c)) => (a, b, c),
        _ => return Outcome::Skip,
    };
    let (result, status) = a.fma(b, c, rm);
    let formatted = format_value(result);
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

/// Parse one decTest operand at the active rounding mode. Returns
/// `None` (→ the caller should `Skip`) for operands this port cannot
/// represent (hex BID interchange, exponents past the parser cap);
/// a syntactically invalid operand becomes `(NaN, INVALID)` so the
/// decTest "negative" cases still exercise the op.
fn parse_operand(s: &str, rm: ferrodec_decimal64::RoundingMode) -> Option<Decimal64> {
    if s.starts_with('#') {
        return None;
    }
    match Decimal64::parse_str(s, rm) {
        Ok((v, _)) => Some(v),
        Err(ParseDecimalError::ExponentOutOfRange) => None,
        Err(_) => Some(Decimal64::NAN),
    }
}

/// `add` / `subtract`: parse both operands, run the op, compare the
/// formatted result and the conformance-masked status. A case the
/// port cannot attempt (unrepresentable operand, `#` result, hex
/// interchange) is skipped, never failed, so the suite's zero
/// failure ceiling stays meaningful.
fn run_binary(case: &TestCase, ctx: &Context) -> Outcome {
    if case.operands.len() != 2 || case.expected.starts_with('#') {
        return Outcome::Skip;
    }
    let rm = match map_rounding(&ctx.rounding) {
        Some(r) => r,
        None => return Outcome::Skip,
    };
    let (a, b) = match (
        parse_operand(&case.operands[0], rm),
        parse_operand(&case.operands[1], rm),
    ) {
        (Some(a), Some(b)) => (a, b),
        _ => return Outcome::Skip,
    };
    let (result, status) = match case.op.as_str() {
        "add" => a.add(b, rm),
        "subtract" => a.sub(b, rm),
        "multiply" => a.mul(b, rm),
        "divide" => a.div(b, rm),
        _ => unreachable!("run_binary only dispatches add / subtract / multiply / divide"),
    };
    let formatted = format_value(result);
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

/// `toSci` and `apply`: parse the operand string at the active
/// rounding mode, format with Display, compare result and emitted
/// status flags against the expected output and decoded conditions.
fn run_tosci(case: &TestCase, ctx: &Context) -> Outcome {
    if case.operands.len() != 1 {
        return Outcome::Skip;
    }
    let input = &case.operands[0];
    // Hex-prefixed operands (#) are BID bit-pattern interchange; we
    // skip those for now (handled in a dedicated ddEncode commit
    // later).
    if input.starts_with('#') || case.expected.starts_with('#') {
        return Outcome::Skip;
    }
    let rm = match map_rounding(&ctx.rounding) {
        Some(r) => r,
        None => return Outcome::Skip,
    };
    let (parsed, status) = match Decimal64::parse_str(input, rm) {
        Ok(r) => r,
        // ExponentOutOfRange covers decTest cases like
        // `1e-999999999` that test the implementation's handling of
        // pathologically large exponents. Our parse_str rejects them
        // at the 1 000 000 magnitude cap; the spec-conformant
        // behaviour (saturate to ±Inf or ±0 at parse time) is a
        // deferred design call. Skip rather than fail those cases.
        Err(ParseDecimalError::ExponentOutOfRange) => return Outcome::Skip,
        // decTest's "negative" test cases use malformed input strings
        // (`1..2`, `+-1`, `e100`, ...) and expect a `NaN` result with
        // `Conversion_syntax` (mapped to INVALID). Translate parse
        // errors to that shape rather than failing.
        Err(_) => (Decimal64::NAN, Status::INVALID),
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

fn format_value(d: Decimal64) -> String {
    use std::fmt::Write as _;
    let mut s = String::new();
    let _ = write!(s, "{d}");
    s
}
