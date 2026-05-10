#![cfg(feature = "fmt")]
//! Conformance test runner for the vendored Mike Cowlishaw decTest
//! suite (`tests/vectors/dd*.decTest`).
//!
//! Each `.decTest` file is parsed line-by-line. Directives accumulate
//! into a mutable context; test cases are routed to the appropriate
//! `Decimal64` method as those methods land in subsequent commits per
//! the plan archived at
//! `docs/decisions/plans/2026-05-09-workspace-and-decimal-siblings.md`.
//! Until an op's dispatch arm is wired up, cases under that op count
//! as `skipped`.
//!
//! # Asymmetric per-file expectation guard
//!
//! Per ADR-0010 the expected pass count is checked **per file**, not
//! just against an aggregate floor. Each subsequent commit that wires
//! a dispatch arm raises the corresponding rows in
//! `expected_per_file`.

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
            "{:<28}  {:>5} pass  {:>4} fail  {:>5} skip",
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
        panic!(
            "conformance per-file expectation mismatch ({} files)",
            mismatch.len()
        );
    }

    const FAIL_CEILING: usize = 0;
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

/// Per-file expected pass count. Each row rises by the number of
/// cases the new dispatch arm passes; an intentional change requires
/// editing this table (see ADR-0010 in the workspace root for the
/// rationale).
///
/// Initial state at C5: every file is at 0 because no Decimal64
/// operations are wired up yet. Files not present in this table are
/// *not* checked by the per-file guard (their pass / skip counts
/// still feed the aggregate `FAIL_CEILING = 0` check). Each
/// subsequent commit adds rows for the files it now exercises.
const fn expected_per_file() -> &'static [(&'static str, usize)] {
    &[("ddBase.decTest", 0), ("ddEncode.decTest", 0)]
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

fn run_file(path: &Path, _failures: &mut Vec<Failure>) -> FileResult {
    let content = fs::read_to_string(path).expect("read file");
    let mut ctx = Context::default();
    let mut passed = 0;
    let failed = 0;
    let mut skipped = 0;

    for raw_line in content.lines() {
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
        }
    }

    FileResult {
        passed,
        failed,
        skipped,
    }
}

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
#[allow(dead_code)] // op / operands / expected / conditions consumed by dispatch arms in C6+
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

#[derive(Clone)]
struct Context {
    #[allow(dead_code)] // consumed by dispatch arms in C6+
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
        Self {
            precision: 16,
            max_exponent: 384,
            min_exponent: -383,
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
            _ => {}
        }
    }
}

#[allow(dead_code)] // Pass constructed by dispatch arms in C6+
enum Outcome {
    Pass,
    Skip,
}

fn run_case(case: &TestCase, _ctx: &Context) -> Outcome {
    let _ = &case.id;
    Outcome::Skip
}
