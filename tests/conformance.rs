//! Conformance test runner for the vendored Mike Cowlishaw decTest
//! suite (`tests/vectors/dq*.decTest`).
//!
//! Each `.decTest` file is parsed line-by-line. Directives
//! (`precision`, `rounding`, `maxExponent`, etc.) accumulate into a
//! mutable context. Test cases of the form
//!
//! ```text
//! id op operand1 [operand2 [operand3]] -> expected [conditions...]
//! ```
//!
//! are routed to the appropriate `Decimal128` method. The runner then
//! compares result and IEEE 754 status flags against the upstream
//! expectation.
//!
//! Cohort matters: decTest expects the *exact* result encoding (e.g.
//! `1.0` and `1.00` are distinct), and we hold ourselves to the same
//! bar — `result.to_bits() == expected.to_bits()` for finite results.
//!
//! Ops we don't yet implement (`comparesig`, `tointegral`, `quantize`,
//! ...) are counted as `skipped`, not failed. When the failures list
//! is non-empty the test panics with a per-file summary so triage can
//! start at a specific test ID.

use std::fs;
use std::path::{Path, PathBuf};

use ferrodec::{Decimal128, RoundingMode, Status};

const VECTORS_DIR: &str = "tests/vectors";

#[test]
fn dectest_conformance() {
    let mut totals = Totals::default();
    let mut failures: Vec<Failure> = Vec::new();

    let entries = fs::read_dir(VECTORS_DIR).expect("vectors directory");
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("decTest"))
        .collect();
    paths.sort();

    for path in paths {
        let result = run_file(&path, &mut failures);
        totals.merge(&result);
        eprintln!(
            "{:<28}  {:>5} pass  {:>4} fail  {:>4} skip",
            path.file_name().unwrap().to_string_lossy(),
            result.passed,
            result.failed,
            result.skipped,
        );
    }

    eprintln!(
        "\nTOTAL: {} cases — {} pass, {} fail, {} skip",
        totals.passed + totals.failed + totals.skipped,
        totals.passed,
        totals.failed,
        totals.skipped,
    );

    // Print up to 200 failures for triage. The remaining ones are tracked
    // by category in `KNOWN_ISSUES.md` (TODO).
    if !failures.is_empty() {
        eprintln!("\nFirst 200 failures (of {}):", failures.len());
        for f in failures.iter().take(200) {
            eprintln!(
                "  {}:{} [{}] {}",
                f.file
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy(),
                f.line,
                f.id,
                f.reason,
            );
        }
    }

    // Regression guard: any change that drops pass count below the
    // floor or raises fail count above the ceiling fails the test.
    // Bumped as known-issue categories get fixed; the remaining gap
    // is dqFMA's separated-rounding 1-ULP envelope (Phase 8 follow-up).
    const PASS_FLOOR: usize = 6150;
    const FAIL_CEILING: usize = 70;

    if totals.passed < PASS_FLOOR {
        panic!(
            "conformance pass count regressed: {} < floor {}",
            totals.passed, PASS_FLOOR
        );
    }
    if totals.failed > FAIL_CEILING {
        panic!(
            "conformance failure count regressed: {} > ceiling {}",
            totals.failed, FAIL_CEILING
        );
    }
}

#[derive(Default, Clone, Copy)]
struct Totals {
    passed: usize,
    failed: usize,
    skipped: usize,
}

impl Totals {
    fn merge(&mut self, other: &FileResult) {
        self.passed += other.passed;
        self.failed += other.failed;
        self.skipped += other.skipped;
    }
}

struct FileResult {
    passed: usize,
    failed: usize,
    skipped: usize,
}

struct Failure {
    file: PathBuf,
    line: usize,
    id: String,
    reason: String,
}

fn run_file(path: &Path, failures: &mut Vec<Failure>) -> FileResult {
    let content = fs::read_to_string(path).expect("read file");
    let mut ctx = Context::default();
    let mut passed = 0;
    let mut failed = 0;
    let mut skipped = 0;

    for (line_no, raw_line) in content.lines().enumerate() {
        let line = strip_comment(raw_line).trim();
        if line.is_empty() {
            continue;
        }
        if let Some((name, value)) = parse_directive(line) {
            ctx.apply(&name, &value);
            continue;
        }
        let case = match parse_test_case(line) {
            Some(c) => c,
            None => continue,
        };
        let outcome = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_case(&case, &ctx)
        })) {
            Ok(o) => o,
            Err(_) => Outcome::Fail("panic during op (debug_assert?)".to_string()),
        };
        match outcome {
            Outcome::Pass => passed += 1,
            Outcome::Skip => skipped += 1,
            Outcome::Fail(reason) => {
                failed += 1;
                failures.push(Failure {
                    file: path.to_owned(),
                    line: line_no + 1,
                    id: case.id.clone(),
                    reason,
                });
            }
        }
    }

    FileResult {
        passed,
        failed,
        skipped,
    }
}

// ---------------------------------------------------------------------------
// Parsing

fn strip_comment(line: &str) -> &str {
    line.find("--").map(|i| &line[..i]).unwrap_or(line)
}

fn parse_directive(line: &str) -> Option<(String, String)> {
    let colon = line.find(':')?;
    let name = line[..colon].trim();
    let value = line[colon + 1..].trim();
    if name
        .chars()
        .any(|c| !(c.is_ascii_alphabetic() || c == '_'))
        || name.is_empty()
    {
        return None;
    }
    Some((name.to_lowercase(), value.to_lowercase()))
}

#[derive(Debug)]
struct TestCase {
    id: String,
    op: String,
    operands: Vec<String>,
    expected: String,
    conditions: Vec<String>,
}

fn parse_test_case(line: &str) -> Option<TestCase> {
    // Tokenise, treating single-quoted strings as one token and removing
    // the quotes.
    let tokens = tokenise(line)?;
    let arrow_pos = tokens.iter().position(|t| t == "->")?;
    if arrow_pos < 3 || arrow_pos + 1 >= tokens.len() {
        return None;
    }
    let id = tokens[0].clone();
    let op = tokens[1].to_lowercase();
    let operands: Vec<String> = tokens[2..arrow_pos].to_vec();
    let expected = tokens[arrow_pos + 1].clone();
    let conditions: Vec<String> = tokens[arrow_pos + 2..]
        .iter()
        .map(|s| s.to_lowercase())
        .collect();
    Some(TestCase {
        id,
        op,
        operands,
        expected,
        conditions,
    })
}

fn tokenise(line: &str) -> Option<Vec<String>> {
    let mut out = Vec::new();
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b' ' | b'\t' => i += 1,
            b'\'' | b'"' => {
                let quote = bytes[i];
                i += 1;
                let start = i;
                while i < bytes.len() && bytes[i] != quote {
                    i += 1;
                }
                if i >= bytes.len() {
                    return None;
                }
                out.push(std::str::from_utf8(&bytes[start..i]).ok()?.to_string());
                i += 1;
            }
            _ => {
                let start = i;
                while i < bytes.len() && bytes[i] != b' ' && bytes[i] != b'\t' {
                    i += 1;
                }
                out.push(std::str::from_utf8(&bytes[start..i]).ok()?.to_string());
            }
        }
    }
    Some(out)
}

// ---------------------------------------------------------------------------
// Context

struct Context {
    precision: u32,
    max_exponent: i32,
    min_exponent: i32,
    rounding: RoundingMode,
}

impl Default for Context {
    fn default() -> Self {
        Self {
            precision: 34,
            max_exponent: 6144,
            min_exponent: -6143,
            rounding: RoundingMode::NearestEven,
        }
    }
}

impl Context {
    fn apply(&mut self, name: &str, value: &str) {
        match name {
            "precision" => {
                if let Ok(p) = value.parse() {
                    self.precision = p;
                }
            }
            "maxexponent" => {
                if let Ok(e) = value.parse() {
                    self.max_exponent = e;
                }
            }
            "minexponent" => {
                if let Ok(e) = value.parse() {
                    self.min_exponent = e;
                }
            }
            "rounding" => {
                self.rounding = match value {
                    "half_even" | "ceiling-tie-to-even" => RoundingMode::NearestEven,
                    "half_up" | "half_away_from_zero" => RoundingMode::NearestAway,
                    "down" | "trunc" | "toward_zero" => RoundingMode::TowardZero,
                    "ceiling" | "up" => RoundingMode::TowardPositive,
                    "floor" => RoundingMode::TowardNegative,
                    _ => self.rounding,
                };
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Execution

enum Outcome {
    Pass,
    Skip,
    Fail(String),
}

fn run_case(case: &TestCase, ctx: &Context) -> Outcome {
    // Skip tests at non-Decimal128 precision — the conformance suite
    // mixes shared and format-specific directives, but our impl is
    // 34-digit precision only.
    if ctx.precision != 34 {
        return Outcome::Skip;
    }

    let op_kind = match dispatch_op(&case.op) {
        Some(o) => o,
        None => return Outcome::Skip,
    };

    let result = match invoke(op_kind, &case.operands, ctx.rounding) {
        Some(r) => r,
        None => return Outcome::Skip,
    };

    // `class` results are class-name strings, not Decimal128 values —
    // we don't have a comparison harness for those yet.
    if matches!(result, OpResult::Class(_)) {
        return Outcome::Skip;
    }

    // Parse the expected result.
    let (expected, _) = match parse_value(&case.expected, ctx.rounding) {
        Some(v) => v,
        None => {
            return Outcome::Fail(format!(
                "couldn't parse expected {:?}",
                case.expected
            ))
        }
    };

    let expected_flags = expected_status(&case.conditions);

    compare(case, &result, expected, expected_flags)
}

#[derive(Clone, Copy)]
enum OpKind {
    Add,
    Subtract,
    Multiply,
    Divide,
    Fma,
    SquareRoot,
    RemainderNear,
    Abs,
    Minus,
    Plus,
    Compare,
    CompareTotal,
    Min,
    Max,
    Class,
}

fn dispatch_op(name: &str) -> Option<OpKind> {
    Some(match name {
        "add" => OpKind::Add,
        "subtract" => OpKind::Subtract,
        "multiply" => OpKind::Multiply,
        "divide" => OpKind::Divide,
        "fma" => OpKind::Fma,
        "squareroot" => OpKind::SquareRoot,
        "remaindernear" => OpKind::RemainderNear,
        "abs" => OpKind::Abs,
        "minus" => OpKind::Minus,
        "plus" => OpKind::Plus,
        "compare" => OpKind::Compare,
        "comparetotal" => OpKind::CompareTotal,
        "min" => OpKind::Min,
        "max" => OpKind::Max,
        "class" => OpKind::Class,
        _ => return None,
    })
}

fn invoke(op: OpKind, operands: &[String], rm: RoundingMode) -> Option<OpResult> {
    match op {
        OpKind::Add => {
            if operands.len() != 2 {
                return None;
            }
            let a = parse_value(&operands[0], rm)?.0;
            let b = parse_value(&operands[1], rm)?.0;
            let (v, s) = a.add(b, rm);
            Some(OpResult::Value(v, s))
        }
        OpKind::Subtract => {
            let a = parse_value(&operands[0], rm)?.0;
            let b = parse_value(&operands[1], rm)?.0;
            let (v, s) = a.sub(b, rm);
            Some(OpResult::Value(v, s))
        }
        OpKind::Multiply => {
            let a = parse_value(&operands[0], rm)?.0;
            let b = parse_value(&operands[1], rm)?.0;
            let (v, s) = a.mul(b, rm);
            Some(OpResult::Value(v, s))
        }
        OpKind::Divide => {
            let a = parse_value(&operands[0], rm)?.0;
            let b = parse_value(&operands[1], rm)?.0;
            let (v, s) = a.div(b, rm);
            Some(OpResult::Value(v, s))
        }
        OpKind::Fma => {
            let a = parse_value(&operands[0], rm)?.0;
            let b = parse_value(&operands[1], rm)?.0;
            let c = parse_value(&operands[2], rm)?.0;
            let (v, s) = a.fma(b, c, rm);
            Some(OpResult::Value(v, s))
        }
        OpKind::SquareRoot => {
            let a = parse_value(&operands[0], rm)?.0;
            let (v, s) = a.sqrt(rm);
            Some(OpResult::Value(v, s))
        }
        OpKind::RemainderNear => {
            let a = parse_value(&operands[0], rm)?.0;
            let b = parse_value(&operands[1], rm)?.0;
            let (v, s) = a.rem(b);
            Some(OpResult::Value(v, s))
        }
        OpKind::Abs => {
            let a = parse_value(&operands[0], rm)?.0;
            // IEEE 754 §5.5.1: abs raises INVALID on signaling NaN.
            let (v, s) = a.abs_with_status();
            Some(OpResult::Value(v, s))
        }
        OpKind::Minus => {
            let a = parse_value(&operands[0], rm)?.0;
            let (v, s) = a.neg_with_status();
            Some(OpResult::Value(v, s))
        }
        OpKind::Plus => {
            let a = parse_value(&operands[0], rm)?.0;
            // `plus(x) = add(0, x)` — identity-with-rounding. We don't yet
            // re-quantize.
            Some(OpResult::Value(a, Status::OK))
        }
        OpKind::Compare => {
            let a = parse_value(&operands[0], rm)?.0;
            let b = parse_value(&operands[1], rm)?.0;
            let (ord, s) = a.partial_cmp(b);
            let v = match ord {
                None => Decimal128::NAN,
                Some(core::cmp::Ordering::Less) => Decimal128::NEG_ONE,
                Some(core::cmp::Ordering::Equal) => Decimal128::ZERO,
                Some(core::cmp::Ordering::Greater) => Decimal128::ONE,
            };
            Some(OpResult::Value(v, s))
        }
        OpKind::CompareTotal => {
            let a = parse_value(&operands[0], rm)?.0;
            let b = parse_value(&operands[1], rm)?.0;
            let v = match a.total_cmp(b) {
                core::cmp::Ordering::Less => Decimal128::NEG_ONE,
                core::cmp::Ordering::Equal => Decimal128::ZERO,
                core::cmp::Ordering::Greater => Decimal128::ONE,
            };
            Some(OpResult::Value(v, Status::OK))
        }
        OpKind::Min => {
            let a = parse_value(&operands[0], rm)?.0;
            let b = parse_value(&operands[1], rm)?.0;
            let (v, s) = a.min(b);
            Some(OpResult::Value(v, s))
        }
        OpKind::Max => {
            let a = parse_value(&operands[0], rm)?.0;
            let b = parse_value(&operands[1], rm)?.0;
            let (v, s) = a.max(b);
            Some(OpResult::Value(v, s))
        }
        OpKind::Class => {
            let a = parse_value(&operands[0], rm)?.0;
            Some(OpResult::Class(a))
        }
    }
}

enum OpResult {
    Value(Decimal128, Status),
    Class(Decimal128),
}

fn parse_value(s: &str, rm: RoundingMode) -> Option<(Decimal128, Status)> {
    let trimmed = s.trim();
    // The decTest format uses '#' for hex-encoded literal bit patterns; we
    // don't support those.
    if trimmed.starts_with('#') {
        return None;
    }
    Decimal128::parse_str(trimmed, rm).ok()
}

fn expected_status(conditions: &[String]) -> Status {
    let mut s = Status::OK;
    for c in conditions {
        match c.as_str() {
            "inexact" => s |= Status::INEXACT,
            "underflow" => s |= Status::UNDERFLOW | Status::INEXACT,
            "overflow" => s |= Status::OVERFLOW | Status::INEXACT,
            "division_by_zero" => s |= Status::DIV_BY_ZERO,
            // Per dec-arith spec, "division_undefined" (0/0, Inf/Inf,
            // 0*Inf for fma) and "division_impossible" (integer divide
            // overflow) are subtypes of Invalid_operation.
            "invalid_operation" | "division_undefined" | "division_impossible" => {
                s |= Status::INVALID
            }
            // Ignore: rounded, clamped, subnormal, lost_digits, conversion_syntax
            _ => {}
        }
    }
    s
}

fn compare(
    case: &TestCase,
    result: &OpResult,
    expected: Decimal128,
    expected_flags: Status,
) -> Outcome {
    match result {
        OpResult::Class(_) => {
            // class op compares against the class name string directly;
            // we don't currently dispatch through it. Skip.
            Outcome::Skip
        }
        OpResult::Value(actual, actual_flags) => {
            // NaN compare: both must be NaN. Cohort/payload is allowed to
            // differ unless the test pinned a payload.
            if expected.is_nan() {
                if !actual.is_nan() {
                    return Outcome::Fail(format!(
                        "expected NaN, got {} ({:032X})",
                        actual,
                        actual.to_bits()
                    ));
                }
            } else if actual.to_bits() != expected.to_bits() {
                return Outcome::Fail(format!(
                    "value mismatch: got {} ({:032X}), want {} ({:032X})",
                    actual,
                    actual.to_bits(),
                    expected,
                    expected.to_bits()
                ));
            }

            // Status flags — compare only the IEEE 754 set we track.
            let mask = Status::INVALID
                | Status::DIV_BY_ZERO
                | Status::OVERFLOW
                | Status::UNDERFLOW
                | Status::INEXACT;
            let _ = mask; // value used via merge below
            let actual_relevant = mask_status(*actual_flags);
            let expected_relevant = mask_status(expected_flags);
            if actual_relevant.bits() != expected_relevant.bits() {
                return Outcome::Fail(format!(
                    "status mismatch (op {}): got {:#x}, want {:#x} from conditions {:?}",
                    case.op,
                    actual_relevant.bits(),
                    expected_relevant.bits(),
                    case.conditions,
                ));
            }
            Outcome::Pass
        }
    }
}

fn mask_status(s: Status) -> Status {
    Status::from_bits_truncate(s.bits())
}
