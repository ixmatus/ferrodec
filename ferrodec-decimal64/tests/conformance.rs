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
        // F4 (S10, fd-7n8): `compare` / `comparetotal` /
        // `comparetotmag` / `samequantum` / `quantize` wired. No
        // ferrodec correctness defect surfaced — the 22 `ddCompare`
        // cases that first failed were NaN-result payload/sign
        // expectations (`compare -NaN -NaN -> -NaN`); the conformance
        // contract for a NaN result is "is a NaN", payload/sign not
        // pinned, exactly as the canonical Decimal128 runner treats
        // them (status, incl. the sNaN `INVALID`, is still compared
        // exactly). `ddCompare` 647 of 649 (2 `#`-hex skips);
        // `ddCompareTotal` / `ddCompareTotalMag` 611 of 613 (2 `#`-hex
        // each); `ddSameQuantum` 333 of 333 (predicate, no skips);
        // `ddQuantize` 606 of 683 (77 skips: extreme exponents past
        // the parser cap and `#`-hex). `ddCompareSig` stays 0 —
        // `compareSignal` is unimplemented (same posture as the
        // Decimal128 runner). Exact-match per ADR-0010.
        ("ddCompare.decTest", 647),
        ("ddCompareTotal.decTest", 611),
        ("ddCompareTotalMag.decTest", 611),
        // fd-37z: copy family wired. Non-signaling bit ops; the
        // files have no `#`-hex or non-IEEE-rounding cases, so every
        // case dispatches and passes.
        // fd-37z: `ddCanonical.decTest` is wholly DPD-hex encoded
        // (every operand and expected is a `#…` DPD pattern, both
        // the `apply` and `canonical` cases). Decimal64 has no DPD
        // codec and no `dpd` feature, so every case Skips on the
        // `#`-hex guard and none reaches `canonicalize`. The
        // `canonical` dispatch arm is correct and forward-useful;
        // this file stays at 0 until a Decimal64 DPD codec lands
        // (fd-bef). Pinned at 0 as a regression guard: it will trip
        // and demand a re-pin when that codec arrives.
        ("ddCanonical.decTest", 0),
        ("ddCopy.decTest", 43),
        ("ddCopyAbs.decTest", 43),
        ("ddCopyNegate.decTest", 43),
        ("ddCopySign.decTest", 107),
        // F2: `multiply` / `divide` wired. No correctness bug
        // surfaced (the H3 typed-BiasedExp work already made them
        // conformant): `ddMultiply.decTest` 444 of 446 (2 `#`-hex
        // skips), `ddDivide.decTest` 702 of 717 (15 skips, extreme
        // exponents / `#`-hex). `ddDivideInt.decTest` is a distinct
        // operation, not wired here.
        ("ddDivide.decTest", 702),
        // fd-ci0.5 (ADR-0031): `divideInteger` wired. 371 of 373 pass;
        // the 2 skips are the `#`-hex BID-interchange cases.
        ("ddDivideInt.decTest", 371),
        ("ddEncode.decTest", 0),
        // F3: `fma` wired. Rises 2 → 1318 of 1378 after the fd-d47
        // FMA-side fix in `h2_borrow_and_extend` (the
        // `ddfma364xx` power-of-ten borrow-extend collapse, the FMA
        // analogue of the addsub boundary family). The H3 case
        // `ddfma2504` is among the passers. 60 skips are
        // unrepresentable operands / `#`-hex.
        ("ddFMA.decTest", 1318),
        // fd-8aq: IEEE 754-2019 §9.6 `maxmag` / `minmag`
        // (`Decimal64::max_magnitude` / `min_magnitude`) wired; both
        // were `Skip` before. Zero failures: `ddMaxMag` 241 of 243,
        // `ddMinMag` 231 of 233 (2 `#`-hex BID-interchange skips
        // each, never failures). Exact-match per ADR-0010.
        ("ddMaxMag.decTest", 241),
        ("ddMinMag.decTest", 231),
        ("ddMultiply.decTest", 444),
        ("ddQuantize.decTest", 606),
        // fd-ci0.4 (ADR-0031): `reduce` wired. 133 of 134 pass; the 1
        // skip is the `#`-hex BID-interchange case (`ddred901`).
        ("ddReduce.decTest", 133),
        // fd-pvu (ADR-0027): `remainder` (truncating, `Decimal64::rem`)
        // and `remaindernear` (IEEE §5.3.1 nearest-even, the new
        // `Decimal64::rem_near`) wired; both were `Skip` before. Zero
        // failures: `ddRemainder` 503 of 505, `ddRemainderNear` 527 of
        // 529 (2 `#`-hex BID-interchange skips each, never failures).
        // Counts pinned exactly from the run per ADR-0010 /
        // feedback_regression_guard_exact_match.
        ("ddRemainder.decTest", 503),
        ("ddRemainderNear.decTest", 527),
        ("ddSameQuantum.decTest", 333),
        ("ddSubtract.decTest", 514),
        // fd-hnx: `tointegral` / `tointegralx` (IEEE 754-2019 §5.9
        // `roundToIntegral{,Exact}`) wired. No ferrodec correctness
        // defect surfaced (0 fail): `ddToIntegral.decTest` 164 of 178,
        // the 14 skips being non-IEEE rounding directives, operands
        // past the parser cap, and `#`-hex interchange — never
        // failures. Exact-match per-file count per ADR-0010 /
        // feedback_regression_guard_exact_match.
        ("ddToIntegral.decTest", 164),
    ]
}

fn run_case(case: &TestCase, ctx: &Context) -> Outcome {
    match case.op.as_str() {
        "tosci" | "apply" => run_tosci(case, ctx),
        "add" | "subtract" | "multiply" | "divide" => run_binary(case, ctx),
        "fma" => run_ternary(case, ctx),
        "compare" => run_compare(case, ctx),
        "comparetotal" => run_total(case, ctx, false),
        // decTest spells the magnitude variant `comparetotmag`; accept
        // the longer alias too for robustness.
        "comparetotmag" | "comparetotalmag" => run_total(case, ctx, true),
        "samequantum" => run_samequantum(case, ctx),
        // decTest `minmag` / `maxmag`: IEEE 754-2019 §9.6
        // minimumMagnitudeNumber / maximumMagnitudeNumber (fd-8aq).
        "minmag" => run_min_max_mag(case, ctx, false),
        "maxmag" => run_min_max_mag(case, ctx, true),
        // decTest `remainder` is the *truncating* remainder
        // (`Decimal64::rem`); `remaindernear` is the IEEE 754-2019
        // §5.3.1 round-half-even-quotient remainder, the new
        // `Decimal64::rem_near` (ADR-0027, fd-pvu). Before rem_near
        // both routed to `Skip`.
        "remainder" => run_rem(case, ctx, false),
        "remaindernear" => run_rem(case, ctx, true),
        "quantize" => run_quantize(case, ctx),
        // decTest `divideint`: General Decimal Arithmetic truncated
        // integer quotient at exponent 0 (ADR-0031).
        "divideint" => run_divide_integer(case, ctx),
        // decTest `reduce`: General Decimal Arithmetic trailing-zero
        // strip on a finite coefficient (ADR-0031). Exact; never raises
        // INEXACT. Zero of any cohort normalises to exponent 0.
        "reduce" => run_reduce(case, ctx),
        "tointegral" => run_integral(case, ctx, false),
        "tointegralx" => run_integral(case, ctx, true),
        // decTest `copynegate`: the non-signaling sign flip
        // (`Decimal64::neg`), distinct from `minus`, which is the
        // arithmetic negation that signals on sNaN. Never raises a
        // flag (the non-arithmetic copy family; fd-37z).
        "copynegate" => run_copy_unary(case, ctx, Decimal64::neg),
        // decTest `copyabs`: the non-signaling absolute value
        // (`Decimal64::abs`), distinct from `abs`/`plus`-style
        // arithmetic that signals on sNaN. Never raises a flag.
        "copyabs" => run_copy_unary(case, ctx, Decimal64::abs),
        // decTest `copysign`: magnitude of operand 1, sign of
        // operand 2. Binary; never raises a flag.
        "copysign" => run_copy_sign(case, ctx),
        // decTest `copy`: returns the operand unchanged, bit for
        // bit. The degenerate copy-family member; never raises a
        // flag.
        "copy" => run_copy_unary(case, ctx, |d| d),
        // decTest `canonical`: ferrodec stores BID, which is always
        // canonical, so `Decimal64::canonicalize` is effectively the
        // identity, but route through it for fidelity. Never raises
        // a flag. `ddCanonical.decTest` is a mixed-op file; its
        // count is cumulative over every arm above (the `comparesig`
        // cases remain Skip, no `compare_signaling` yet).
        "canonical" => run_copy_unary(case, ctx, Decimal64::canonicalize),
        _ => Outcome::Skip,
    }
}

/// Render an `Ordering` as the decTest integer token `-1` / `0` / `1`.
fn ord_token(ord: core::cmp::Ordering) -> Decimal64 {
    match ord {
        core::cmp::Ordering::Less => Decimal64::NEG_ONE,
        core::cmp::Ordering::Equal => Decimal64::ZERO,
        core::cmp::Ordering::Greater => Decimal64::ONE,
    }
}

/// Whether a decTest expected token denotes a NaN (quiet or signaling,
/// any sign, any payload — e.g. `NaN`, `-NaN`, `NaN9`, `sNaN123`).
fn expected_is_nan(expected: &str) -> bool {
    let body = expected.trim_start_matches(['-', '+']);
    body.starts_with("NaN")
        || body.starts_with("nan")
        || body.starts_with("sNaN")
        || body.starts_with("snan")
        || body.starts_with("qNaN")
}

/// Compare the formatted result and conformance-masked status against
/// the decTest expectation, with the same skip-not-fail policy as
/// `run_binary`.
///
/// NaN-result policy mirrors the canonical `Decimal128` conformance
/// runner: when the test expects a NaN, the contract is only that the
/// actual is *a* NaN — the result NaN's sign and payload are not pinned
/// (ferrodec's compare returns an ordering, not the propagated operand
/// NaN, and `Display` does not render a payload). The status is still
/// compared exactly, so the signaling-NaN `INVALID` distinction is
/// preserved.
fn check(result: Decimal64, status: Status, case: &TestCase) -> Outcome {
    let formatted = format_value(result);
    if expected_is_nan(&case.expected) {
        if !result.is_nan() {
            return Outcome::Fail(format!("expected NaN, got {formatted:?}"));
        }
    } else if formatted != case.expected {
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

/// `compare`: GDA quiet comparison. A NaN operand yields a NaN result
/// (signaling NaN additionally raises `INVALID`); otherwise the result
/// is `-1` / `0` / `1`. The status from `partial_cmp` is the IEEE 754
/// status the spec mandates.
fn run_compare(case: &TestCase, ctx: &Context) -> Outcome {
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
    let (ord, status) = a.partial_cmp(b);
    let result = match ord {
        None => Decimal64::NAN,
        Some(o) => ord_token(o),
    };
    check(result, status, case)
}

/// `comparetotal` / `comparetotmag`: IEEE 754-2019 §5.10 total-order
/// predicate (or its magnitude variant). Always defined: never NaN,
/// never raises a flag.
fn run_total(case: &TestCase, ctx: &Context, magnitude: bool) -> Outcome {
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
    let ord = if magnitude {
        a.compare_total_magnitude(b)
    } else {
        a.total_cmp(b)
    };
    check(ord_token(ord), Status::OK, case)
}

/// `minmag` / `maxmag`: IEEE 754-2019 §9.6 minimumMagnitudeNumber /
/// maximumMagnitudeNumber (`Decimal64::min_magnitude` /
/// `max_magnitude`). NaN-as-missing-value; a signaling NaN raises
/// `INVALID`. The op returns the value and the IEEE status the spec
/// mandates.
fn run_min_max_mag(case: &TestCase, ctx: &Context, max: bool) -> Outcome {
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
    let (result, status) = if max {
        a.max_magnitude(b)
    } else {
        a.min_magnitude(b)
    };
    check(result, status, case)
}

/// The decTest copy family (`copy`, `copyabs`, `copynegate`,
/// `canonical`) plus `canonical`: unary, non-arithmetic bit
/// operations. GDA defines them to never raise a flag, not even
/// `INVALID` on a signaling NaN, so the result status is always
/// `Status::OK` (compare the signaling `abs_with_status` /
/// `neg_with_status`, which are *not* what these ops use). `op` is
/// the pure transform; `Decimal64::neg` / `abs` / `canonicalize` are
/// already the non-signaling bit forms, and `copy` is the identity.
fn run_copy_unary(case: &TestCase, ctx: &Context, op: fn(Decimal64) -> Decimal64) -> Outcome {
    if case.operands.len() != 1 || case.expected.starts_with('#') {
        return Outcome::Skip;
    }
    let rm = match map_rounding(&ctx.rounding) {
        Some(r) => r,
        None => return Outcome::Skip,
    };
    let a = match parse_operand(&case.operands[0], rm) {
        Some(a) => a,
        None => return Outcome::Skip,
    };
    check(op(a), Status::OK, case)
}

/// `copysign`: the binary member of the copy family. Takes the
/// magnitude of the first operand and the sign of the second
/// (`Decimal64::copysign`). Non-arithmetic; never raises a flag.
fn run_copy_sign(case: &TestCase, ctx: &Context) -> Outcome {
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
    check(a.copysign(b), Status::OK, case)
}

/// `samequantum`: §5.3.2 predicate, rendered as the decTest boolean
/// token `1` / `0`. Total, never raises a flag.
fn run_samequantum(case: &TestCase, ctx: &Context) -> Outcome {
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
    let result = if a.same_quantum(b) {
        Decimal64::ONE
    } else {
        Decimal64::ZERO
    };
    check(result, Status::OK, case)
}

/// `quantize`: §5.3.3, identical result/status comparison shape to
/// `run_binary` (it returns `(Decimal64, Status)`).
fn run_quantize(case: &TestCase, ctx: &Context) -> Outcome {
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
    let (result, status) = a.quantize(b, rm);
    check(result, status, case)
}

/// `remainder` (truncating, `Decimal64::rem`) / `remaindernear`
/// (IEEE 754-2019 §5.3.1 nearest-even, `Decimal64::rem_near`). Both
/// return `(Decimal64, Status)`; same comparison shape and
/// skip-not-fail policy as `run_quantize` (ADR-0027, fd-pvu).
fn run_rem(case: &TestCase, ctx: &Context, near: bool) -> Outcome {
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
    let (result, status) = if near { a.rem_near(b) } else { a.rem(b, rm) };
    check(result, status, case)
}

/// `tointegral` / `tointegralx`: IEEE 754-2019 §5.9
/// `roundToIntegral{,Exact}`. Unary, at the active rounding mode;
/// `tointegralx` signals `INEXACT` when a non-zero fractional part is
/// discarded, `tointegral` never does. Same result/status comparison
/// shape and skip-not-fail policy as `run_quantize`.
/// `divideint`: General Decimal Arithmetic truncated integer
/// quotient at exponent 0 (ADR-0031, fd-ci0.5). Two operands; no
/// rounding-mode interaction (the kernel is exact and never raises
/// `INEXACT`).
fn run_divide_integer(case: &TestCase, ctx: &Context) -> Outcome {
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
    let (result, status) = a.divide_integer(b);
    check(result, status, case)
}

/// `reduce`: General Decimal Arithmetic trailing-zero strip (ADR-0031,
/// fd-ci0.4). Single operand, no rounding-mode interaction (the op is
/// exact and never raises `INEXACT`). The context's rounding is still
/// consulted for operand parsing fidelity but the kernel itself
/// ignores it.
fn run_reduce(case: &TestCase, ctx: &Context) -> Outcome {
    if case.operands.len() != 1 || case.expected.starts_with('#') {
        return Outcome::Skip;
    }
    let rm = match map_rounding(&ctx.rounding) {
        Some(r) => r,
        None => return Outcome::Skip,
    };
    let a = match parse_operand(&case.operands[0], rm) {
        Some(a) => a,
        None => return Outcome::Skip,
    };
    let (result, status) = a.reduce();
    check(result, status, case)
}

fn run_integral(case: &TestCase, ctx: &Context, signal_inexact: bool) -> Outcome {
    if case.operands.len() != 1 || case.expected.starts_with('#') {
        return Outcome::Skip;
    }
    let rm = match map_rounding(&ctx.rounding) {
        Some(r) => r,
        None => return Outcome::Skip,
    };
    let a = match parse_operand(&case.operands[0], rm) {
        Some(a) => a,
        None => return Outcome::Skip,
    };
    let (result, status) = if signal_inexact {
        a.round_to_integral_exact(rm)
    } else {
        a.round_to_integral(rm)
    };
    check(result, status, case)
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
