//! General Decimal Arithmetic `decTest` conformance for `ferrodec-decimal`.
//!
//! The vendored vectors under `tests/vectors/` are the *general* (precision
//! driven) files from Mike Cowlishaw's testcase suite: each file sets its own
//! `precision`, `maxExponent`, `minExponent`, `rounding`, and `clamp` through
//! in-file directives, which the runner reads into a per-file
//! [`ferrodec_decimal::Context`]. This is the independent, spec-authored cross
//! check of the operations the randomized libmpdec differential already covers.
//!
//! Unlike the fixed-format sibling harnesses this runner does not reuse the
//! shared `ferrodec-test-support` driver: that one tracks neither `clamp` nor
//! the three General Decimal Arithmetic rounding modes (`half_down`, `up`,
//! `05up`), and it masks the `Clamped` flag. An arbitrary-precision crate
//! exercises all of those, so the runner is bespoke. The line parser is the
//! same small state machine the workspace-root runner uses; it is copied rather
//! than shared to keep this crate's dependency graph minimal.
//!
//! Comparison is cohort exact: the result must equal the expected value under
//! `Decimal`'s representation equality (so `1.0` and `1.00` differ), and the
//! status flags must match exactly, `Clamped` and `Underflow` included. Two
//! operations relax this. `toSci` / `toEng` compare the rendered string
//! directly, since they are the test of the formatting itself. `power` is
//! compared within a one-ulp band, because this crate's `power` is correctly
//! rounded by construction while the spec's reference is only "almost always"
//! correctly rounded (see `tests/pow_oracle.rs`).

#![cfg(feature = "fmt")]

use std::fs;
use std::path::{Path, PathBuf};

use ferrodec_decimal::{Context, DecBig, Decimal, ParseDecimalError, Rounding, Status};

const VECTORS_DIR: &str = "tests/vectors";

/// Per-file expected pass counts (ADR-0010 record-then-pin). Each row is a
/// deliberate baseline: a legitimate increase is a visible one-line edit, and a
/// silent cross-file trade-off becomes a hard failure.
fn expected_per_file() -> &'static [(&'static str, usize)] {
    &[
        ("abs.decTest", 89),
        ("add.decTest", 2100),
        ("and.decTest", 279),
        // 1152 -> 1154: the quote-aware strip_comment fix recovered the
        // adversarial parse vectors basx504 ('--1') and basx555 ('1E--1'),
        // which a position-blind comment cut had silently dropped (fd-aqs.9).
        ("base.decTest", 1154),
        ("clamp.decTest", 111),
        ("class.decTest", 84),
        ("compare.decTest", 639),
        ("comparesig.decTest", 625),
        ("comparetotal.decTest", 670),
        ("comparetotmag.decTest", 664),
        ("copy.decTest", 43),
        ("copyabs.decTest", 43),
        ("copynegate.decTest", 43),
        ("copysign.decTest", 111),
        ("divide.decTest", 631),
        ("divideint.decTest", 389),
        ("exp.decTest", 436),
        ("fma.decTest", 2612),
        ("inexact.decTest", 145),
        ("invert.decTest", 128),
        ("ln.decTest", 410),
        ("log10.decTest", 385),
        ("logb.decTest", 128),
        ("max.decTest", 328),
        ("maxmag.decTest", 313),
        ("min.decTest", 317),
        ("minmag.decTest", 303),
        ("minus.decTest", 113),
        ("multiply.decTest", 521),
        ("nextminus.decTest", 104),
        ("nextplus.decTest", 106),
        ("nexttoward.decTest", 341),
        ("or.decTest", 276),
        ("plus.decTest", 122),
        ("power.decTest", 1197),
        ("powersqrt.decTest", 2856),
        ("quantize.decTest", 742),
        ("reduce.decTest", 168),
        ("remainder.decTest", 517),
        ("remaindernear.decTest", 446),
        ("rotate.decTest", 195),
        ("rounding.decTest", 1030),
        ("samequantum.decTest", 333),
        ("scaleb.decTest", 155),
        ("shift.decTest", 200),
        ("squareroot.decTest", 3586),
        ("subtract.decTest", 681),
        ("tointegral.decTest", 168),
        ("tointegralx.decTest", 180),
        ("xor.decTest", 277),
    ]
}

#[test]
fn dectest_conformance() {
    let mut totals = (0usize, 0usize, 0usize); // pass, fail, skip
    let mut failures: Vec<Failure> = Vec::new();
    let mut file_results: Vec<(String, usize)> = Vec::new();

    let mut paths: Vec<PathBuf> = fs::read_dir(VECTORS_DIR)
        .expect("vectors directory")
        .filter_map(std::result::Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("decTest"))
        .collect();
    paths.sort();

    let mut total_unparsed = 0;
    for path in &paths {
        let (passed, failed, skipped, unparsed) = run_file(path, &mut failures);
        totals.0 += passed;
        totals.1 += failed;
        totals.2 += skipped;
        total_unparsed += unparsed;
        eprintln!(
            "{:<24}  {passed:>5} pass  {failed:>4} fail  {skipped:>5} skip",
            path.file_name().unwrap().to_string_lossy(),
        );
        file_results.push((
            path.file_name().unwrap().to_string_lossy().into_owned(),
            passed,
        ));
    }

    eprintln!(
        "\nTOTAL: {} cases — {} pass, {} fail, {} skip, {} unparsed",
        totals.0 + totals.1 + totals.2,
        totals.0,
        totals.1,
        totals.2,
        total_unparsed,
    );

    // Pinned at zero: every non-empty, non-directive line must tokenise,
    // so a parser regression cannot drop cases silently (fd-aqs.9).
    assert_eq!(
        total_unparsed, 0,
        "{total_unparsed} non-directive lines failed to parse (see UNPARSED lines above)"
    );

    if !failures.is_empty() {
        eprintln!("\nFirst 400 failures (of {}):", failures.len());
        for f in failures.iter().take(400) {
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
    for (name, exp) in expected_per_file() {
        let got = file_results
            .iter()
            .find(|(n, _)| n == name)
            .map_or(0, |(_, p)| *p);
        if got != *exp {
            mismatch.push(((*name).to_string(), *exp, got));
        }
    }
    if !mismatch.is_empty() {
        eprintln!("\nPer-file pass-count mismatch (update expected_per_file, ADR-0010):");
        for (name, exp, got) in &mismatch {
            eprintln!("  {name:<24}  expected {exp}  got {got}");
        }
        panic!(
            "conformance per-file expectation mismatch ({})",
            mismatch.len()
        );
    }

    assert_eq!(totals.1, 0, "conformance failures: {}", totals.1);
}

// ---------------------------------------------------------------------------
// Per-file driver

struct Failure {
    file: PathBuf,
    line: usize,
    id: String,
    reason: String,
}

fn run_file(path: &Path, failures: &mut Vec<Failure>) -> (usize, usize, usize, usize) {
    let content = fs::read_to_string(path).expect("read file");
    let mut ctx = DirCtx::default();
    let (mut passed, mut failed, mut skipped, mut unparsed) = (0, 0, 0, 0);

    for (line_no, raw) in content.lines().enumerate() {
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if let Some((name, value)) = parse_directive(line) {
            ctx.apply(&name, &value);
            continue;
        }
        let Some(case) = parse_test_case(line) else {
            unparsed += 1;
            eprintln!(
                "UNPARSED {}:{}: {raw}",
                path.file_name().unwrap_or_default().to_string_lossy(),
                line_no + 1,
            );
            continue;
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
    (passed, failed, skipped, unparsed)
}

// ---------------------------------------------------------------------------
// Directive-aware context

struct DirCtx {
    precision: u32,
    emax: i32,
    emin: i32,
    rounding: Rounding,
    clamp: bool,
}

impl Default for DirCtx {
    fn default() -> Self {
        // The general files set every directive before their first case; these
        // defaults only cover the (unused) pre-directive prologue.
        Self {
            precision: 9,
            emax: 384,
            emin: -383,
            rounding: Rounding::HalfEven,
            clamp: false,
        }
    }
}

impl DirCtx {
    fn apply(&mut self, name: &str, value: &str) {
        match name {
            "precision" => {
                if let Ok(v) = value.parse() {
                    self.precision = v;
                }
            }
            "maxexponent" => {
                if let Ok(v) = value.parse() {
                    self.emax = v;
                }
            }
            "minexponent" => {
                if let Ok(v) = value.parse() {
                    self.emin = v;
                }
            }
            "rounding" => {
                if let Some(r) = map_rounding(value) {
                    self.rounding = r;
                }
            }
            "clamp" => self.clamp = value == "1",
            _ => {}
        }
    }

    fn context(&self) -> Context {
        Context::new(
            core::num::NonZeroU32::new(self.precision).unwrap(),
            self.emax,
            self.emin,
            self.rounding,
        )
        .with_clamp(self.clamp)
    }
}

/// All eight General Decimal Arithmetic rounding modes map to this crate's
/// [`Rounding`]; there is no skip-for-rounding bucket (the advantage over the
/// fixed-format harnesses, which decline `half_down` / `up` / `05up`).
fn map_rounding(s: &str) -> Option<Rounding> {
    Some(match s {
        "half_even" => Rounding::HalfEven,
        "half_up" => Rounding::HalfUp,
        "half_down" => Rounding::HalfDown,
        "down" => Rounding::Down,
        "up" => Rounding::Up,
        "ceiling" => Rounding::Ceiling,
        "floor" => Rounding::Floor,
        "05up" => Rounding::ZeroFiveUp,
        _ => return None,
    })
}

// ---------------------------------------------------------------------------
// Case execution

enum Outcome {
    Pass,
    Skip,
    Fail(String),
}

fn run_case(case: &TestCase, dctx: &DirCtx) -> Outcome {
    let op = case.op.as_str();

    // A `#hex` or `NN#literal` operand or expected is a fixed-width encoding
    // literal (a decimal32/64/128 bit pattern, or a value tagged with the format
    // it was rounded in), which an arbitrary-precision value cannot reproduce.
    // A bare `#` operand is the null sentinel and is left to the parse path,
    // where it yields (NaN, Invalid_operation).
    if case.expected.contains('#') {
        return Outcome::Skip;
    }
    if case.operands.iter().any(|o| o.contains('#') && o != "#") {
        return Outcome::Skip;
    }
    // The transcendental files include cases that expect Invalid_context: a
    // precision or exponent bound beyond the reference's internal limits (a
    // precision of 100000000, a maxExponent of 1000000). That is an
    // implementation limit of the reference, not a spec-arithmetic result;
    // this crate places no such ceiling, so the operation succeeds. The suite
    // itself notes these are skippable by harnesses that do not model the
    // restriction.
    if case.conditions.iter().any(|c| c == "invalid_context") {
        return Outcome::Skip;
    }

    let ctx = dctx.context();

    if op == "tosci" {
        return run_tosci(case, &ctx);
    }
    if op == "toeng" {
        return run_toeng(case, &ctx);
    }
    if op == "class" {
        return run_class(case, &ctx);
    }

    // Parse operands exactly. A syntactically invalid (or bare `#` null)
    // operand yields (NaN, Invalid_operation) per the specification.
    let mut operands = Vec::with_capacity(case.operands.len());
    for raw in &case.operands {
        match Decimal::parse_str(raw) {
            Ok(d) => operands.push(d),
            // An exponent beyond i32 is outside this crate's representable range
            // (a deliberate bound), not a conversion-syntax error: skip it.
            Err(ParseDecimalError::ExponentOverflow) => return Outcome::Skip,
            Err(_) => {
                return compare(
                    case,
                    &Decimal::quiet_nan(false, DecBig::zero()),
                    Status::INVALID,
                )
            }
        }
    }

    // The reference restricts the transcendental functions' operands to an
    // adjusted exponent in decNumber's `DEC_MAX_MATH` range (at most `999999`,
    // and for the exponent operand down to `-1999997`); an operand outside it
    // raises Invalid_operation. This crate places no such ceiling and computes
    // the mathematically correct result within its `i32` exponent, so a case
    // whose only reason for expecting Invalid_operation is an out-of-range
    // operand is skipped. An in-range operand whose value is genuinely valid
    // (the surrounding "operand range" cases that return a real result) still
    // runs and is compared.
    if matches!(op, "exp" | "ln" | "log10" | "power")
        && case.conditions.iter().any(|c| c == "invalid_operation")
        && operands.iter().any(|d| {
            d.finite_parts().is_some_and(|(_, coeff, exp)| {
                let adj = i64::from(exp) + coeff.decimal_digit_count() as i64 - 1;
                !(-1_999_997..=999_999).contains(&adj)
            })
        })
    {
        return Outcome::Skip;
    }

    let (res, status) = match op {
        "add" => operands[0].add(&operands[1], &ctx),
        "subtract" => operands[0].subtract(&operands[1], &ctx),
        "multiply" => operands[0].multiply(&operands[1], &ctx),
        "divide" => operands[0].divide(&operands[1], &ctx),
        "divideint" => operands[0].divide_integer(&operands[1], &ctx),
        "remainder" => operands[0].remainder(&operands[1], &ctx),
        "remaindernear" => operands[0].remainder_near(&operands[1], &ctx),
        "fma" => operands[0].fma(&operands[1], &operands[2], &ctx),
        "squareroot" => operands[0].sqrt(&ctx),
        "quantize" => operands[0].quantize(&operands[1], &ctx),
        "tointegral" => operands[0].round_to_integral_value(&ctx),
        "tointegralx" => operands[0].round_to_integral_exact(&ctx),
        "reduce" => operands[0].reduce(&ctx),
        "plus" => operands[0].plus(&ctx),
        "minus" => operands[0].minus(&ctx),
        "abs" => operands[0].abs(&ctx),
        "apply" => apply(&operands[0], &ctx),
        "compare" => operands[0].compare(&operands[1], &ctx),
        "comparetotal" => (operands[0].compare_total(&operands[1]), Status::OK),
        "max" => operands[0].max(&operands[1], &ctx),
        "min" => operands[0].min(&operands[1], &ctx),
        "copyabs" => (operands[0].copy_abs(), Status::OK),
        "copynegate" => (operands[0].copy_negate(), Status::OK),
        "copysign" => (operands[0].copy_sign(&operands[1]), Status::OK),
        "copy" => (operands[0].copy(), Status::OK),
        "comparesig" => operands[0].compare_signal(&operands[1], &ctx),
        "comparetotmag" => (operands[0].compare_total_mag(&operands[1]), Status::OK),
        "maxmag" => operands[0].max_magnitude(&operands[1], &ctx),
        "minmag" => operands[0].min_magnitude(&operands[1], &ctx),
        "samequantum" => (operands[0].same_quantum(&operands[1]), Status::OK),
        "and" => operands[0].and(&operands[1], &ctx),
        "or" => operands[0].or(&operands[1], &ctx),
        "xor" => operands[0].xor(&operands[1], &ctx),
        "invert" => operands[0].invert(&ctx),
        "shift" => operands[0].shift(&operands[1], &ctx),
        "rotate" => operands[0].rotate(&operands[1], &ctx),
        "scaleb" => operands[0].scaleb(&operands[1], &ctx),
        "logb" => operands[0].logb(&ctx),
        "nextplus" => operands[0].next_plus(&ctx),
        "nextminus" => operands[0].next_minus(&ctx),
        "nexttoward" => operands[0].next_toward(&operands[1], &ctx),
        "exp" => operands[0].exp(&ctx),
        "ln" => operands[0].ln(&ctx),
        "log10" => operands[0].log10(&ctx),
        "power" => operands[0].power(&operands[1], &ctx),
        _ => return Outcome::Skip,
    };

    // `power` is correctly rounded by construction, while the spec's reference
    // is only "almost always" correctly rounded, so it is compared within a
    // one-ulp band (the same allowance the differential makes); every other
    // operation is cohort-exact.
    if op == "power" {
        compare_power(case, &res, status)
    } else {
        compare(case, &res, status)
    }
}

/// Compare a `power` result, allowing the reference to differ by up to one unit
/// in the last place (this crate's `power` is correctly rounded; the reference
/// is not always). An exact cohort-and-flag match always passes; otherwise two
/// finite results within one ulp pass with their flags allowed to differ (a
/// zero versus the smallest subnormal at the underflow boundary).
fn compare_power(case: &TestCase, res: &Decimal, status: Status) -> Outcome {
    match compare(case, res, status) {
        Outcome::Pass => Outcome::Pass,
        other => {
            let Ok(want) = Decimal::parse_str(&case.expected) else {
                return other;
            };
            if res.is_finite() && want.is_finite() && within_one_ulp(res, &want) {
                Outcome::Pass
            } else {
                other
            }
        }
    }
}

/// Whether two finite values are within one unit in the last place.
fn within_one_ulp(a: &Decimal, b: &Decimal) -> bool {
    let wide = Context::new(
        core::num::NonZeroU32::new(60).unwrap(),
        1_000_000,
        -1_000_000,
        Rounding::HalfEven,
    );
    let absdiff = a.subtract(b, &wide).0.abs(&wide).0;
    let ea = a.finite_parts().map_or(0, |p| p.2);
    let eb = b.finite_parts().map_or(0, |p| p.2);
    let ulp = Decimal::finite(false, DecBig::from_u32(1), ea.max(eb));
    let cmp = absdiff.compare(&ulp, &wide).0;
    cmp.is_zero() || cmp.is_negative()
}

/// decTest `apply`: round the operand to the context. This is `plus` for every
/// value except a zero, whose sign `plus` resolves through the add-from-zero
/// rule (so `+(-0)` becomes `+0`); `apply` instead preserves the operand's sign
/// while still rounding and clamping the exponent.
fn apply(d: &Decimal, ctx: &Context) -> (Decimal, Status) {
    let (mut r, s) = d.plus(ctx);
    if d.is_zero() {
        if let Some((_, coeff, exp)) = r.finite_parts() {
            r = Decimal::finite(d.is_negative(), coeff.clone(), exp);
        }
    }
    (r, s)
}

/// decTest `toSci`: render the operand in to-scientific notation (`Display`).
fn run_tosci(case: &TestCase, ctx: &Context) -> Outcome {
    run_to_string(case, ctx, "toSci", Decimal::to_string)
}

/// decTest `toEng`: render the operand in to-engineering notation.
fn run_toeng(case: &TestCase, ctx: &Context) -> Outcome {
    run_to_string(case, ctx, "toEng", Decimal::to_eng_string)
}

/// decTest `class`: classify the operand and compare the class string. The
/// operand is read exactly (no context rounding) and `class` never signals.
fn run_class(case: &TestCase, ctx: &Context) -> Outcome {
    match Decimal::parse_str(&case.operands[0]) {
        Ok(d) => {
            let got = d.class(ctx);
            if got == case.expected {
                Outcome::Pass
            } else {
                Outcome::Fail(format!("class: got {got} want {}", case.expected))
            }
        }
        Err(ParseDecimalError::ExponentOverflow) => Outcome::Skip,
        Err(_) => Outcome::Fail(format!("class: unparseable operand {:?}", case.operands[0])),
    }
}

/// Shared driver for the string-rendering operations. The operand is read
/// *under the context*: a finite value is rounded to the working precision and
/// exponent range (raising Inexact / Overflow / Underflow / Clamped as `apply`
/// does), then rendered by `render`; a special (Infinity / NaN / sNaN) passes
/// through unchanged and raises nothing, because reading a string never
/// signals, except that a NaN whose payload exceeds the precision is
/// `conversion_syntax`. The rendered string is compared directly, since these
/// operations are the test of the formatting itself.
fn run_to_string(
    case: &TestCase,
    ctx: &Context,
    label: &str,
    render: impl Fn(&Decimal) -> String,
) -> Outcome {
    match Decimal::parse_str(&case.operands[0]) {
        Ok(d) => {
            let (r, status) = if d.is_finite() {
                apply(&d, ctx)
            } else {
                // A NaN read under the context is conversion_syntax if its
                // payload exceeds the precision; an Infinity passes through.
                if let Some((_, _, payload)) = d.nan_parts() {
                    if payload.decimal_digit_count() > u64::from(ctx.precision.get()) {
                        return compare(
                            case,
                            &Decimal::quiet_nan(false, DecBig::zero()),
                            Status::INVALID,
                        );
                    }
                }
                (d, Status::OK)
            };
            let exp_status = expected_status(&case.conditions);
            if render(&r) == case.expected && status == exp_status {
                Outcome::Pass
            } else {
                Outcome::Fail(format!(
                    "{label}: got {} [{status:?}] want {:?} [{exp_status:?}]",
                    render(&r),
                    case.expected
                ))
            }
        }
        // An exponent beyond i32 is outside the representable range: skip.
        Err(ParseDecimalError::ExponentOverflow) => Outcome::Skip,
        // A malformed literal is a conversion-syntax invalid yielding NaN.
        Err(_) => compare(
            case,
            &Decimal::quiet_nan(false, DecBig::zero()),
            Status::INVALID,
        ),
    }
}

fn compare(case: &TestCase, res: &Decimal, status: Status) -> Outcome {
    let exp_status = expected_status(&case.conditions);
    let Ok(exp_val) = Decimal::parse_str(&case.expected) else {
        return Outcome::Fail(format!("unparseable expected {:?}", case.expected));
    };
    if *res == exp_val && status == exp_status {
        Outcome::Pass
    } else {
        Outcome::Fail(format!(
            "got {res} [{status:?}] want {} [{exp_status:?}]",
            case.expected
        ))
    }
}

/// Decode decTest condition tokens onto this crate's six status flags. `Clamped`
/// and `Underflow` are compared for real (unlike the shared harness, which masks
/// them). `Subnormal`, `Rounded`, and `Lost_digits` have no corresponding flag
/// and are informational, so they are ignored.
fn expected_status(conditions: &[String]) -> Status {
    let mut s = Status::OK;
    for cond in conditions {
        match cond.as_str() {
            "inexact" => s |= Status::INEXACT,
            "overflow" => s |= Status::OVERFLOW | Status::INEXACT,
            "underflow" => s |= Status::UNDERFLOW | Status::INEXACT,
            "clamped" => s |= Status::CLAMPED,
            "division_by_zero" => s |= Status::DIV_BY_ZERO,
            "invalid_operation"
            | "division_impossible"
            | "division_undefined"
            | "conversion_syntax"
            | "invalid_context" => s |= Status::INVALID,
            _ => {}
        }
    }
    s
}

// ---------------------------------------------------------------------------
// decTest line parser (copied from the workspace-root runner; the `.decTest`
// grammar is frozen, so this small state machine does not drift).

struct TestCase {
    id: String,
    op: String,
    operands: Vec<String>,
    expected: String,
    conditions: Vec<String>,
}

/// Strip an end-of-line `--` comment, ignoring `--` inside single- or
/// double-quoted operands (the same quoting rules as `tokenise`); a
/// position-blind cut silently dropped the quoted adversarial vectors
/// basx504 (`'--1'`) and basx555 (`'1E--1'`) from every bucket
/// (fd-aqs.9). Mirrors `ferrodec-test-support`'s copy.
fn strip_comment(line: &str) -> &str {
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

fn parse_directive(line: &str) -> Option<(String, String)> {
    let colon = line.find(':')?;
    let name = line[..colon].trim();
    let value = line[colon + 1..].trim();
    if name.is_empty() || name.chars().any(|c| !(c.is_ascii_alphabetic() || c == '_')) {
        return None;
    }
    Some((name.to_lowercase(), value.to_lowercase()))
}

fn parse_test_case(line: &str) -> Option<TestCase> {
    let tokens = tokenise(line)?;
    let arrow = tokens.iter().position(|t| t == "->")?;
    if arrow < 3 || arrow + 1 >= tokens.len() {
        return None;
    }
    Some(TestCase {
        id: tokens[0].clone(),
        op: tokens[1].to_lowercase(),
        operands: tokens[2..arrow].to_vec(),
        expected: tokens[arrow + 1].clone(),
        conditions: tokens[arrow + 2..]
            .iter()
            .map(|s| s.to_lowercase())
            .collect(),
    })
}

fn tokenise(line: &str) -> Option<Vec<String>> {
    let mut out = Vec::new();
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b' ' | b'\t' | b'\r' => i += 1,
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
                while i < bytes.len() && !matches!(bytes[i], b' ' | b'\t' | b'\r') {
                    i += 1;
                }
                out.push(std::str::from_utf8(&bytes[start..i]).ok()?.to_string());
            }
        }
    }
    Some(out)
}
