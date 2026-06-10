//! Near-anchor band gate for `Decimal64` (fd-aqs.6) — the sibling
//! mirror of the root crate's `tests/transcend_anchor_bands.rs`,
//! replaying the shared committed corpus
//! (`tests/vectors/transcend/anchor_bands/`) at 16 significant
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

use ferrodec_decimal64::{Decimal64, RoundingMode};
use ferrodec_test_support::frozen;

const PREC: u32 = 16;

/// Exact expected count per `(func, mode)` bucket at `PREC`.
/// Regenerating the corpus updates these pins in the same commit.
const EXPECTED: &[(&str, &str, usize)] = &[
    ("acos", "NearestEven", 8),
    ("acos", "NearestAway", 8),
    ("acos", "TowardZero", 8),
    ("acos", "TowardPositive", 8),
    ("acos", "TowardNegative", 8),
    ("asin", "NearestEven", 5),
    ("asin", "NearestAway", 5),
    ("asin", "TowardZero", 5),
    ("asin", "TowardPositive", 5),
    ("asin", "TowardNegative", 5),
    ("asinh", "NearestEven", 8),
    ("asinh", "NearestAway", 8),
    ("asinh", "TowardZero", 8),
    ("asinh", "TowardPositive", 8),
    ("asinh", "TowardNegative", 8),
    ("atan", "NearestEven", 8),
    ("atan", "NearestAway", 8),
    ("atan", "TowardZero", 8),
    ("atan", "TowardPositive", 8),
    ("atan", "TowardNegative", 8),
    ("atanh", "NearestEven", 8),
    ("atanh", "NearestAway", 8),
    ("atanh", "TowardZero", 8),
    ("atanh", "TowardPositive", 8),
    ("atanh", "TowardNegative", 8),
    ("cos", "NearestEven", 3),
    ("cos", "NearestAway", 3),
    ("cos", "TowardZero", 3),
    ("cos", "TowardPositive", 3),
    ("cos", "TowardNegative", 3),
    ("cosh", "NearestEven", 3),
    ("cosh", "NearestAway", 3),
    ("cosh", "TowardZero", 3),
    ("cosh", "TowardPositive", 3),
    ("cosh", "TowardNegative", 3),
    ("exp", "NearestEven", 4),
    ("exp", "NearestAway", 4),
    ("exp", "TowardZero", 4),
    ("exp", "TowardPositive", 4),
    ("exp", "TowardNegative", 4),
    ("exp2", "NearestEven", 4),
    ("exp2", "NearestAway", 4),
    ("exp2", "TowardZero", 4),
    ("exp2", "TowardPositive", 4),
    ("exp2", "TowardNegative", 4),
    ("ln", "NearestEven", 8),
    ("ln", "NearestAway", 8),
    ("ln", "TowardZero", 8),
    ("ln", "TowardPositive", 8),
    ("ln", "TowardNegative", 8),
    ("log10", "NearestEven", 8),
    ("log10", "NearestAway", 8),
    ("log10", "TowardZero", 8),
    ("log10", "TowardPositive", 8),
    ("log10", "TowardNegative", 8),
    ("log2", "NearestEven", 8),
    ("log2", "NearestAway", 8),
    ("log2", "TowardZero", 8),
    ("log2", "TowardPositive", 8),
    ("log2", "TowardNegative", 8),
    ("pow", "NearestEven", 2),
    ("pow", "NearestAway", 2),
    ("pow", "TowardZero", 2),
    ("pow", "TowardPositive", 2),
    ("pow", "TowardNegative", 2),
    ("sin", "NearestEven", 8),
    ("sin", "NearestAway", 8),
    ("sin", "TowardZero", 8),
    ("sin", "TowardPositive", 8),
    ("sin", "TowardNegative", 8),
    ("sinh", "NearestEven", 8),
    ("sinh", "NearestAway", 8),
    ("sinh", "TowardZero", 8),
    ("sinh", "TowardPositive", 8),
    ("sinh", "TowardNegative", 8),
    ("tan", "NearestEven", 8),
    ("tan", "NearestAway", 8),
    ("tan", "TowardZero", 8),
    ("tan", "TowardPositive", 8),
    ("tan", "TowardNegative", 8),
    ("tanh", "NearestEven", 8),
    ("tanh", "NearestAway", 8),
    ("tanh", "TowardZero", 8),
    ("tanh", "TowardPositive", 8),
    ("tanh", "TowardNegative", 8),
];

fn parse(s: &str) -> Decimal64 {
    Decimal64::parse_str(s, RoundingMode::NearestEven)
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

fn kernel(v: &frozen::FrozenVec, rm: RoundingMode) -> Decimal64 {
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
        "sin" => x.sin(rm).0,
        "cos" => x.cos(rm).0,
        "tan" => x.tan(rm).0,
        "atan" => x.atan(rm).0,
        "sinh" => x.sinh(rm).0,
        "cosh" => x.cosh(rm).0,
        "tanh" => x.tanh(rm).0,
        "exp" => x.exp(rm).0,
        "exp2" => x.exp2(rm).0,
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
