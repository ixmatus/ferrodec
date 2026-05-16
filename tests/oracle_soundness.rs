//! Independent soundness gate for the exact correctly-rounded oracle.
//!
//! The oracle in `ferrodec-test-support` is the keystone every migrated
//! property test (S2-S5) asserts against bit-for-bit. A wrong oracle
//! gives false confidence, so it is pinned here directly against the
//! IBM decTest vectors — Mike Cowlishaw's reference suite, authored by
//! the IEEE 754-2019 decimal arithmetic spec author. This cross-checks
//! the oracle against the *specification's own* reference, not against
//! `ferrodec` (whose conformance the suite already establishes
//! separately), so the validation is genuinely independent.
//!
//! For every `add` / `subtract` / `multiply` case at `precision: 34`
//! with finite operands under an IEEE 754 rounding directive, the
//! oracle's predicted value (re-parsed to bits) and status must equal
//! decTest's expected value and conditions exactly. Non-IEEE rounding
//! (`half_down` / `05up`, ADR-0005), special operands, and the few
//! cases under a non-34 precision directive are out of this oracle's
//! modelled scope and are skipped, not failed.

#![cfg(feature = "fmt")]

use ferrodec::Decimal128;
use ferrodec_test_support::conformance::{
    decode_conditions, map_rounding, parse_directive, parse_test_case, status_conformance_eq,
    strip_comment,
};
use ferrodec_test_support::oracle::{self, parse_decimal, Format};

fn replay(file: &str) -> (usize, usize) {
    let path = format!("{}/tests/vectors/{file}", env!("CARGO_MANIFEST_DIR"));
    let content = std::fs::read_to_string(&path).expect("read decTest");
    let mut precision: u32 = 34;
    let mut rounding = String::from("half_even");
    let mut checked = 0usize;
    let mut skipped = 0usize;

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
        if !matches!(case.op.as_str(), "add" | "subtract" | "multiply" | "fma") {
            continue;
        }
        let arity = if case.op == "fma" { 3 } else { 2 };
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
        // decTest marks an undefined / special result with these
        // tokens; the oracle only models finite arithmetic here.
        if case.expected == "?" || case.expected == "#" || case.expected.starts_with('#') {
            skipped += 1;
            continue;
        }
        let Ok((expected, _)) = Decimal128::parse_str(&case.expected, rm) else {
            skipped += 1;
            continue;
        };

        let f = Format::DECIMAL128;
        let r = match case.op.as_str() {
            "add" => oracle::add(&operands[0], &operands[1], f, rm),
            "subtract" => oracle::sub(&operands[0], &operands[1], f, rm),
            "multiply" => oracle::mul(&operands[0], &operands[1], f, rm),
            _ => oracle::fma(&operands[0], &operands[1], &operands[2], f, rm),
        };
        let (got, _) =
            Decimal128::parse_str(&r.decimal_string(), rm).expect("oracle string re-parses");
        let want_status = decode_conditions(&case.conditions);

        assert_eq!(
            got.to_bits(),
            expected.to_bits(),
            "[{}] {} {} {} {} rm={:?}: oracle {} ({:#034x}) != decTest {} ({:#034x})",
            case.id,
            case.op,
            case.operands[0],
            case.operands[1],
            case.expected,
            rm,
            got,
            got.to_bits(),
            expected,
            expected.to_bits(),
        );
        assert!(
            status_conformance_eq(r.status, want_status),
            "[{}] {} {} {} -> {} rm={:?}: oracle status {:?} != decTest {:?}",
            case.id,
            case.op,
            case.operands[0],
            case.operands[1],
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
fn oracle_matches_dectest_add_sub_mul() {
    let mut total = 0;
    for f in ["dqAdd.decTest", "dqSubtract.decTest", "dqMultiply.decTest"] {
        let (checked, skipped) = replay(f);
        eprintln!("{f}: {checked} oracle-checked, {skipped} out-of-scope skipped");
        total += checked;
    }
    // Sanity: the suite must actually exercise the oracle, not skip
    // everything (a regression that broke parsing would otherwise pass
    // silently).
    assert!(
        total > 1000,
        "expected the oracle to be exercised on >1000 decTest cases, got {total}"
    );
}
