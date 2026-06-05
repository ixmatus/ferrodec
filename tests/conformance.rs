#![cfg(feature = "fmt")]
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
//! Ops we don't yet implement (`comparesig`, `tointegral`, ...) are
//! counted as `skipped`, not failed. When the failures list is
//! non-empty the test panics with a per-file summary so triage can
//! start at a specific test ID.

use std::fs;
use std::path::{Path, PathBuf};

use ferrodec::{Decimal128, RoundingMode, Status};

const VECTORS_DIR: &str = "tests/vectors";

/// Slice F.3 regression guard. Pins the corpus-level count of test
/// cases whose active `rounding:` directive maps to
/// `CaseRounding::Unsupported` (`half_down` and `05up` per
/// ADR-0005). The current count is 101 cases. The
/// `KNOWN_ISSUES.md` "99 residual skips" figure is slightly smaller
/// (2 cases also hit earlier skip reasons — precision-mismatch or
/// `#`-operand paths — so the runner attributes them to those
/// buckets first).
///
/// Future drift in either direction surfaces here as a count
/// change rather than a silent skip-bucket migration. If the
/// `Unsupported` taxonomy ever changes (say, ferrodec adds
/// `half_down` support and removes 90 cases from this bucket),
/// that's a deliberate policy change that should edit both this
/// expected count and the matching `KNOWN_ISSUES.md` row.
#[test]
fn dectest_skip_taxonomy_non_ieee_rounding_directive_count() {
    const EXPECTED_NON_IEEE_DIRECTIVE_CASES: usize = 101;

    let entries = fs::read_dir(VECTORS_DIR).expect("vectors directory");
    let mut paths: Vec<PathBuf> = entries
        .filter_map(std::result::Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("decTest"))
        .collect();
    paths.sort();

    let mut non_ieee_count = 0usize;
    for path in &paths {
        let content = fs::read_to_string(path).expect("read decTest");
        let mut ctx = Context {
            encoding: if is_dpd_encoded(path) {
                Encoding::Dpd
            } else {
                Encoding::Bid
            },
            ..Context::default()
        };
        for raw_line in content.lines() {
            let line = strip_comment(raw_line).trim();
            if line.is_empty() {
                continue;
            }
            if let Some((name, value)) = parse_directive(line) {
                ctx.apply(&name, &value);
                continue;
            }
            if parse_test_case(line).is_some() && matches!(ctx.rounding, CaseRounding::Unsupported)
            {
                non_ieee_count += 1;
            }
        }
    }

    assert_eq!(
        non_ieee_count, EXPECTED_NON_IEEE_DIRECTIVE_CASES,
        "non-IEEE rounding (half_down / 05up) directive-case count drifted; \
         update both this expected value and the KNOWN_ISSUES.md row if intentional"
    );
}

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

    // Print up to 200 failures for triage. The skipped cases (currently
    // 572 of 8721) are categorised in `KNOWN_ISSUES.md` at the repo root.
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

    // Regression guard, two layers:
    //
    // 1. **Per-file expectation table** (below). Each `.decTest`
    //    file's pass count must match its row exactly. The asymmetry
    //    is intentional: a legitimate increase requires a one-line
    //    table edit (explicit, surfaces in git history); a silent
    //    trade-off (`pass↑file_a + pass↓file_b` = total unchanged)
    //    becomes a hard failure. ADR-0010 documents the rationale.
    //
    // 2. **Aggregate `FAIL_CEILING = 0`**: any failure anywhere
    //    panics. Skips don't count.
    //
    // Notes on what's skipped and why:
    // * `up` (round-away-from-zero, directional) is honored via a
    //   runner-side two-pass wrapper that uses TowardZero to detect
    //   the sign of the exact result, then dispatches to
    //   TowardPositive or TowardNegative.
    // * `half_down` and `05up` are General Decimal Arithmetic modes
    //   that are not part of IEEE 754-2019; cases under those
    //   directives are skipped rather than coerced into a kernel
    //   mode that doesn't match the spec (ADR-0005).
    // * The DPD interchange vectors (dqEncode, dqCanonical) only
    //   run when the `dpd` Cargo feature is enabled. With the
    //   feature off, both files are skipped at the file gate.
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
            let delta = got.wrapping_sub(*exp) as i64
                - if *got < *exp {
                    (*exp - *got) as i64 * 2
                } else {
                    0
                };
            eprintln!("  {name:<28}  expected {exp}  got {got}  (Δ {delta:+})");
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

/// Whether a `.decTest` file's `#hex` operands and expecteds are
/// IEEE 754 DPD-encoded byte patterns (decoded via
/// `Decimal128::from_dpd_bytes`) rather than the runner's default
/// BID-encoded raw bits (`Decimal128::from_bits`).
///
/// Determined by file name: only `dqEncode.decTest` and
/// `dqCanonical.decTest` ship DPD vectors. Everything else uses
/// either decimal-string operands (the common case) or BID `#hex`.
fn is_dpd_encoded(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|s| s.to_str()),
        Some("dqEncode.decTest" | "dqCanonical.decTest"),
    )
}

fn run_file(path: &Path, failures: &mut Vec<Failure>) -> FileResult {
    // The DPD vector files require `Decimal128::from_dpd_bytes` /
    // `to_dpd_bytes`, which only exist behind the `dpd` cargo
    // feature. Without the feature, count the file as zero / zero /
    // zero so the existing 8 622-pass baseline holds.
    if is_dpd_encoded(path) && !cfg!(feature = "dpd") {
        return FileResult {
            passed: 0,
            failed: 0,
            skipped: 0,
        };
    }
    let content = fs::read_to_string(path).expect("read file");
    let mut ctx = Context {
        encoding: if is_dpd_encoded(path) {
            Encoding::Dpd
        } else {
            Encoding::Bid
        },
        ..Context::default()
    };
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
    rounding: CaseRounding,
    encoding: Encoding,
}

/// Interpretation of `#hex` operand and expected literals in this
/// file. See [`is_dpd_encoded`].
#[derive(Clone, Copy, PartialEq, Eq)]
enum Encoding {
    /// `#hex` decodes via `Decimal128::from_bits` (raw BID bit
    /// pattern, zero-extended for short inputs). Default.
    Bid,
    /// `#hex` is exactly 32 hex chars and decodes via
    /// `Decimal128::from_dpd_bytes` (IEEE 754-2019 DPD layout,
    /// big-endian).
    Dpd,
}

/// Rounding directive a decTest block may select.
///
/// IEEE 754-2019 defines five rounding-direction attributes; decTest
/// adds two more (`half_down`, `05up`) plus a directional `up` (round
/// away from zero). ferrodec only implements the IEEE set, so for the
/// extras the runner either emulates (`up`, via a two-pass wrapper) or
/// skips the case (`half_down`, `05up`) rather than coercing them onto
/// a kernel mode they don't match.
#[derive(Clone, Copy)]
enum CaseRounding {
    /// One of the five IEEE 754 rounding-direction attributes.
    Ieee(RoundingMode),
    /// decTest `up` — round away from zero (directional). Implemented
    /// as a runner-side two-pass: `TowardZero` to determine the sign of
    /// the exact result, then TowardPositive/TowardNegative to round
    /// magnitude up.
    Up,
    /// `half_down` (nearest, ties toward zero) and `05up` (round-zero-
    /// five-up). Not in IEEE 754; cases under these are skipped.
    Unsupported,
}

impl CaseRounding {
    /// Rounding mode to use when parsing operand literals or expected
    /// values for cases under this directive. For literals that fit in
    /// 34 digits exactly (the common case) the mode is irrelevant; we
    /// fall back to `NearestEven` for the non-IEEE directives so parses
    /// of long literals stay deterministic.
    fn for_parse(self) -> RoundingMode {
        match self {
            Self::Ieee(m) => m,
            Self::Up | Self::Unsupported => RoundingMode::NearestEven,
        }
    }
}

impl Default for Context {
    fn default() -> Self {
        Self {
            precision: 34,
            max_exponent: 6144,
            min_exponent: -6143,
            rounding: CaseRounding::Ieee(RoundingMode::NearestEven),
            encoding: Encoding::Bid,
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
                    "half_even" | "ceiling-tie-to-even" => {
                        CaseRounding::Ieee(RoundingMode::NearestEven)
                    }
                    "half_up" | "half_away_from_zero" => {
                        CaseRounding::Ieee(RoundingMode::NearestAway)
                    }
                    "down" | "trunc" | "toward_zero" => {
                        CaseRounding::Ieee(RoundingMode::TowardZero)
                    }
                    "ceiling" => CaseRounding::Ieee(RoundingMode::TowardPositive),
                    "floor" => CaseRounding::Ieee(RoundingMode::TowardNegative),
                    "up" => CaseRounding::Up,
                    "half_down" | "05up" => CaseRounding::Unsupported,
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

    // decTest's bare `#` is the "null operand" sentinel: a missing /
    // unparseable operand whose expected behavior per the dec spec is
    // `(NaN, Invalid_operation)` regardless of which op was nominally
    // dispatched. The runner's `parse_value` returns `None` for bare
    // `#`, which would otherwise route to `Outcome::Skip` because
    // `invoke()` discards parse-time status. Short-circuit here so
    // these cases match against the expected `NaN Invalid_operation`.
    if case.operands.iter().any(|o| o.trim() == "#") {
        return run_null_test(case, ctx);
    }

    let result = match ctx.rounding {
        CaseRounding::Unsupported => return Outcome::Skip,
        CaseRounding::Ieee(rm) => match invoke(op_kind, &case.operands, rm, ctx.encoding) {
            Some(r) => r,
            None => return Outcome::Skip,
        },
        CaseRounding::Up => match invoke_up(op_kind, &case.operands, ctx.encoding) {
            Some(r) => r,
            None => return Outcome::Skip,
        },
    };

    // `class` results are class-name strings, not Decimal128 values.
    // Compare directly against `case.expected` (also a string) and
    // short-circuit; the expected isn't a Decimal128 literal so we
    // can't route it through the value-comparator below.
    if let OpResult::Class(name) = &result {
        return if name == &case.expected {
            Outcome::Pass
        } else {
            Outcome::Fail(format!(
                "expected class {:?}, got {:?}",
                case.expected, name
            ))
        };
    }

    // Parse the expected result. Expected literals in decTest are exact
    // strings, so any rounding mode parses the same; we use the IEEE
    // mode where one applies and NearestEven otherwise.
    let (expected, _) = match parse_value(&case.expected, ctx.rounding.for_parse(), ctx.encoding) {
        Some(v) => v,
        None => return Outcome::Fail(format!("couldn't parse expected {:?}", case.expected)),
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
    Remainder,
    Abs,
    Minus,
    Plus,
    Apply,
    Compare,
    /// fd-bef.1 (ADR-0049): GDA `compareSignaling` — like `Compare`
    /// but every NaN operand (quiet or signaling) raises `INVALID`.
    CompareSig,
    CompareTotal,
    CompareTotalMag,
    Min,
    Max,
    Class,
    Quantize,
    SameQuantum,
    ScaleB,
    LogB,
    NextPlus,
    NextMinus,
    /// fd-ci0.4 (ADR-0031): General Decimal Arithmetic `reduce` —
    /// strip non-significant trailing zeros from a finite coefficient.
    Reduce,
    /// fd-ci0.5 (ADR-0031): General Decimal Arithmetic
    /// `divideInteger` — truncated integer quotient at exponent 0.
    DivideInteger,
    /// fd-ci0.6 (ADR-0031): General Decimal Arithmetic
    /// `logical_invert` — digit-wise complement of a logical operand.
    LogicalInvert,
    /// fd-ci0.7 (ADR-0031): logical AND / OR / XOR over digit-wise
    /// logical operands; one variant per truth table.
    LogicalAnd,
    LogicalOr,
    LogicalXor,
    /// fd-ci0.8 (ADR-0031): digit-shift inside the precision-wide
    /// window; zero-fill on the leading side.
    Shift,
    /// fd-ci0.9 (ADR-0031): digit-rotate inside the precision-wide
    /// window; modular wrap.
    Rotate,
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
        // decTest's `remainder` is the *truncating* remainder
        // (sign of dividend, integer quotient toward zero),
        // distinct from `remaindernear` (round-half-to-even on the
        // quotient, the IEEE 754 §5.3.1 remainder). ferrodec
        // implements both: `Decimal128::rem_near` for IEEE remainder,
        // `Decimal128::rem_trunc` for the truncating variant. The
        // 1.x bare `rem` spelling was retired in 2.0 per ADR-0027.
        "remainder" => OpKind::Remainder,
        "abs" => OpKind::Abs,
        "minus" => OpKind::Minus,
        "plus" => OpKind::Plus,
        // decTest `apply` exercises the rounding/precision context.
        // ferrodec is fixed at PRECISION=34, and `parse_str` already
        // applies precision when constructing the operand, so apply
        // becomes identity at this layer.
        "apply" => OpKind::Apply,
        "compare" => OpKind::Compare,
        // decTest `comparesig`: GDA `compareSignaling` (ADR-0049).
        "comparesig" => OpKind::CompareSig,
        "comparetotal" => OpKind::CompareTotal,
        // decTest spells the magnitude variant `comparetotmag` (no `al`).
        "comparetotmag" | "comparetotalmag" => OpKind::CompareTotalMag,
        "min" => OpKind::Min,
        "max" => OpKind::Max,
        "class" => OpKind::Class,
        "quantize" => OpKind::Quantize,
        "samequantum" => OpKind::SameQuantum,
        "scaleb" => OpKind::ScaleB,
        "logb" => OpKind::LogB,
        "nextplus" => OpKind::NextPlus,
        "nextminus" => OpKind::NextMinus,
        // decTest `reduce`: General Decimal Arithmetic trailing-zero
        // strip (ADR-0031). Exact; never raises INEXACT. Zero of any
        // cohort normalises to exponent 0.
        "reduce" => OpKind::Reduce,
        // decTest `divideint`: General Decimal Arithmetic truncated
        // integer quotient at exponent 0 (ADR-0031).
        "divideint" => OpKind::DivideInteger,
        // decTest `invert`: digit-wise complement of a logical operand
        // (ADR-0031). Operand must be a non-negative integer at
        // exponent 0 with all digits in {0, 1}.
        "invert" => OpKind::LogicalInvert,
        // decTest `and` / `or` / `xor`: digit-wise truth-table ops
        // over two logical operands (ADR-0031).
        "and" => OpKind::LogicalAnd,
        "or" => OpKind::LogicalOr,
        "xor" => OpKind::LogicalXor,
        // decTest `shift`: digit-shift within the precision-wide
        // window with zero fill (ADR-0031).
        "shift" => OpKind::Shift,
        // decTest `rotate`: digit-rotate within the precision-wide
        // window with modular wrap (ADR-0031).
        "rotate" => OpKind::Rotate,
        _ => return None,
    })
}

fn invoke(op: OpKind, operands: &[String], rm: RoundingMode, enc: Encoding) -> Option<OpResult> {
    match op {
        OpKind::Add => {
            if operands.len() != 2 {
                return None;
            }
            let a = parse_value(&operands[0], rm, enc)?.0;
            let b = parse_value(&operands[1], rm, enc)?.0;
            let (v, s) = a.add(b, rm);
            Some(OpResult::Value(v, s))
        }
        OpKind::Subtract => {
            let a = parse_value(&operands[0], rm, enc)?.0;
            let b = parse_value(&operands[1], rm, enc)?.0;
            let (v, s) = a.sub(b, rm);
            Some(OpResult::Value(v, s))
        }
        OpKind::Multiply => {
            let a = parse_value(&operands[0], rm, enc)?.0;
            let b = parse_value(&operands[1], rm, enc)?.0;
            let (v, s) = a.mul(b, rm);
            Some(OpResult::Value(v, s))
        }
        OpKind::Divide => {
            let a = parse_value(&operands[0], rm, enc)?.0;
            let b = parse_value(&operands[1], rm, enc)?.0;
            let (v, s) = a.div(b, rm);
            Some(OpResult::Value(v, s))
        }
        OpKind::Fma => {
            let a = parse_value(&operands[0], rm, enc)?.0;
            let b = parse_value(&operands[1], rm, enc)?.0;
            let c = parse_value(&operands[2], rm, enc)?.0;
            let (v, s) = a.fma(b, c, rm);
            Some(OpResult::Value(v, s))
        }
        OpKind::SquareRoot => {
            let a = parse_value(&operands[0], rm, enc)?.0;
            let (v, s) = a.sqrt(rm);
            Some(OpResult::Value(v, s))
        }
        OpKind::RemainderNear => {
            let a = parse_value(&operands[0], rm, enc)?.0;
            let b = parse_value(&operands[1], rm, enc)?.0;
            let (v, s) = a.rem_near(b);
            Some(OpResult::Value(v, s))
        }
        OpKind::Remainder => {
            let a = parse_value(&operands[0], rm, enc)?.0;
            let b = parse_value(&operands[1], rm, enc)?.0;
            let (v, s) = a.rem_trunc(b);
            Some(OpResult::Value(v, s))
        }
        OpKind::Abs => {
            let a = parse_value(&operands[0], rm, enc)?.0;
            // IEEE 754 §5.5.1: abs raises INVALID on signaling NaN.
            let (v, s) = a.abs_with_status();
            Some(OpResult::Value(v, s))
        }
        OpKind::Minus => {
            let a = parse_value(&operands[0], rm, enc)?.0;
            let (v, s) = a.neg_with_status();
            Some(OpResult::Value(v, s))
        }
        OpKind::Plus => {
            let a = parse_value(&operands[0], rm, enc)?.0;
            // `plus(x) = add(0, x)` — identity-with-rounding. We don't yet
            // re-quantize.
            Some(OpResult::Value(a, Status::OK))
        }
        OpKind::Apply => {
            // decTest `apply` returns the operand after applying the
            // current precision/rounding context. parse_value already
            // routes through `parse_str` under `rm` and rounds to
            // PRECISION=34, so the parsed value is the result.
            let (a, s) = parse_value(&operands[0], rm, enc)?;
            Some(OpResult::Value(a, s))
        }
        OpKind::Compare => {
            let a = parse_value(&operands[0], rm, enc)?.0;
            let b = parse_value(&operands[1], rm, enc)?.0;
            let (ord, s) = a.partial_cmp(b);
            let v = match ord {
                // Unordered means a NaN operand. The GDA `compare` op
                // returns that NaN propagated (sNaN-priority, sign and
                // payload preserved, signaling quietened) — exactly the
                // arithmetic NaN-propagation rule, so reuse `add`.
                None => a.add(b, RoundingMode::NearestEven).0,
                Some(core::cmp::Ordering::Less) => Decimal128::NEG_ONE,
                Some(core::cmp::Ordering::Equal) => Decimal128::ZERO,
                Some(core::cmp::Ordering::Greater) => Decimal128::ONE,
            };
            Some(OpResult::Value(v, s))
        }
        OpKind::CompareSig => {
            let a = parse_value(&operands[0], rm, enc)?.0;
            let b = parse_value(&operands[1], rm, enc)?.0;
            // `compareSignaling` differs from `compare` only in the
            // status: a quiet NaN raises INVALID here too. The result
            // value is the same propagated NaN (the GDA NaN-propagation
            // rule, reusing `add`) on an unordered comparison.
            let (ord, s) = a.compare_signaling(b);
            let v = match ord {
                None => a.add(b, RoundingMode::NearestEven).0,
                Some(core::cmp::Ordering::Less) => Decimal128::NEG_ONE,
                Some(core::cmp::Ordering::Equal) => Decimal128::ZERO,
                Some(core::cmp::Ordering::Greater) => Decimal128::ONE,
            };
            Some(OpResult::Value(v, s))
        }
        OpKind::CompareTotal => {
            let a = parse_value(&operands[0], rm, enc)?.0;
            let b = parse_value(&operands[1], rm, enc)?.0;
            let v = match a.total_cmp(b) {
                core::cmp::Ordering::Less => Decimal128::NEG_ONE,
                core::cmp::Ordering::Equal => Decimal128::ZERO,
                core::cmp::Ordering::Greater => Decimal128::ONE,
            };
            Some(OpResult::Value(v, Status::OK))
        }
        OpKind::Min => {
            let a = parse_value(&operands[0], rm, enc)?.0;
            let b = parse_value(&operands[1], rm, enc)?.0;
            let (v, s) = a.min(b);
            Some(OpResult::Value(v, s))
        }
        OpKind::Max => {
            let a = parse_value(&operands[0], rm, enc)?.0;
            let b = parse_value(&operands[1], rm, enc)?.0;
            let (v, s) = a.max(b);
            Some(OpResult::Value(v, s))
        }
        OpKind::Class => {
            let a = parse_value(&operands[0], rm, enc)?.0;
            Some(OpResult::Class(classify_to_gda_name(a)))
        }
        OpKind::Quantize => {
            let a = parse_value(&operands[0], rm, enc)?.0;
            let b = parse_value(&operands[1], rm, enc)?.0;
            let (v, s) = a.quantize(b, rm);
            Some(OpResult::Value(v, s))
        }
        OpKind::SameQuantum => {
            let a = parse_value(&operands[0], rm, enc)?.0;
            let b = parse_value(&operands[1], rm, enc)?.0;
            let same = a.same_quantum(b);
            // decTest represents the boolean result as "1" or "0".
            let v = if same {
                Decimal128::ONE
            } else {
                Decimal128::ZERO
            };
            Some(OpResult::Value(v, Status::OK))
        }
        OpKind::ScaleB => {
            let a = parse_value(&operands[0], rm, enc)?.0;
            let n_dec = parse_value(&operands[1], rm, enc)?.0;
            // Validate the second operand per GDA §5.3:
            //  • sNaN in either operand    → quiet NaN + INVALID
            //  • qNaN in either operand    → propagate
            //  • Inf as second operand     → NaN + INVALID
            //  • non-integer quantum (i.e. biased_exp != BIAS) → NaN + INVALID
            // Magnitude bound (|n| > 12356) is enforced by `scaleb` itself.
            if a.is_nan() || n_dec.is_nan() {
                // Propagate the NaN operand the same way the arithmetic
                // kernels do (sNaN-priority, sign and payload preserved,
                // signaling quietened). `add` is a convenient vehicle
                // for that rule; the status carries INVALID iff a
                // signaling NaN was present.
                let v = a.add(n_dec, RoundingMode::NearestEven).0;
                let status = if a.is_signaling_nan() || n_dec.is_signaling_nan() {
                    Status::INVALID
                } else {
                    Status::OK
                };
                return Some(OpResult::Value(v, status));
            }
            if n_dec.is_infinite() {
                return Some(OpResult::Value(Decimal128::NAN, Status::INVALID));
            }
            // Integer-quantum check: same_quantum against ONE (which has
            // biased_exp = BIAS). Anything with a different quantum (e.g.
            // 1.00, 1E+1, 0.5) is non-integer per the spec.
            if !n_dec.same_quantum(Decimal128::ONE) {
                return Some(OpResult::Value(Decimal128::NAN, Status::INVALID));
            }
            // to_i32 saturates on overflow; the saturated value's magnitude
            // is well above 12356, so scaleb's bound check catches it.
            let (n_i32, _) = n_dec.to_i32(RoundingMode::TowardZero);
            let (v, s) = a.scaleb(n_i32, rm);
            Some(OpResult::Value(v, s))
        }
        OpKind::LogB => {
            let a = parse_value(&operands[0], rm, enc)?.0;
            let (v, s) = a.logb();
            Some(OpResult::Value(v, s))
        }
        OpKind::NextPlus => {
            let a = parse_value(&operands[0], rm, enc)?.0;
            let (v, s) = a.next_up();
            Some(OpResult::Value(v, s))
        }
        OpKind::NextMinus => {
            let a = parse_value(&operands[0], rm, enc)?.0;
            let (v, s) = a.next_down();
            Some(OpResult::Value(v, s))
        }
        OpKind::CompareTotalMag => {
            let a = parse_value(&operands[0], rm, enc)?.0;
            let b = parse_value(&operands[1], rm, enc)?.0;
            let v = match a.compare_total_magnitude(b) {
                core::cmp::Ordering::Less => Decimal128::NEG_ONE,
                core::cmp::Ordering::Equal => Decimal128::ZERO,
                core::cmp::Ordering::Greater => Decimal128::ONE,
            };
            Some(OpResult::Value(v, Status::OK))
        }
        OpKind::Reduce => {
            let a = parse_value(&operands[0], rm, enc)?.0;
            let (v, s) = a.reduce();
            Some(OpResult::Value(v, s))
        }
        OpKind::DivideInteger => {
            let a = parse_value(&operands[0], rm, enc)?.0;
            let b = parse_value(&operands[1], rm, enc)?.0;
            let (v, s) = a.divide_integer(b);
            Some(OpResult::Value(v, s))
        }
        OpKind::LogicalInvert => {
            let a = parse_value(&operands[0], rm, enc)?.0;
            let (v, s) = a.logical_invert();
            Some(OpResult::Value(v, s))
        }
        OpKind::LogicalAnd => {
            let a = parse_value(&operands[0], rm, enc)?.0;
            let b = parse_value(&operands[1], rm, enc)?.0;
            let (v, s) = a.logical_and(b);
            Some(OpResult::Value(v, s))
        }
        OpKind::LogicalOr => {
            let a = parse_value(&operands[0], rm, enc)?.0;
            let b = parse_value(&operands[1], rm, enc)?.0;
            let (v, s) = a.logical_or(b);
            Some(OpResult::Value(v, s))
        }
        OpKind::LogicalXor => {
            let a = parse_value(&operands[0], rm, enc)?.0;
            let b = parse_value(&operands[1], rm, enc)?.0;
            let (v, s) = a.logical_xor(b);
            Some(OpResult::Value(v, s))
        }
        OpKind::Shift => {
            let a = parse_value(&operands[0], rm, enc)?.0;
            let b = parse_value(&operands[1], rm, enc)?.0;
            let (v, s) = a.shift(b);
            Some(OpResult::Value(v, s))
        }
        OpKind::Rotate => {
            let a = parse_value(&operands[0], rm, enc)?.0;
            let b = parse_value(&operands[1], rm, enc)?.0;
            let (v, s) = a.rotate(b);
            Some(OpResult::Value(v, s))
        }
    }
}

enum OpResult {
    Value(Decimal128, Status),
    Class(String),
}

/// Run an op under decTest's directional `up` rounding (away from zero).
///
/// Strategy: round once with `TowardZero` to recover the sign of the
/// exact result without losing it to a sign-of-zero ambiguity, then
/// dispatch to whichever directional IEEE mode rounds magnitude up
/// for that sign — `TowardPositive` for non-negative, `TowardNegative`
/// for negative.
///
/// If the first pass is already exact, both modes would agree, so we
/// short-circuit. The IEEE 754 sign-of-zero rule means `TowardZero`
/// returns `+0` for cancellation results (only `TowardNegative` gives
/// `-0`), so the sign check correctly steers zero-result cases to
/// `TowardPositive`, matching `up`'s convention.
fn invoke_up(op: OpKind, operands: &[String], enc: Encoding) -> Option<OpResult> {
    let probe = invoke(op, operands, RoundingMode::TowardZero, enc)?;
    let (val, status) = match probe {
        OpResult::Value(v, s) => (v, s),
        OpResult::Class(_) => return Some(probe),
    };
    if !status.inexact() {
        return Some(OpResult::Value(val, status));
    }
    let mode = if val.is_sign_negative() {
        RoundingMode::TowardNegative
    } else {
        RoundingMode::TowardPositive
    };
    invoke(op, operands, mode, enc)
}

fn parse_value(s: &str, rm: RoundingMode, enc: Encoding) -> Option<(Decimal128, Status)> {
    let trimmed = s.trim();
    // decTest's `#` syntax encodes the operand as a raw 128-bit hex
    // literal. Default interpretation is BID (the runner's existing
    // contract; up to 32 hex chars, shorter inputs zero-extended by
    // `u128::from_str_radix`). Files vendored as DPD interchange
    // tests (dqEncode, dqCanonical) decode via
    // `Decimal128::from_dpd_bytes`, which requires exactly 32 hex
    // chars in big-endian order.
    //
    // Bare `#` (no hex chars) is the "null test" sentinel — handled by
    // `run_null_test` upstream via a case-level short-circuit, so it
    // never reaches here.
    if let Some(hex) = trimmed.strip_prefix('#') {
        return match enc {
            Encoding::Bid => {
                let bits = u128::from_str_radix(hex, 16).ok()?;
                Some((Decimal128::from_bits(bits), Status::OK))
            }
            Encoding::Dpd => parse_dpd_hex(hex),
        };
    }
    Decimal128::parse_str(trimmed, rm).ok()
}

#[cfg(feature = "dpd")]
fn parse_dpd_hex(hex: &str) -> Option<(Decimal128, Status)> {
    if hex.len() != 32 {
        return None;
    }
    let mut bytes = [0u8; 16];
    for (i, byte) in bytes.iter_mut().enumerate() {
        let chunk = hex.get(2 * i..2 * i + 2)?;
        *byte = u8::from_str_radix(chunk, 16).ok()?;
    }
    Some((Decimal128::from_dpd_bytes(bytes), Status::OK))
}

#[cfg(not(feature = "dpd"))]
fn parse_dpd_hex(_hex: &str) -> Option<(Decimal128, Status)> {
    // Without the `dpd` feature, files containing DPD `#hex` operands
    // are skipped at the per-file gate in `run_file`, so this branch
    // is unreachable. The function exists only to keep `parse_value`'s
    // match exhaustive across feature flags.
    None
}

/// Synthesize the dec-spec answer for a "null operand" case (bare `#`):
/// `(NaN, Invalid_operation)`. Compares against the case's expected
/// value/flags via the regular comparator. The compare path treats the
/// expected payload as "may differ" for NaN (the dec spec doesn't pin
/// payloads on parse-failure NaN), so the only assertion against the
/// case is that the expected is NaN-shaped and the conditions list
/// names `invalid_operation`.
fn run_null_test(case: &TestCase, ctx: &Context) -> Outcome {
    // Class results never carry `#` operands in the corpus, so the
    // class-string short-circuit doesn't apply here.
    let synth = OpResult::Value(Decimal128::NAN, Status::INVALID);
    let (expected, _) = match parse_value(&case.expected, ctx.rounding.for_parse(), ctx.encoding) {
        Some(v) => v,
        None => return Outcome::Fail(format!("couldn't parse expected {:?}", case.expected)),
    };
    let expected_flags = expected_status(&case.conditions);
    compare(case, &synth, expected, expected_flags)
}

/// Render a `Decimal128` as the GDA `class` op's expected string form.
///
/// IEEE 754 categories plus sign for the finite / infinite cases; bare
/// `NaN` / `sNaN` (no sign — GDA collapses NaN sign in the class name).
fn classify_to_gda_name(d: Decimal128) -> String {
    if d.is_signaling_nan() {
        return "sNaN".to_string();
    }
    if d.is_nan() {
        return "NaN".to_string();
    }
    let sign = if d.is_sign_negative() { "-" } else { "+" };
    let kind = if d.is_infinite() {
        "Infinity"
    } else if d.is_zero() {
        "Zero"
    } else if d.is_subnormal() {
        "Subnormal"
    } else {
        "Normal"
    };
    format!("{sign}{kind}")
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
                s |= Status::INVALID;
            }
            // §7.4 CLAMPED is now compared (fd-61r / ADR-0048): ferrodec
            // raises it at every detectable in-operation clamp site.
            "clamped" => s |= Status::CLAMPED,
            // Ignore: rounded, subnormal, lost_digits, conversion_syntax
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
            // Class results are short-circuited in `run_case` before
            // they reach this comparator (the expected isn't a
            // Decimal128 literal we can re-parse). Reachable only via
            // a future refactor accident.
            unreachable!("class results are compared in run_case");
        }
        OpResult::Value(actual, actual_flags) => {
            // NaN compare: both must be NaN, with the same sign,
            // signaling bit, and diagnostic payload. decTest pins all
            // three on NaN-result vectors (the propagated-payload
            // first-NaN-wins / sNaN-priority rule, IEEE 754-2019
            // §6.2.3), and the parent renders sign / sNaN-vs-NaN /
            // payload through `Display` (the 1.9.0 NaN-with-payload
            // support), so the `Display` string is the exact projection
            // onto the fields the spec pins for a NaN: comparing it
            // catches a flipped sign or a mispropagated payload that the
            // old `is_nan()`-only check let through. The informational
            // §7.4 CLAMPED flag stays masked in the status comparison
            // below (fd-61r), coherent with the sibling runners; this
            // tightens only the NaN *value* side.
            if expected.is_nan() {
                if !actual.is_nan() {
                    return Outcome::Fail(format!(
                        "expected NaN, got {} ({:032X})",
                        actual,
                        actual.to_bits()
                    ));
                }
                let (got, want) = (format!("{actual}"), format!("{expected}"));
                if got != want {
                    return Outcome::Fail(format!(
                        "NaN sign/payload mismatch: got {got} ({:032X}), want {want} ({:032X})",
                        actual.to_bits(),
                        expected.to_bits()
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

            // Status flags. The five IEEE 754 exceptions are compared
            // strictly via `mask_status`. The §7.4 informational CLAMPED is
            // also compared now (fd-61r / ADR-0048), except for the
            // BID-structural residual: when an operand was itself clamped
            // into its cohort at parse, BID cannot store the operand's
            // pre-clamp exponent, so the operation cannot reconstruct the
            // ideal exponent decNumber raises CLAMPED from. Those cases are
            // skipped (tallied under the documented structural-CLAMPED
            // category in KNOWN_ISSUES.md), not failed.
            let actual_ieee = mask_status(*actual_flags);
            let expected_ieee = mask_status(expected_flags);
            if actual_ieee.bits() != expected_ieee.bits() {
                return Outcome::Fail(format!(
                    "status mismatch (op {}): got {:#x}, want {:#x} from conditions {:?}",
                    case.op,
                    actual_ieee.bits(),
                    expected_ieee.bits(),
                    case.conditions,
                ));
            }
            if actual_flags.clamped() != expected_flags.clamped() {
                if expected_flags.clamped()
                    && !actual_flags.clamped()
                    && any_operand_clamped_at_parse(&case.operands)
                {
                    return Outcome::Skip;
                }
                return Outcome::Fail(format!(
                    "CLAMPED mismatch (op {}): got {}, want {} from conditions {:?}",
                    case.op,
                    actual_flags.clamped(),
                    expected_flags.clamped(),
                    case.conditions,
                ));
            }
            Outcome::Pass
        }
    }
}

fn mask_status(s: Status) -> Status {
    // Compare only the five IEEE 754 exception flags. The informational
    // GDA conditions (CLAMPED, ROUNDED, SUBNORMAL, LOST_DIGITS) are not
    // part of IEEE 754 conformance: `decode_conditions` already drops
    // them from the expected side, and ferrodec may emit the §7.4
    // informational CLAMPED at genuine clamp sites where the dec corpus
    // also pins it (the remaining ideal-exponent cases are deferred,
    // fd-61r). Masking both sides keeps the comparison consistent and
    // matches the sibling runners' `status_conformance_eq`.
    let ieee = Status::INVALID
        | Status::DIV_BY_ZERO
        | Status::OVERFLOW
        | Status::UNDERFLOW
        | Status::INEXACT;
    Status::from_bits_truncate(s.bits() & ieee.bits())
}

/// `true` when any operand of a decTest case is itself clamped at parse,
/// i.e. its literal quantum exceeds the format range so BID stores it in a
/// padded cohort. Such an operand has lost its pre-clamp exponent, so the
/// downstream operation cannot reconstruct the wide ideal exponent
/// decNumber raises §7.4 CLAMPED from. These cases are the documented
/// BID-structural CLAMPED residual (fd-61r / ADR-0048); the comparator
/// skips them rather than failing.
fn any_operand_clamped_at_parse(operands: &[String]) -> bool {
    operands.iter().any(|op| {
        Decimal128::parse_str(op, RoundingMode::NearestEven)
            .is_ok_and(|(_, status)| status.clamped())
    })
}

/// Per-file expected `passed` count for the decTest run.
///
/// Asserted exactly (not as a floor) so that any silent trade-off
/// — e.g. a refactor that adds 4 passes in one file while regressing
/// 4 in another — fails the test instead of slipping through. ADR-0010
/// covers the design rationale.
///
/// When making a change that legitimately moves a count, edit the
/// corresponding row here in the same commit. The diff then surfaces
/// the trade-off explicitly in code review.
///
/// `dqEncode` and `dqCanonical` are gated on the `dpd` feature; the
/// runner records them as 0 / 0 / 0 when the feature is off so the
/// table below stays a single source of truth across both feature
/// configurations.
fn expected_per_file() -> &'static [(&'static str, usize)] {
    #[cfg(not(feature = "dpd"))]
    {
        &[
            ("dqAbs.decTest", 75),
            ("dqAdd.decTest", 1004),
            // fd-ci0.7 (ADR-0031): logical and/or/xor wired. All
            // 357/341/348 cases pass on first run.
            ("dqAnd.decTest", 357),
            ("dqCanonical.decTest", 0),
            ("dqClass.decTest", 42),
            ("dqCompare.decTest", 659),
            // fd-bef.1 (ADR-0049): `compareSignaling` wired. All 559
            // cases pass; like `compare` but every NaN raises INVALID.
            ("dqCompareSig.decTest", 559),
            ("dqCompareTotal.decTest", 613),
            ("dqCompareTotalMag.decTest", 613),
            ("dqDivide.decTest", 683),
            // fd-ci0.5 (ADR-0031): `divideInteger` wired. All 374
            // cases pass on first run.
            ("dqDivideInt.decTest", 374),
            ("dqEncode.decTest", 0),
            ("dqFMA.decTest", 1418),
            // fd-ci0.6 (ADR-0031): `logical_invert` wired. All 193
            // cases pass on first run after the qNaN-as-INVALID fix.
            ("dqInvert.decTest", 193),
            ("dqLogB.decTest", 109),
            ("dqMax.decTest", 257),
            ("dqMin.decTest", 247),
            ("dqMinus.decTest", 43),
            ("dqMultiply.decTest", 473),
            ("dqNextMinus.decTest", 84),
            ("dqNextPlus.decTest", 84),
            // fd-ci0.7 (ADR-0031): `logical_or`.
            ("dqOr.decTest", 341),
            ("dqQuantize.decTest", 622),
            // fd-ci0.4 (ADR-0031): `reduce` wired. All 134 cases pass
            // on first run; no `#`-hex skips in the dq* counterpart.
            ("dqReduce.decTest", 134),
            ("dqRemainderNear.decTest", 521),
            // fd-ci0.9 (ADR-0031): `rotate`. All 248 cases pass.
            ("dqRotate.decTest", 248),
            ("dqSameQuantum.decTest", 333),
            ("dqScaleB.decTest", 202),
            // fd-ci0.8 (ADR-0031): `shift`. All 248 cases pass.
            ("dqShift.decTest", 248),
            ("dqSubtract.decTest", 520),
            // fd-ci0.7 (ADR-0031): `logical_xor`.
            ("dqXor.decTest", 348),
        ]
    }
    #[cfg(feature = "dpd")]
    {
        &[
            ("dqAbs.decTest", 75),
            ("dqAdd.decTest", 1004),
            // fd-ci0.7 (ADR-0031): logical and/or/xor wired;
            // encoding-independent.
            ("dqAnd.decTest", 357),
            // fd-bef.1 (ADR-0049): rises 90 -> 95 as the 5 `comparesig`
            // cases in this mixed-op DPD file now dispatch via
            // `compare_signaling` (the other 149 skips are GDA bit ops
            // not yet dispatched on the DPD path, e.g. copy / logical /
            // nexttoward).
            ("dqCanonical.decTest", 95),
            ("dqClass.decTest", 42),
            ("dqCompare.decTest", 659),
            // fd-bef.1 (ADR-0049): `compareSignaling` wired. All 559
            // cases pass; like `compare` but every NaN raises INVALID.
            ("dqCompareSig.decTest", 559),
            ("dqCompareTotal.decTest", 613),
            ("dqCompareTotalMag.decTest", 613),
            ("dqDivide.decTest", 683),
            // fd-ci0.5 (ADR-0031): `divideInteger` wired. All 374
            // cases pass on first run; encoding-independent.
            ("dqDivideInt.decTest", 374),
            ("dqEncode.decTest", 368),
            ("dqFMA.decTest", 1418),
            // fd-ci0.6 (ADR-0031): `logical_invert` wired. 193 cases
            // pass; encoding-independent.
            ("dqInvert.decTest", 193),
            ("dqLogB.decTest", 109),
            ("dqMax.decTest", 257),
            ("dqMin.decTest", 247),
            ("dqMinus.decTest", 43),
            ("dqMultiply.decTest", 473),
            ("dqNextMinus.decTest", 84),
            ("dqNextPlus.decTest", 84),
            // fd-ci0.7 (ADR-0031): `logical_or`.
            ("dqOr.decTest", 341),
            ("dqQuantize.decTest", 622),
            // fd-ci0.4 (ADR-0031): `reduce` wired. 134 of 134 pass on
            // first run, same as the non-dpd build (the op is encoding-
            // independent).
            ("dqReduce.decTest", 134),
            ("dqRemainderNear.decTest", 521),
            // fd-ci0.9 (ADR-0031): `rotate`. Encoding-independent.
            ("dqRotate.decTest", 248),
            ("dqSameQuantum.decTest", 333),
            ("dqScaleB.decTest", 202),
            // fd-ci0.8 (ADR-0031): `shift`. Encoding-independent.
            ("dqShift.decTest", 248),
            ("dqSubtract.decTest", 520),
            // fd-ci0.7 (ADR-0031): `logical_xor`.
            ("dqXor.decTest", 348),
        ]
    }
}

// ---------------------------------------------------------------------------
// Runner self-tests
//
// The conformance runner has a comparator (`compare`) whose NaN arm
// treats any expected-NaN as matching any actual-NaN. That's correct
// per the dec spec (most ops don't pin NaN payloads), but it admits a
// silent-pass class if `parse_value` ever returns a NaN sentinel from
// genuinely unparsable input. The tests below pin the parser's
// rejection behaviour so a future change can't silently weaken the
// runner's discrimination.

#[test]
fn parse_value_rejects_garbage_expected() {
    // Truly malformed tokens: parse_value returns None, which routes
    // to Outcome::Fail at the call site (line ~525). A future parser
    // change that degraded garbage to a NaN sentinel would break the
    // runner's ability to distinguish "computation produced a NaN"
    // from "expected token is corrupt".
    assert!(parse_value("garbage", RoundingMode::NearestEven, Encoding::Bid).is_none());
    assert!(parse_value("3.14xyz", RoundingMode::NearestEven, Encoding::Bid).is_none());
    assert!(parse_value("NaNxyz", RoundingMode::NearestEven, Encoding::Bid).is_none());
    assert!(parse_value("", RoundingMode::NearestEven, Encoding::Bid).is_none());
    // Bare # is the null-test sentinel handled upstream; with junk hex
    // it should refuse.
    assert!(parse_value("#zzzz", RoundingMode::NearestEven, Encoding::Bid).is_none());
}

#[test]
fn parse_value_accepts_valid_special_payloads() {
    // Positive control: payload-bearing NaN tokens parse cleanly so
    // the rejection above is genuinely about garbage, not about
    // collateral NaN intolerance.
    let (qnan, _) = parse_value("NaN31", RoundingMode::NearestEven, Encoding::Bid).unwrap();
    assert!(qnan.is_nan() && !qnan.is_signaling_nan());
    assert_eq!(qnan.to_bits() & ((1u128 << 110) - 1), 31);

    let (snan, _) = parse_value("sNaN7", RoundingMode::NearestEven, Encoding::Bid).unwrap();
    assert!(snan.is_signaling_nan());
    assert_eq!(snan.to_bits() & ((1u128 << 110) - 1), 7);

    let (inf, _) = parse_value("Infinity", RoundingMode::NearestEven, Encoding::Bid).unwrap();
    assert!(inf.is_infinite() && !inf.is_sign_negative());
}
