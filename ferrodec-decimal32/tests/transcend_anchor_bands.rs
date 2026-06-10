//! Near-anchor band gate for `Decimal32` (fd-aqs.6) — the sibling
//! mirror of the root crate's `tests/transcend_anchor_bands.rs`,
//! replaying the shared committed corpus
//! (`tests/vectors/transcend/anchor_bands/`) at 7 significant
//! digits. See the root file and `tools/gen_anchor_band_vectors.py`
//! for the hazard-band rationale, the oracle, and the acceptance
//! rule. Per-(function, mode) counts are pinned exactly.

#![cfg(all(
    feature = "exp-log",
    feature = "trig",
    feature = "hyperbolic",
    feature = "pow"
))]

use core::cmp::Ordering;
use std::collections::BTreeMap;

use ferrodec_decimal32::{Decimal32, RoundingMode};
use ferrodec_test_support::frozen;

const PREC: u32 = 7;

/// Exact expected count per `(func, mode)` bucket at `PREC`.
/// Regenerating the corpus updates these pins in the same commit.
const EXPECTED: &[(&str, &str, usize)] = &[
    ("acos", "NearestEven", 6),
    ("acos", "NearestAway", 6),
    ("acos", "TowardZero", 6),
    ("acos", "TowardPositive", 6),
    ("acos", "TowardNegative", 6),
    ("asin", "NearestEven", 3),
    ("asin", "NearestAway", 3),
    ("asin", "TowardZero", 3),
    ("asin", "TowardPositive", 3),
    ("asin", "TowardNegative", 3),
    ("asinh", "NearestEven", 6),
    ("asinh", "NearestAway", 6),
    ("asinh", "TowardZero", 4),
    ("asinh", "TowardPositive", 4),
    ("asinh", "TowardNegative", 4),
    ("atanh", "NearestEven", 6),
    ("atanh", "NearestAway", 6),
    ("atanh", "TowardZero", 4),
    ("atanh", "TowardPositive", 4),
    ("atanh", "TowardNegative", 4),
    ("ln", "NearestEven", 5),
    ("ln", "NearestAway", 5),
    ("ln", "TowardZero", 5),
    ("ln", "TowardPositive", 5),
    ("ln", "TowardNegative", 5),
    ("log10", "NearestEven", 5),
    ("log10", "NearestAway", 5),
    ("log10", "TowardZero", 5),
    ("log10", "TowardPositive", 5),
    ("log10", "TowardNegative", 5),
    ("log2", "NearestEven", 5),
    ("log2", "NearestAway", 5),
    ("log2", "TowardZero", 5),
    ("log2", "TowardPositive", 5),
    ("log2", "TowardNegative", 5),
    ("pow", "NearestEven", 2),
    ("pow", "NearestAway", 2),
    ("pow", "TowardZero", 2),
    ("pow", "TowardPositive", 2),
    ("pow", "TowardNegative", 2),
];

fn parse(s: &str) -> Decimal32 {
    Decimal32::parse_str(s, RoundingMode::NearestEven)
        .unwrap_or_else(|_| panic!("anchor-band token parses: {s:?}"))
        .0
}

fn mode(s: &str) -> RoundingMode {
    match s {
        "NearestEven" => RoundingMode::NearestEven,
        "NearestAway" => RoundingMode::NearestAway,
        "TowardZero" => RoundingMode::TowardZero,
        "TowardPositive" => RoundingMode::TowardPositive,
        "TowardNegative" => RoundingMode::TowardNegative,
        other => panic!("anchor-band corpus has an unknown rounding mode {other:?}"),
    }
}

fn kernel(v: &frozen::FrozenVec, rm: RoundingMode) -> Decimal32 {
    let x = parse(&v.input);
    match v.func.as_str() {
        "pow" => x.pow(parse(v.input2.as_deref().expect("pow input2")), rm).0,
        "ln" => x.ln(rm).0,
        "log10" => x.log10(rm).0,
        "log2" => x.log2(rm).0,
        "atanh" => x.atanh(rm).0,
        "asinh" => x.asinh(rm).0,
        "asin" => x.asin(rm).0,
        "acos" => x.acos(rm).0,
        other => panic!("anchor-band corpus has no kernel mapping for {other:?}"),
    }
}

#[test]
fn anchor_band_vectors_correctly_rounded() {
    let vectors = frozen::load_anchor_bands(PREC);
    let mut by_bucket: BTreeMap<(String, String), usize> = BTreeMap::new();
    for v in &vectors {
        let rm = mode(&v.mode);
        let cr = parse(&v.output);
        let got = kernel(v, rm);
        assert_eq!(
            got.partial_cmp(cr).0,
            Some(Ordering::Equal),
            "anchor-band contract violated [{}]: {}({}{}) -> ferrodec {} | \
             correctly rounded {}",
            v.mode,
            v.func,
            v.input,
            v.input2
                .as_deref()
                .map(|y| format!(", {y}"))
                .unwrap_or_default(),
            got,
            cr
        );
        *by_bucket
            .entry((v.func.clone(), v.mode.clone()))
            .or_insert(0) += 1;
    }
    // Exact per-bucket pins: a regenerated corpus that drops a
    // function or a mode fails here rather than passing silently.
    for (func, md, want) in EXPECTED {
        let got = by_bucket
            .get(&((*func).to_string(), (*md).to_string()))
            .copied()
            .unwrap_or(0);
        assert_eq!(
            got, *want,
            "anchor-band bucket {func}/{md}: {got} vectors, pinned {want}"
        );
    }
    let pinned: usize = EXPECTED.iter().map(|(_, _, n)| n).sum();
    assert_eq!(
        vectors.len(),
        pinned,
        "anchor-band corpus carries unpinned vectors at p{PREC}"
    );
}
