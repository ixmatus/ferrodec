//! S1 witness replay: the ADR-0059 falsification corpus turned
//! regression gate (M8, fd-4zo.16).
//!
//! The S1 probe swept high-decade `Decimal128` trig (decades the
//! sampled corpus never reached, full 34-digit coefficients) against
//! the shipped 50-digit kernel and committed every Arb-certified
//! misround under `tests/vectors/transcend/campaign/s1/` — the rows
//! that falsified the shipped correctly-rounded claim. Each row
//! carries the input, the rounding mode, the certified correctly
//! rounded value (Arb via python-flint at `CAP_BITS` 65536, spot
//! confirmed by mpmath), and the then-production output, which
//! differed by construction.
//!
//! The M8 escalation ladder must turn every row green: the witness
//! inputs sit within rung 1's honest trig budget of a rounding
//! boundary (the 38-digit π/2 truncation item), so the predicate
//! escalates them and rung 2's `reduce_wide` (analytic `< 10^-114`
//! truncation) resolves the side. Row counts are pinned exactly per
//! file — the regression-guard discipline: a witness silently
//! dropped from the corpus must fail this gate, not shrink it.

#![cfg(feature = "trig")]

use ferrodec::{Decimal128, RoundingMode, Status};
use std::fs;
use std::path::PathBuf;

fn parse(s: &str) -> Decimal128 {
    Decimal128::parse_str(s, RoundingMode::NearestEven)
        .unwrap_or_else(|e| panic!("parse {s:?}: {e:?}"))
        .0
}

fn mode(s: &str) -> RoundingMode {
    match s {
        "NearestEven" => RoundingMode::NearestEven,
        "NearestAway" => RoundingMode::NearestAway,
        "TowardZero" => RoundingMode::TowardZero,
        "TowardPositive" => RoundingMode::TowardPositive,
        "TowardNegative" => RoundingMode::TowardNegative,
        other => panic!("unknown mode {other:?}"),
    }
}

/// Replay one witness file; returns the number of rows exercised.
fn replay(file: &str, f: impl Fn(Decimal128, RoundingMode) -> (Decimal128, Status)) -> usize {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/vectors/transcend/campaign/s1")
        .join(file);
    let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let mut rows = 0;
    for (lineno, line) in text.lines().enumerate() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        assert_eq!(
            fields.len(),
            8,
            "{file}:{}: malformed witness row",
            lineno + 1
        );
        assert_eq!(fields[0], "MISROUND", "{file}:{}", lineno + 1);
        let x = parse(fields[4]);
        let rm = mode(fields[5]);
        let want = parse(fields[6]);
        let was = parse(fields[7]);
        let (got, status) = f(x, rm);
        assert_eq!(
            got.partial_cmp(want).0,
            Some(core::cmp::Ordering::Equal),
            "{file}:{}: {}({}) {rm:?}: got {got:?}, certified {}",
            lineno + 1,
            fields[2],
            fields[4],
            fields[6],
        );
        // The certified value differs from the misround by
        // construction; a row where they agree is corpus corruption.
        assert_ne!(
            want.partial_cmp(was).0,
            Some(core::cmp::Ordering::Equal),
            "{file}:{}: witness row is not a misround",
            lineno + 1
        );
        assert!(
            status.inexact(),
            "{file}:{}: trig at a nonzero input must raise INEXACT",
            lineno + 1
        );
        rows += 1;
    }
    rows
}

#[test]
fn s1_sin_witnesses_all_fixed() {
    assert_eq!(replay("sin_misrounds.tsv", Decimal128::sin), 643);
}

#[test]
fn s1_cos_witnesses_all_fixed() {
    assert_eq!(replay("cos_misrounds.tsv", Decimal128::cos), 570);
}

#[test]
fn s1_tan_witnesses_all_fixed() {
    assert_eq!(replay("tan_misrounds.tsv", Decimal128::tan), 606);
}
