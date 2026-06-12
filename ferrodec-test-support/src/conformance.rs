//! IBM decTest conformance harness scaffolding.
//!
//! The `.decTest` file format mixes directive lines (`precision: 16`,
//! `rounding: half_even`, ...) with test-case lines of the form:
//!
//! ```text
//! id op operand1 [operand2 [operand3]] -> expected [conditions...]
//! ```
//!
//! This module supplies the precision-agnostic machinery: the parser
//! ([`parse_directive`], [`parse_test_case`]), the directive-aware
//! [`Context`], the [`Outcome`] enum, the [`Failure`] / [`FileResult`]
//! / [`Totals`] accumulators, the asymmetric per-file expectation
//! guard from ADR-0010, and the [`run_suite`] driver. Each sibling
//! crate's harness implements a type-specific `dispatch` closure that
//! routes `Outcome`s back from its own `parse_str` / `Display`
//! implementations, and calls [`run_suite`] from a single `#[test]`.

use std::fs;
use std::path::{Path, PathBuf};

use ferrodec_ieee::{RoundingMode, Status};

// ---------------------------------------------------------------------------
// Parser

/// A parsed test case from a `.decTest` file.
#[derive(Debug, Clone)]
pub struct TestCase {
    pub id: String,
    pub op: String,
    pub operands: Vec<String>,
    pub expected: String,
    pub conditions: Vec<String>,
}

/// Strip an end-of-line `--` comment, ignoring `--` inside single- or
/// double-quoted operands (the same quoting rules as [`tokenise`]). The
/// vendored corpora carry adversarial parse vectors whose operands are
/// quoted strings like `'--1'` and `'1E--1'`; a position-blind cut at the
/// first `--` silently dropped them from every bucket (fd-aqs.9).
#[must_use]
pub fn strip_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut quote: Option<u8> = None;
    let mut i = 0;
    while i < bytes.len() {
        match quote {
            Some(q) => {
                if bytes[i] == q {
                    quote = None;
                }
            }
            None => match bytes[i] {
                b'\'' | b'"' => quote = Some(bytes[i]),
                b'-' if i + 1 < bytes.len() && bytes[i + 1] == b'-' => {
                    return &line[..i];
                }
                _ => {}
            },
        }
        i += 1;
    }
    line
}

/// Parse a directive line of the form `name: value`. Returns
/// `(lowercased_name, lowercased_value)` or `None` if the line isn't a
/// directive.
#[must_use]
pub fn parse_directive(line: &str) -> Option<(String, String)> {
    let colon = line.find(':')?;
    let name = line[..colon].trim();
    let value = line[colon + 1..].trim();
    if name.chars().any(|c| !(c.is_ascii_alphabetic() || c == '_')) || name.is_empty() {
        return None;
    }
    Some((name.to_lowercase(), value.to_lowercase()))
}

/// Parse a single test-case line. Returns `None` if the line lacks the
/// `->` separator or has too few tokens.
#[must_use]
pub fn parse_test_case(line: &str) -> Option<TestCase> {
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

/// Whitespace-respecting tokeniser that honours single- and
/// double-quoted strings.
#[must_use]
pub fn tokenise(line: &str) -> Option<Vec<String>> {
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

/// Directive-aware context tracking the active precision, exponent
/// range, and rounding mode for the file currently being processed.
/// Constructed via [`Context::for_decimal32`] /
/// [`Context::for_decimal64`] / [`Context::for_decimal128`] (or with
/// custom defaults), then mutated as `precision:` / `rounding:` /
/// `maxExponent:` / `minExponent:` directives appear.
#[derive(Clone, Debug)]
pub struct Context {
    pub precision: u32,
    pub max_exponent: i32,
    pub min_exponent: i32,
    pub rounding: String,
}

impl Context {
    /// IEEE 754-2019 §3.5 Decimal32 defaults.
    #[must_use]
    pub fn for_decimal32() -> Self {
        Self {
            precision: 7,
            max_exponent: 96,
            min_exponent: -95,
            rounding: "half_even".to_string(),
        }
    }

    /// IEEE 754-2019 §3.5 Decimal64 defaults.
    #[must_use]
    pub fn for_decimal64() -> Self {
        Self {
            precision: 16,
            max_exponent: 384,
            min_exponent: -383,
            rounding: "half_even".to_string(),
        }
    }

    /// IEEE 754-2019 §3.5 Decimal128 defaults.
    #[must_use]
    pub fn for_decimal128() -> Self {
        Self {
            precision: 34,
            max_exponent: 6144,
            min_exponent: -6143,
            rounding: "half_even".to_string(),
        }
    }

    /// Apply a directive line. `extended`, `clamp`, `version` are
    /// recognised but ignored (test-suite metadata, not runtime
    /// behaviour).
    pub fn apply(&mut self, name: &str, value: &str) {
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

// ---------------------------------------------------------------------------
// Dispatch outcome

/// Outcome of running a single test case. The dispatch closure
/// returns `Skip` for cases under op codes the harness doesn't (yet)
/// route, and `Pass` / `Fail` for cases the dispatch does cover.
#[derive(Debug)]
pub enum Outcome {
    Pass,
    Skip,
    Fail(String),
}

// ---------------------------------------------------------------------------
// Helpers exposed for dispatch closures

/// Map a decTest rounding directive onto the IEEE 754 [`RoundingMode`]
/// it corresponds to. Returns `None` for non-IEEE directives
/// (`half_down`, `05up`, `up`) — cases under those should be skipped
/// rather than coerced (mirrors ferrodec's ADR-0005 posture).
#[must_use]
pub fn map_rounding(s: &str) -> Option<RoundingMode> {
    match s {
        "half_even" => Some(RoundingMode::NearestEven),
        "half_up" => Some(RoundingMode::NearestAway),
        "down" => Some(RoundingMode::TowardZero),
        "ceiling" => Some(RoundingMode::TowardPositive),
        "floor" => Some(RoundingMode::TowardNegative),
        _ => None,
    }
}

/// Project decTest condition tokens onto our 5-flag [`Status`] set.
/// Informational tokens (`Rounded`, `Subnormal`, `Clamped`,
/// `Lost_digits`) are deliberately ignored: they're not raised by our
/// `Status` and decTest treats them as supplementary information
/// rather than IEEE 754 exceptions.
#[must_use]
pub fn decode_conditions(conditions: &[String]) -> Status {
    let mut s = Status::OK;
    for cond in conditions {
        match cond.as_str() {
            "inexact" => s |= Status::INEXACT,
            "overflow" => s |= Status::OVERFLOW | Status::INEXACT,
            "underflow" => s |= Status::UNDERFLOW | Status::INEXACT,
            "invalid_operation"
            | "division_impossible"
            | "division_undefined"
            | "conversion_syntax" => {
                s |= Status::INVALID;
            }
            "division_by_zero" => s |= Status::DIV_BY_ZERO,
            _ => {}
        }
    }
    s
}

/// Compare an operation's emitted [`Status`] against the expected
/// status decoded from a decTest case, ignoring informational flags.
///
/// [`decode_conditions`] deliberately never produces `CLAMPED` (it is
/// supplementary information in decTest, not an IEEE 754 exception),
/// so an implementation that *does* raise `CLAMPED` at a §7.4 clamp
/// site would otherwise mismatch every clamped case. The conformance
/// contract is the five IEEE 754 mandatory flags; `CLAMPED` is masked
/// on the actual side to mirror its omission on the expected side.
/// Per-file pass counts are therefore unaffected by `CLAMPED`
/// emission.
#[must_use]
pub fn status_conformance_eq(actual: Status, expected: Status) -> bool {
    let mask = !Status::CLAMPED.bits();
    (actual.bits() & mask) == (expected.bits() & mask)
}

// ---------------------------------------------------------------------------
// Driver

/// Per-file pass/fail/skip counts, plus the count of non-empty,
/// non-directive lines the parser could not tokenise. Unparseable lines
/// are pinned at zero by [`run_suite`]: a parser regression that starts
/// dropping cases must fail loudly, not shrink the buckets silently.
#[derive(Default, Clone, Copy, Debug)]
pub struct FileResult {
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub unparsed: usize,
}

/// Aggregate counts across every file in the suite.
#[derive(Default, Clone, Copy, Debug)]
pub struct Totals {
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub unparsed: usize,
}

impl Totals {
    fn merge(&mut self, other: &FileResult) {
        self.passed += other.passed;
        self.failed += other.failed;
        self.skipped += other.skipped;
        self.unparsed += other.unparsed;
    }
}

/// One failing case's location and reason — printed in the run
/// summary when a regression triggers a panic.
#[derive(Debug)]
pub struct Failure {
    pub file: PathBuf,
    pub line: usize,
    pub id: String,
    pub reason: String,
}

/// Walk every `*.decTest` file under `vectors_dir`, run each case
/// through `dispatch` (paired with the active `Context`), and panic
/// if either the per-file expectation table fails to match the
/// observed pass counts or the aggregate `failed` count exceeds
/// `FAIL_CEILING = 0`.
///
/// The asymmetric per-file guard is the load-bearing piece per
/// ADR-0010: a refactor that improves one file by N cases while
/// regressing another by N cases — net zero in aggregate — fails
/// this guard. Each intentional improvement requires editing the
/// `expected_per_file` table the caller passes in, which makes the
/// change visible in git history.
///
/// `initial_context` is what the harness starts each file with;
/// per-file directives can override it during processing. Use
/// [`Context::for_decimal32`] / `for_decimal64` / `for_decimal128` to
/// pick the format defaults.
pub fn run_suite<F>(
    vectors_dir: &str,
    expected_per_file: &[(&str, usize)],
    initial_context: &Context,
    mut dispatch: F,
) where
    F: FnMut(&TestCase, &Context) -> Outcome,
{
    let mut totals = Totals::default();
    let mut failures: Vec<Failure> = Vec::new();
    let mut file_results: Vec<(String, FileResult)> = Vec::new();

    let entries = fs::read_dir(vectors_dir).expect("vectors directory");
    let mut paths: Vec<PathBuf> = entries
        .filter_map(std::result::Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("decTest"))
        .collect();
    paths.sort();

    for path in paths {
        let result = run_file(&path, &mut failures, initial_context.clone(), &mut dispatch);
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
        "\nTOTAL: {} cases — {} pass, {} fail, {} skip, {} unparsed",
        totals.passed + totals.failed + totals.skipped,
        totals.passed,
        totals.failed,
        totals.skipped,
        totals.unparsed,
    );

    // Pinned at zero: every non-empty, non-directive line must tokenise.
    // A parser regression that silently drops cases shrinks the buckets
    // without failing anything; this is the loud counterpart (fd-aqs.9).
    assert_eq!(
        totals.unparsed, 0,
        "{} non-directive lines failed to parse (see UNPARSED lines above)",
        totals.unparsed
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

    let mut mismatch = Vec::new();
    for (name, exp_passed) in expected_per_file {
        let got = file_results
            .iter()
            .find(|(n, _)| n == name)
            .map_or(0, |(_, r)| r.passed);
        if got != *exp_passed {
            mismatch.push(((*name).to_string(), *exp_passed, got));
        }
    }
    if !mismatch.is_empty() {
        eprintln!("\nPer-file pass-count mismatch:");
        for (name, exp, got) in &mismatch {
            eprintln!("  {name:<28}  expected {exp}  got {got}");
        }
        eprintln!(
            "\nIf the change is intentional, update `expected_per_file` in\
             \nyour tests/conformance.rs to record the new baseline (one row\
             \nper file). See ADR-0010 for why per-file expectations are\
             \nexact-match rather than floor-only."
        );
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

fn run_file<F>(
    path: &Path,
    failures: &mut Vec<Failure>,
    mut ctx: Context,
    dispatch: &mut F,
) -> FileResult
where
    F: FnMut(&TestCase, &Context) -> Outcome,
{
    let content = fs::read_to_string(path).expect("read file");
    let mut passed = 0;
    let mut failed = 0;
    let mut skipped = 0;
    let mut unparsed = 0;

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
            None => {
                unparsed += 1;
                eprintln!(
                    "UNPARSED {}:{}: {raw_line}",
                    path.file_name().unwrap_or_default().to_string_lossy(),
                    line_no + 1,
                );
                continue;
            }
        };
        let outcome = dispatch(&case, &ctx);
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
        unparsed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_directive() {
        let (n, v) = parse_directive("precision: 7").unwrap();
        assert_eq!(n, "precision");
        assert_eq!(v, "7");
    }

    #[test]
    fn directive_lowercases_name_and_value() {
        let (n, v) = parse_directive("Rounding: Half_Even").unwrap();
        assert_eq!(n, "rounding");
        assert_eq!(v, "half_even");
    }

    #[test]
    fn rejects_non_directive() {
        assert!(parse_directive("dsbas001 toSci '0' -> '0'").is_none());
    }

    #[test]
    fn parses_test_case_with_two_operands() {
        let case = parse_test_case("dsadd001 add '1' '2' -> '3' Inexact").unwrap();
        assert_eq!(case.id, "dsadd001");
        assert_eq!(case.op, "add");
        assert_eq!(case.operands, vec!["1", "2"]);
        assert_eq!(case.expected, "3");
        assert_eq!(case.conditions, vec!["inexact"]);
    }

    #[test]
    fn strip_comment_removes_double_dash_tail() {
        assert_eq!(strip_comment("foo -- bar"), "foo ");
        assert_eq!(strip_comment("no comment here"), "no comment here");
    }

    #[test]
    fn strip_comment_ignores_quoted_dashes() {
        // The adversarial parse vectors: '--1' and '1E--1' are operands,
        // not comments (ddBase 532/583, dsBase 496/547, base 548/599).
        assert_eq!(
            strip_comment("ddbas532 toSci '--1' -> NaN Conversion_syntax"),
            "ddbas532 toSci '--1' -> NaN Conversion_syntax"
        );
        let full = "x toSci \"1E--1\" -> NaN";
        assert_eq!(strip_comment(full), full);
        // A real comment after a quoted operand still strips.
        assert_eq!(
            strip_comment("x toSci '--1' -> NaN -- why"),
            "x toSci '--1' -> NaN "
        );
        // An apostrophe inside a comment does not arm quote mode.
        assert_eq!(
            strip_comment("x add 1 2 -> 3 -- don't care"),
            "x add 1 2 -> 3 "
        );
    }

    #[test]
    fn quoted_dash_case_parses() {
        let case = parse_test_case(strip_comment(
            "ddbas532 toSci '--1' -> NaN Conversion_syntax",
        ))
        .unwrap();
        assert_eq!(case.operands, vec!["--1"]);
        assert_eq!(case.expected, "NaN");
    }

    #[test]
    fn map_rounding_recognises_five_ieee_modes() {
        assert!(map_rounding("half_even").is_some());
        assert!(map_rounding("half_up").is_some());
        assert!(map_rounding("down").is_some());
        assert!(map_rounding("ceiling").is_some());
        assert!(map_rounding("floor").is_some());
        // Non-IEEE: skip.
        assert!(map_rounding("half_down").is_none());
        assert!(map_rounding("05up").is_none());
    }

    #[test]
    fn decode_conditions_overflow_implies_inexact() {
        let s = decode_conditions(&["overflow".to_string()]);
        assert!(s.overflow());
        assert!(s.inexact());
    }

    #[test]
    fn context_for_decimal32_defaults() {
        let ctx = Context::for_decimal32();
        assert_eq!(ctx.precision, 7);
        assert_eq!(ctx.max_exponent, 96);
    }

    #[test]
    fn context_for_decimal64_defaults() {
        let ctx = Context::for_decimal64();
        assert_eq!(ctx.precision, 16);
        assert_eq!(ctx.max_exponent, 384);
    }
}
