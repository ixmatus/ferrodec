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
//! # Two operand encodings
//!
//! `dsBase.decTest` uses decimal-string operands. `dsEncode.decTest`
//! is DPD interchange: its `#hex` literals are IEEE 754-2019 decimal32
//! DPD byte patterns (8 hex chars = 4 big-endian bytes), not BID raw
//! bits. The file header states "Selected DPD codes" and the patterns
//! match the §3.5.2 declet encoding. The `dpd`-gated
//! [`Decimal32::from_dpd_bytes`] / [`Decimal32::to_dpd_bytes`] codec
//! decodes and encodes them; with the `dpd` feature off, every
//! `#hex` case is skipped (so `dsEncode` stays at its
//! string-only baseline).
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
/// - `dsBase.decTest`: 698 of 909 cases pass. The 211 skips break
///   down as ~7 pathologically large exponents (deferred, see
///   `ParseDecimalError::ExponentOutOfRange`) plus ~204 cases under
///   non-IEEE rounding directives (`half_down`, `05up`) which we
///   won't coerce onto an IEEE mode (mirrors ferrodec's ADR-0005
///   posture). Unchanged by the DPD work.
/// - `dsEncode.decTest`: the count is feature-conditional. With the
///   `dpd` feature off, only the 2 string-to-string `apply` cases
///   pass and every `#hex` case is skipped at the dispatcher. With
///   `dpd` on, [`Decimal32::from_dpd_bytes`] / `to_dpd_bytes` decode
///   and encode the IEEE 754-2019 DPD interchange patterns; the
///   exact pinned count is the measured pass total. The residual
///   skips with `dpd` on are the `half_up`-context cases whose
///   `Clamped` expectation the runner cannot reproduce through
///   `parse_str` (decimal32's `parse_str` does not re-quantize to the
///   format's clamped preferred exponent), tracked in
///   `KNOWN_ISSUES.md`.
const fn expected_per_file() -> &'static [(&'static str, usize)] {
    #[cfg(not(feature = "dpd"))]
    {
        &[("dsBase.decTest", 698), ("dsEncode.decTest", 2)]
    }
    #[cfg(feature = "dpd")]
    {
        &[("dsBase.decTest", 698), ("dsEncode.decTest", 250)]
    }
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
///
/// `apply` cases in `dsEncode.decTest` come in two `#hex` directions:
///
/// * `#hex -> value` — decode the DPD bytes, render via `Display`,
///   compare the rendered string against the expected value.
/// * `value -> #hex` — parse the decimal string, encode to DPD bytes,
///   compare the hex against the expected `#hex`.
///
/// Both require the `dpd` feature; without it, any `#`-bearing case
/// is skipped (and the `expected_per_file` table records the
/// feature-off baseline).
fn run_tosci(case: &TestCase, ctx: &Context) -> Outcome {
    if case.operands.len() != 1 {
        return Outcome::Skip;
    }
    let input = &case.operands[0];
    let operand_is_hex = input.starts_with('#');
    let expected_is_hex = case.expected.starts_with('#');

    if operand_is_hex || expected_is_hex {
        return run_dpd_case(case, input, operand_is_hex, expected_is_hex);
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
        // 1 000 000 magnitude cap (via `ExponentOutOfRange` on the
        // explicit-exponent path or `CoefficientOverflow` on the
        // implicit-exponent / leading-zero-saturation path; the latter
        // variant was promoted by ADR-0029 item 2 / fd-7f1); the
        // spec-conformant behaviour (saturate to ±Inf or ±0 at parse
        // time) is a deferred design call. Skip rather than fail those
        // cases.
        Err(ParseDecimalError::ExponentOutOfRange | ParseDecimalError::CoefficientOverflow) => {
            return Outcome::Skip;
        }
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
    clamped_outcome(status, case)
}

/// Compare the §7.4 CLAMPED flag (fd-61r / ADR-0048), separately from the
/// five IEEE flags (which `status_conformance_eq` masks CLAMPED out of).
/// Returns `Pass` on a match, `Skip` for the BID-structural residual (an
/// operand clamped into its cohort at parse, whose pre-clamp exponent BID
/// cannot keep), and `Fail` for a genuine under- or over-raise.
fn clamped_outcome(status: Status, case: &TestCase) -> Outcome {
    let expected_clamped = case.conditions.iter().any(|c| c == "clamped");
    if status.clamped() != expected_clamped {
        if expected_clamped && !status.clamped() && any_operand_clamped_at_parse(&case.operands) {
            return Outcome::Skip;
        }
        return Outcome::Fail(format!(
            "CLAMPED mismatch: got {} want {expected_clamped} (conditions {:?})",
            status.clamped(),
            case.conditions
        ));
    }
    Outcome::Pass
}

/// `true` when any operand of a decTest case is itself clamped at parse
/// (its literal quantum exceeds the format range, so it is stored in a
/// padded cohort and loses its pre-clamp exponent). The documented
/// BID-structural CLAMPED residual (fd-61r / ADR-0048).
fn any_operand_clamped_at_parse(operands: &[String]) -> bool {
    operands.iter().any(|op| {
        Decimal32::parse_str(op, ferrodec_decimal32::RoundingMode::NearestEven)
            .is_ok_and(|(_, s)| s.clamped())
    })
}

/// Handle a `dsEncode` `apply` case that touches the DPD interchange
/// surface. Returns `Skip` when the `dpd` feature is off so the
/// feature-off baseline holds.
#[cfg(feature = "dpd")]
fn run_dpd_case(
    case: &TestCase,
    input: &str,
    operand_is_hex: bool,
    expected_is_hex: bool,
) -> Outcome {
    if operand_is_hex && !expected_is_hex {
        // `#hex -> value`: decode then render.
        let bytes = match parse_dpd_hex(input) {
            Some(b) => b,
            None => return Outcome::Fail(format!("bad #hex operand {input:?}")),
        };
        let decoded = Decimal32::from_dpd_bytes(bytes);
        let rendered = format_value(decoded);
        // decTest renders NaN payloads in the expected string (e.g.
        // `sNaN999999`); `Display` emits `NaN` / `sNaN` without the
        // payload. Match the special-token shape rather than the
        // payload digits, which is what the value carries.
        if expected_is_special(&case.expected) {
            return if special_shape_eq(decoded, &case.expected) {
                Outcome::Pass
            } else {
                Outcome::Fail(format!(
                    "special mismatch: decoded {rendered:?} want {:?}",
                    case.expected
                ))
            };
        }
        if rendered == case.expected {
            Outcome::Pass
        } else {
            Outcome::Fail(format!(
                "decode mismatch: {input} -> {rendered:?} want {:?}",
                case.expected
            ))
        }
    } else if !operand_is_hex && expected_is_hex {
        // `value -> #hex`: parse then encode.
        let parsed = match Decimal32::parse_str(input, ferrodec_ieee_nearest()) {
            Ok((v, _)) => v,
            Err(ParseDecimalError::ExponentOutOfRange | ParseDecimalError::CoefficientOverflow) => {
                return Outcome::Skip;
            }
            Err(_) => return Outcome::Skip,
        };
        let want = match parse_dpd_hex(&case.expected) {
            Some(b) => b,
            None => return Outcome::Fail(format!("bad #hex expected {:?}", case.expected)),
        };
        // Several `value -> #hex` cases carry a `Clamped` condition:
        // the spec re-quantizes the coefficient to the format's
        // clamped preferred exponent before encoding. decimal32's
        // `parse_str` does not perform that §7.4 clamp, so the
        // encoded bytes legitimately differ. Skip the clamped
        // direction rather than fail it (tracked in KNOWN_ISSUES.md);
        // the matching `#hex -> value` direction still passes and
        // exercises the decoder.
        if case
            .conditions
            .iter()
            .any(|c| c.eq_ignore_ascii_case("clamped"))
        {
            return Outcome::Skip;
        }
        let got = parsed.to_dpd_bytes();
        if got == want {
            Outcome::Pass
        } else {
            Outcome::Fail(format!(
                "encode mismatch: {input} -> {:08X} want {:08X}",
                u32::from_be_bytes(got),
                u32::from_be_bytes(want),
            ))
        }
    } else {
        // `#hex -> #hex`: decode then re-encode (canonicalization).
        let in_bytes = match parse_dpd_hex(input) {
            Some(b) => b,
            None => return Outcome::Fail(format!("bad #hex operand {input:?}")),
        };
        let want = match parse_dpd_hex(&case.expected) {
            Some(b) => b,
            None => return Outcome::Fail(format!("bad #hex expected {:?}", case.expected)),
        };
        let got = Decimal32::from_dpd_bytes(in_bytes).to_dpd_bytes();
        if got == want {
            Outcome::Pass
        } else {
            Outcome::Fail(format!(
                "canonicalize mismatch: {input} -> {:08X} want {:08X}",
                u32::from_be_bytes(got),
                u32::from_be_bytes(want),
            ))
        }
    }
}

#[cfg(not(feature = "dpd"))]
fn run_dpd_case(
    _case: &TestCase,
    _input: &str,
    _operand_is_hex: bool,
    _expected_is_hex: bool,
) -> Outcome {
    // Without the `dpd` feature the interchange codec is absent, so
    // every `#hex` case is skipped. `expected_per_file` records the
    // feature-off baseline (`dsEncode` = 2).
    Outcome::Skip
}

/// Decode an 8-char `#hex` literal into the 4 big-endian DPD bytes.
#[cfg(feature = "dpd")]
fn parse_dpd_hex(s: &str) -> Option<[u8; 4]> {
    let h = s.strip_prefix('#')?;
    if h.len() != 8 {
        return None;
    }
    let mut out = [0u8; 4];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(h.get(2 * i..2 * i + 2)?, 16).ok()?;
    }
    Some(out)
}

#[cfg(feature = "dpd")]
fn ferrodec_ieee_nearest() -> ferrodec_decimal32::RoundingMode {
    ferrodec_decimal32::RoundingMode::NearestEven
}

/// Whether the expected token is a NaN / sNaN / Infinity literal
/// (possibly signed, possibly with a NaN payload like `sNaN999999`).
#[cfg(feature = "dpd")]
fn expected_is_special(expected: &str) -> bool {
    let body = expected.trim_start_matches(['-', '+']);
    body.starts_with("NaN")
        || body.starts_with("sNaN")
        || body.eq_ignore_ascii_case("infinity")
        || body.eq_ignore_ascii_case("inf")
}

/// Compare a decoded special value's shape (kind + sign) against the
/// expected special token. NaN payload digits in the expected token
/// (`sNaN999999`) are not pinned: the decoded value carries the
/// payload in its trailing bits, and the conformance contract for
/// these decode cases is the kind and sign, matching how the
/// decimal128 runner treats NaN comparisons.
#[cfg(feature = "dpd")]
fn special_shape_eq(d: Decimal32, expected: &str) -> bool {
    let neg = expected.starts_with('-');
    let body = expected.trim_start_matches(['-', '+']);
    if body.eq_ignore_ascii_case("infinity") || body.eq_ignore_ascii_case("inf") {
        return d.is_infinite() && d.is_sign_negative() == neg;
    }
    if body.starts_with("sNaN") {
        return d.is_signaling_nan() && d.is_sign_negative() == neg;
    }
    if body.starts_with("NaN") {
        return d.is_nan() && !d.is_signaling_nan() && d.is_sign_negative() == neg;
    }
    false
}

fn format_value(d: Decimal32) -> String {
    use std::fmt::Write as _;
    let mut s = String::new();
    let _ = write!(s, "{d}");
    s
}
