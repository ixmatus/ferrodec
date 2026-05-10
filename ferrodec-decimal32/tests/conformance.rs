#![cfg(feature = "fmt")]
//! Conformance test runner for the vendored Mike Cowlishaw decTest
//! suite (`tests/vectors/ds*.decTest`).
//!
//! Each `.decTest` file is parsed line-by-line. Directives
//! (`precision`, `rounding`, `maxExponent`, etc.) accumulate into a
//! mutable context. Test cases of the form
//!
//! ```text
//! id op operand1 [operand2 [operand3]] -> expected [conditions...]
//! ```
//!
//! will be routed to the appropriate `Decimal32` method as those
//! methods land in subsequent commits per the plan archived at
//! `docs/decisions/plans/2026-05-09-workspace-and-decimal-siblings.md`.
//! Until an op's dispatch arm is wired up, cases under that op count as
//! `skipped` (not `failed`).
//!
//! # Asymmetric per-file expectation guard
//!
//! Per ADR-0010 (testing strategy after the 6-agent correctness
//! review), the expected pass count is checked **per file**, not just
//! against an aggregate floor. This catches silent trade-offs: a
//! refactor that improves one file by N cases while regressing another
//! by N cases — net zero in aggregate — fails this guard. Each
//! intentional improvement requires editing `expected_per_file`, which
//! makes the change visible in git history.
//!
//! At present every entry in `expected_per_file` is zero because no
//! Decimal32 operations are implemented yet. Each subsequent commit
//! that wires a dispatch arm raises the corresponding row by the
//! number of cases it now passes.

use std::fs;
use std::path::{Path, PathBuf};

const VECTORS_DIR: &str = "tests/vectors";

#[test]
fn dectest_conformance() {
    let mut totals = Totals::default();
    let mut failures: Vec<Failure> = Vec::new();
    let mut file_results: Vec<(String, FileResult)> = Vec::new();

    let entries = fs::read_dir(VECTORS_DIR).expect("vectors directory");
    let mut paths: Vec<PathBuf> = entries
        .filter_map(std::result::Result::ok)
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
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        file_results.push((name, result));
    }

    eprintln!(
        "\nTOTAL: {} cases — {} pass, {} fail, {} skip",
        totals.passed + totals.failed + totals.skipped,
        totals.passed,
        totals.failed,
        totals.skipped,
    );

    if !failures.is_empty() {
        eprintln!("\nFirst 200 failures (of {}):", failures.len());
        for f in failures.iter().take(200) {
            eprintln!(
                "  {}:{} [{}] {}",
                f.file.file_name().unwrap_or_default().to_string_lossy(),
                f.line,
                f.id,
                f.reason,
            );
        }
    }

    // Per-file expectation table. Update entries as dispatch arms land.
    let expected = expected_per_file();
    let mut mismatch = Vec::new();
    for (name, exp_passed) in expected {
        let got = file_results
            .iter()
            .find(|(n, _)| n == name)
            .map_or(0, |(_, r)| r.passed);
        if got != *exp_passed {
            mismatch.push((name.to_string(), *exp_passed, got));
        }
    }
    if !mismatch.is_empty() {
        eprintln!("\nPer-file pass-count mismatch:");
        for (name, exp, got) in &mismatch {
            eprintln!("  {name:<28}  expected {exp}  got {got}");
        }
        eprintln!(
            "\nIf the change is intentional, update `expected_per_file` in\
             \ntests/conformance.rs to record the new baseline (one row per\
             \nfile). See ADR-0010 for why per-file expectations are\
             \nexact-match rather than floor-only."
        );
        panic!(
            "conformance per-file expectation mismatch ({} files)",
            mismatch.len()
        );
    }

    const FAIL_CEILING: usize = 0;
    // FAIL_CEILING is currently 0; if it ever rises, replace with `<=`.
    #[allow(clippy::absurd_extreme_comparisons)]
    {
        assert!(
            totals.failed <= FAIL_CEILING,
            "conformance failure count regressed: {} > ceiling {}",
            totals.failed,
            FAIL_CEILING
        );
    }
}

/// Per-file expected pass count. Each row rises by the number of cases
/// the new dispatch arm passes. Both files start at 0 because no
/// Decimal32 operations are wired up yet.
const fn expected_per_file() -> &'static [(&'static str, usize)] {
    &[("dsBase.decTest", 0), ("dsEncode.decTest", 0)]
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
        let outcome = run_case(&case, &ctx);
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
    line.find("--").map_or(line, |i| &line[..i])
}

fn parse_directive(line: &str) -> Option<(String, String)> {
    let colon = line.find(':')?;
    let name = line[..colon].trim();
    let value = line[colon + 1..].trim();
    if name.chars().any(|c| !(c.is_ascii_alphabetic() || c == '_')) || name.is_empty() {
        return None;
    }
    Some((name.to_lowercase(), value.to_lowercase()))
}

#[derive(Debug)]
#[allow(dead_code)] // op / operands / expected / conditions consumed by dispatch arms in B6+
struct TestCase {
    id: String,
    op: String,
    operands: Vec<String>,
    expected: String,
    conditions: Vec<String>,
}

fn parse_test_case(line: &str) -> Option<TestCase> {
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
// Context (directive accumulator)

#[derive(Clone)]
struct Context {
    #[allow(dead_code)] // consumed by dispatch arms in B6+
    precision: u32,
    #[allow(dead_code)]
    max_exponent: i32,
    #[allow(dead_code)]
    min_exponent: i32,
    #[allow(dead_code)]
    rounding: String,
}

impl Default for Context {
    fn default() -> Self {
        // Decimal32 IEEE 754-2019 §3.5 defaults; overridden by file
        // directives.
        Self {
            precision: 7,
            max_exponent: 96,
            min_exponent: -95,
            rounding: "half_even".to_string(),
        }
    }
}

impl Context {
    fn apply(&mut self, name: &str, value: &str) {
        match name {
            "precision" => {
                if let Ok(v) = value.parse() {
                    self.precision = v;
                }
            }
            "maxexponent" => {
                if let Ok(v) = value.parse() {
                    self.max_exponent = v;
                }
            }
            "minexponent" => {
                if let Ok(v) = value.parse() {
                    self.min_exponent = v;
                }
            }
            "rounding" => self.rounding = value.to_string(),
            // `extended`, `clamp`, `version` are recognised but ignored:
            // they describe the test-suite metadata, not Decimal32
            // behaviour we need to alter.
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Dispatch

#[allow(dead_code)] // Pass / Fail constructed by dispatch arms in B6+
enum Outcome {
    Pass,
    Skip,
    Fail(String),
}

fn run_case(case: &TestCase, _ctx: &Context) -> Outcome {
    // Every case currently skips. Subsequent commits add per-op
    // dispatch arms (B6 wires `tosci`, `apply`, `class`; B7 wires
    // `add`/`subtract`; etc.). The id parameter is suppressed here so
    // future arms can reference it without a new diff line.
    let _ = &case.id;
    Outcome::Skip
}
