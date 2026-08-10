//! Replay of the Lefèvre–Stehlé–Zimmermann decimal64 `exp` worst
//! cases (fd-4zo.7): the only externally certified worst-case table
//! in any IEEE decimal format, recertified through Arb by
//! `tools/gen_lsz_d64_exp.py` (which reproduces the paper's own
//! claimed digit expansions in ball arithmetic before emitting a
//! line, so a transcription error cannot reach this gate) and frozen
//! at `tests/vectors/transcend/external/lsz_d64_exp.txt`.
//!
//! Every input's exp sits within 1e-15 ulp of a rounding breakpoint —
//! the published output of a lattice-reduction search this repository
//! could not cheaply reproduce — so agreement here is calibration
//! from a fully independent lineage: their search, their table, our
//! Arb recertification, this kernel. The corpus is positive-argument
//! only (the paper's search covered positives; negatives were still
//! running at publication), which the registry entry
//! (`docs/references/lefevre-stehle-zimmermann-d64-exp.md`) records
//! as the coverage gap.
//!
//! The pins are exact per mode (34 inputs × 5 modes), per the
//! regression-guard discipline.

#![cfg(feature = "exp-log")]

use std::collections::BTreeMap;
use std::path::PathBuf;

use ferrodec_decimal64::{Decimal64, RoundingMode};

const CORPUS: &str = "../tests/vectors/transcend/external/lsz_d64_exp.txt";
const INPUTS_PER_MODE: usize = 34;

fn parse(s: &str) -> Decimal64 {
    Decimal64::parse_str(s, RoundingMode::NearestEven)
        .unwrap_or_else(|_| panic!("frozen token parses: {s:?}"))
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

#[test]
fn lsz_worst_cases_replay_correctly_rounded() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(CORPUS);
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));

    let mut per_mode: BTreeMap<String, usize> = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        assert_eq!(fields.len(), 4, "malformed corpus line {line:?}");
        assert_eq!(fields[0], "16", "the LSZ table is decimal64 only");
        let rm = mode(fields[1]);
        let x = parse(fields[2]);
        let want = parse(fields[3]);

        let (got, _) = x.exp(rm);
        assert_eq!(
            got.partial_cmp(want).0,
            Some(core::cmp::Ordering::Equal),
            "LSZ worst case violated [{}]: exp({}) -> ferrodec {} | \
             certified {}",
            fields[1],
            fields[2],
            got,
            want
        );
        *per_mode.entry(fields[1].to_string()).or_insert(0) += 1;
    }

    // Exact per-mode pins: a regenerated corpus that drops an input
    // or a mode fails here rather than passing silently.
    assert_eq!(per_mode.len(), 5, "a rounding mode went missing");
    for (m, n) in &per_mode {
        assert_eq!(
            *n, INPUTS_PER_MODE,
            "LSZ bucket {m}: {n} vectors, pinned {INPUTS_PER_MODE}"
        );
    }
}
