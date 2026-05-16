//! Independent soundness gate for the exact correctly-rounded oracle.
//!
//! The oracle in `ferrodec-test-support` is the keystone every migrated
//! property test (S2-S5) asserts against bit-for-bit. A wrong oracle
//! gives false confidence, so it is pinned here directly against the
//! IBM decTest vectors — Mike Cowlishaw's reference suite, authored by
//! the IEEE 754-2019 decimal arithmetic spec author. This cross-checks
//! the oracle against the *specification's own* reference, with no
//! `ferrodec` arithmetic in the loop at all (the oracle's predicted
//! cohort and status are compared directly against decTest's expected
//! string and conditions), so the validation is genuinely independent
//! — and immune to the `parse_str` extreme-exponent round-trip
//! fragility.
//!
//! For every `add` / `subtract` / `multiply` / `fma` / `divide` /
//! `remaindernear` case at `precision: 34` with finite operands under
//! an IEEE 754 rounding directive, the oracle's predicted value
//! (cohort: sign, coefficient, exponent) and status must equal
//! decTest's expected value and conditions exactly. Non-IEEE rounding
//! (`half_down` / `05up`, ADR-0005), special operands / results
//! (NaN, Infinity, division by zero), and the few cases under a
//! non-34 precision directive are out of this oracle's modelled scope
//! and are skipped, not failed.

#![cfg(feature = "fmt")]

use ferrodec_test_support::conformance::{
    decode_conditions, map_rounding, parse_directive, parse_test_case, status_conformance_eq,
    strip_comment,
};
use ferrodec_test_support::oracle::{self, parse_decimal, Expect, Format};

/// Does the oracle's predicted value equal decTest's expected token?
/// `None` ⇒ the expected token is special (NaN / Infinity / `?` /
/// `#…`) and out of the finite-arithmetic oracle's scope.
fn value_agrees(value: &Expect, expected: &str) -> Option<bool> {
    let lower = expected.to_ascii_lowercase();
    if let Expect::Nan = value {
        // The oracle predicts GDA Division_impossible / undefined;
        // decTest must agree by expecting a NaN.
        return Some(lower.contains("nan"));
    }
    if expected == "?" || expected.starts_with('#') || lower.contains("nan") {
        return None;
    }
    let is_inf = lower.contains("inf");
    Some(match value {
        Expect::Nan => unreachable!("handled above"),
        Expect::Infinity { neg } => is_inf && (*neg == expected.starts_with('-')),
        Expect::Finite { neg, coeff, exp } => {
            if is_inf {
                return Some(false);
            }
            // decTest expected strings are cohort-faithful; parse to
            // (sign, coefficient, exponent) and compare exactly. This
            // pins the §6.3 preferred exponent against the reference.
            match parse_decimal(expected) {
                Some(d) => d.neg == *neg && d.coeff == *coeff && d.exp == *exp,
                None => return None,
            }
        }
    })
}

fn replay(file: &str) -> (usize, usize) {
    let path = format!("{}/tests/vectors/{file}", env!("CARGO_MANIFEST_DIR"));
    let content = std::fs::read_to_string(&path).expect("read decTest");
    let mut precision: u32 = 34;
    let mut rounding = String::from("half_even");
    let mut checked = 0usize;
    let mut skipped = 0usize;
    let f = Format::DECIMAL128;

    for raw in content.lines() {
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if let Some((name, value)) = parse_directive(line) {
            match name.as_str() {
                "precision" => precision = value.parse().unwrap_or(precision),
                "rounding" => rounding = value,
                _ => {}
            }
            continue;
        }
        let Some(case) = parse_test_case(line) else {
            continue;
        };
        let arity = match case.op.as_str() {
            "add" | "subtract" | "multiply" | "divide" | "remaindernear" => 2,
            "fma" => 3,
            _ => continue,
        };
        // Out of this oracle's modelled scope -> skip, never fail.
        if precision != 34 || case.operands.len() != arity {
            skipped += 1;
            continue;
        }
        let Some(rm) = map_rounding(&rounding) else {
            skipped += 1;
            continue;
        };
        let Some(operands) = case
            .operands
            .iter()
            .map(|s| parse_decimal(s))
            .collect::<Option<Vec<_>>>()
        else {
            skipped += 1;
            continue;
        };
        // Division / remainder by zero is a special-value case the
        // finite-arithmetic oracle does not model.
        if matches!(case.op.as_str(), "divide" | "remaindernear") && operands[1].is_zero() {
            skipped += 1;
            continue;
        }

        let r = match case.op.as_str() {
            "add" => oracle::add(&operands[0], &operands[1], f, rm),
            "subtract" => oracle::sub(&operands[0], &operands[1], f, rm),
            "multiply" => oracle::mul(&operands[0], &operands[1], f, rm),
            "divide" => oracle::div(&operands[0], &operands[1], f, rm),
            "remaindernear" => oracle::rem(&operands[0], &operands[1], f, rm),
            _ => oracle::fma(&operands[0], &operands[1], &operands[2], f, rm),
        };

        let Some(value_ok) = value_agrees(&r.value, &case.expected) else {
            skipped += 1;
            continue;
        };
        assert!(
            value_ok,
            "[{}] {} {:?} -> {} rm={:?}: oracle {} disagrees with decTest",
            case.id,
            case.op,
            case.operands,
            case.expected,
            rm,
            r.decimal_string(),
        );
        let want_status = decode_conditions(&case.conditions);
        assert!(
            status_conformance_eq(r.status, want_status),
            "[{}] {} {:?} -> {} rm={:?}: oracle status {:?} != decTest {:?}",
            case.id,
            case.op,
            case.operands,
            case.expected,
            rm,
            r.status,
            want_status,
        );
        checked += 1;
    }
    (checked, skipped)
}

#[test]
fn oracle_matches_dectest_reference() {
    let mut total = 0;
    for f in [
        "dqAdd.decTest",
        "dqSubtract.decTest",
        "dqMultiply.decTest",
        "dqFMA.decTest",
        "dqDivide.decTest",
        "dqRemainderNear.decTest",
    ] {
        let (checked, skipped) = replay(f);
        eprintln!("{f}: {checked} oracle-checked, {skipped} out-of-scope skipped");
        total += checked;
    }
    // Sanity: the suite must actually exercise the oracle, not skip
    // everything (a regression that broke parsing would otherwise pass
    // silently).
    assert!(
        total > 2000,
        "expected the oracle to be exercised on >2000 decTest cases, got {total}"
    );
}
