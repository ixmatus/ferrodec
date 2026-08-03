//! Loader for the Arb/FLINT frozen hard-to-round vector corpus
//! (`tests/vectors/transcend/`, Phase 2 of fd-cb6, ADR-0026).
//!
//! The corpus is committed data: the proven NearestEven
//! correctly-rounded value of each transcendental at a chosen
//! argument, including a decimal Table-Maker's-Dilemma worst-case
//! search. There is no oracle and no C-FFI in this path — it parses
//! checked-in text — so the per-crate frozen-vector tests are
//! default-on and run in standard CI under `--features
//! transcendentals`, unlike the gated astro-float / mpmath / MPFR
//! oracles. Each format's frozen-vector test asserts its kernel is
//! *correctly rounded* (ADR-0032, superseding ADR-0021's faithful
//! ≤1 ULP contract): the result equals the proven correctly-rounded
//! value exactly, not merely within one representable step.

use std::fs;
use std::path::{Path, PathBuf};

/// Corpus file stems that carry binary `func(input, input2)` vectors
/// (fd-97a); every other stem is unary.
pub const BINARY_FUNCS: &[&str] = &["pow", "atan2"];

/// One frozen vector: `func(input[, input2])` correctly rounds to
/// `output` at the filtered format precision under `mode`.
#[derive(Debug, Clone)]
pub struct FrozenVec {
    /// Function name (the corpus file stem, e.g. `exp`, `sin`).
    pub func: String,
    /// Rounding mode the proven `output` was rounded under
    /// (`NearestEven`, `TowardZero`, `TowardPositive`,
    /// `TowardNegative`, `NearestAway`).
    pub mode: String,
    /// Exact decimal input (`coef e exp`, parseable by `parse_str`).
    pub input: String,
    /// Second operand for the binary functions (`pow`, `atan2`);
    /// `None` for the unary functions.
    pub input2: Option<String>,
    /// Proven correctly-rounded value at the format precision under
    /// `mode`.
    pub output: String,
}

/// Absolute path to the corpus directory, resolved from this crate's
/// manifest dir (`<ws>/ferrodec-test-support`) to
/// `<ws>/tests/vectors/transcend` — the same `../` resolution the
/// differential harness uses for `tools/`.
fn corpus_dir() -> PathBuf {
    PathBuf::from(format!(
        "{}/../tests/vectors/transcend",
        env!("CARGO_MANIFEST_DIR")
    ))
}

/// Every frozen vector for `prec` significant digits, across all
/// `*.txt` corpus files, sorted by `(func, input)` so a failure
/// reproduces verbatim. Panics if the corpus directory is missing
/// (a checked-in artifact: its absence is a real breakage, not a
/// skip).
#[must_use]
pub fn load(prec: u32) -> Vec<FrozenVec> {
    load_from(&corpus_dir(), prec)
}

/// Expected per-`(func, mode)` vector counts for the sampled frozen
/// corpus ([`load`]), one table per format precision (Decimal128 p34,
/// Decimal64 p16, Decimal32 p7). Authored from a direct corpus scan
/// independent of [`load`]'s parse, so a loader regression that
/// silently drops or duplicates a bucket is caught even when the
/// corpus bytes are unchanged. Complements, not duplicates, the
/// `SHA256SUMS` byte pin (`corpus_integrity.rs`): the hash guards the
/// bytes, these guard the loader's interpretation of them. Replaces
/// the former aggregate `len() > 500` floor, which admitted silent
/// compensating drift between buckets (fd-aqs.10). Regenerate the
/// numbers alongside the corpus.
pub const EXPECTED_BUCKETS_P34: &[(&str, &str, usize)] = &[
    ("acos", "NearestEven", 46),
    ("acosh", "NearestEven", 45),
    ("asin", "NearestEven", 46),
    ("asinh", "NearestEven", 52),
    ("atan", "NearestAway", 21),
    ("atan", "NearestEven", 49),
    ("atan", "TowardNegative", 21),
    ("atan", "TowardPositive", 21),
    ("atan", "TowardZero", 21),
    ("atan2", "NearestAway", 50),
    ("atan2", "NearestEven", 50),
    ("atan2", "TowardNegative", 50),
    ("atan2", "TowardPositive", 50),
    ("atan2", "TowardZero", 50),
    ("atanh", "NearestEven", 48),
    ("cbrt", "NearestAway", 61),
    ("cbrt", "NearestEven", 89),
    ("cbrt", "TowardNegative", 61),
    ("cbrt", "TowardPositive", 61),
    ("cbrt", "TowardZero", 61),
    ("cos", "NearestAway", 46),
    ("cos", "NearestEven", 74),
    ("cos", "TowardNegative", 46),
    ("cos", "TowardPositive", 46),
    ("cos", "TowardZero", 46),
    ("cosh", "NearestEven", 51),
    ("exp", "NearestAway", 24),
    ("exp", "NearestEven", 46),
    ("exp", "TowardNegative", 24),
    ("exp", "TowardPositive", 24),
    ("exp", "TowardZero", 24),
    ("exp2", "NearestEven", 48),
    ("ln", "NearestAway", 66),
    ("ln", "NearestEven", 94),
    ("ln", "TowardNegative", 66),
    ("ln", "TowardPositive", 66),
    ("ln", "TowardZero", 66),
    ("log10", "NearestAway", 66),
    ("log10", "NearestEven", 94),
    ("log10", "TowardNegative", 62),
    ("log10", "TowardPositive", 62),
    ("log10", "TowardZero", 62),
    ("log2", "NearestEven", 94),
    ("logp1", "NearestAway", 58),
    ("logp1", "NearestEven", 86),
    ("logp1", "TowardNegative", 58),
    ("logp1", "TowardPositive", 58),
    ("logp1", "TowardZero", 58),
    ("pow", "NearestAway", 36),
    ("pow", "NearestEven", 36),
    ("pow", "TowardNegative", 33),
    ("pow", "TowardPositive", 34),
    ("pow", "TowardZero", 33),
    ("sin", "NearestAway", 46),
    ("sin", "NearestEven", 74),
    ("sin", "TowardNegative", 46),
    ("sin", "TowardPositive", 46),
    ("sin", "TowardZero", 46),
    ("sinh", "NearestEven", 43),
    ("tan", "NearestEven", 74),
    ("tanh", "NearestEven", 52),
];

/// Decimal64 (p16) per-`(func, mode)` counts; see [`EXPECTED_BUCKETS_P34`].
pub const EXPECTED_BUCKETS_P16: &[(&str, &str, usize)] = &[
    ("acos", "NearestEven", 46),
    ("acosh", "NearestEven", 45),
    ("asin", "NearestEven", 46),
    ("asinh", "NearestEven", 52),
    ("atan", "NearestAway", 21),
    ("atan", "NearestEven", 49),
    ("atan", "TowardNegative", 21),
    ("atan", "TowardPositive", 21),
    ("atan", "TowardZero", 21),
    ("atan2", "NearestAway", 50),
    ("atan2", "NearestEven", 50),
    ("atan2", "TowardNegative", 50),
    ("atan2", "TowardPositive", 50),
    ("atan2", "TowardZero", 50),
    ("atanh", "NearestEven", 48),
    ("cbrt", "NearestAway", 51),
    ("cbrt", "NearestEven", 79),
    ("cbrt", "TowardNegative", 51),
    ("cbrt", "TowardPositive", 51),
    ("cbrt", "TowardZero", 51),
    ("cos", "NearestAway", 41),
    ("cos", "NearestEven", 69),
    ("cos", "TowardNegative", 41),
    ("cos", "TowardPositive", 41),
    ("cos", "TowardZero", 41),
    ("cosh", "NearestEven", 17),
    ("exp", "NearestAway", 14),
    ("exp", "NearestEven", 16),
    ("exp", "TowardNegative", 16),
    ("exp", "TowardPositive", 14),
    ("exp", "TowardZero", 13),
    ("exp2", "NearestEven", 15),
    ("ln", "NearestAway", 56),
    ("ln", "NearestEven", 84),
    ("ln", "TowardNegative", 56),
    ("ln", "TowardPositive", 56),
    ("ln", "TowardZero", 56),
    ("log10", "NearestAway", 56),
    ("log10", "NearestEven", 84),
    ("log10", "TowardNegative", 52),
    ("log10", "TowardPositive", 52),
    ("log10", "TowardZero", 52),
    ("log2", "NearestEven", 84),
    ("logp1", "NearestAway", 58),
    ("logp1", "NearestEven", 86),
    ("logp1", "TowardNegative", 58),
    ("logp1", "TowardPositive", 58),
    ("logp1", "TowardZero", 58),
    ("pow", "NearestAway", 36),
    ("pow", "NearestEven", 36),
    ("pow", "TowardNegative", 34),
    ("pow", "TowardPositive", 34),
    ("pow", "TowardZero", 34),
    ("sin", "NearestAway", 41),
    ("sin", "NearestEven", 69),
    ("sin", "TowardNegative", 41),
    ("sin", "TowardPositive", 41),
    ("sin", "TowardZero", 41),
    ("sinh", "NearestEven", 14),
    ("tan", "NearestEven", 69),
    ("tanh", "NearestEven", 52),
];

/// Decimal32 (p7) per-`(func, mode)` counts; see [`EXPECTED_BUCKETS_P34`].
pub const EXPECTED_BUCKETS_P7: &[(&str, &str, usize)] = &[
    ("acos", "NearestEven", 46),
    ("acosh", "NearestEven", 45),
    ("asin", "NearestEven", 46),
    ("asinh", "NearestEven", 52),
    ("atan", "NearestAway", 21),
    ("atan", "NearestEven", 49),
    ("atan", "TowardNegative", 21),
    ("atan", "TowardPositive", 21),
    ("atan", "TowardZero", 21),
    ("atan2", "NearestAway", 50),
    ("atan2", "NearestEven", 50),
    ("atan2", "TowardNegative", 50),
    ("atan2", "TowardPositive", 50),
    ("atan2", "TowardZero", 50),
    ("atanh", "NearestEven", 48),
    ("cbrt", "NearestAway", 43),
    ("cbrt", "NearestEven", 71),
    ("cbrt", "TowardNegative", 43),
    ("cbrt", "TowardPositive", 43),
    ("cbrt", "TowardZero", 43),
    ("cos", "NearestAway", 37),
    ("cos", "NearestEven", 65),
    ("cos", "TowardNegative", 37),
    ("cos", "TowardPositive", 37),
    ("cos", "TowardZero", 37),
    ("cosh", "NearestEven", 22),
    ("exp", "NearestAway", 15),
    ("exp", "NearestEven", 18),
    ("exp", "TowardNegative", 16),
    ("exp", "TowardPositive", 16),
    ("exp", "TowardZero", 17),
    ("exp2", "NearestEven", 21),
    ("ln", "NearestAway", 48),
    ("ln", "NearestEven", 76),
    ("ln", "TowardNegative", 48),
    ("ln", "TowardPositive", 48),
    ("ln", "TowardZero", 48),
    ("log10", "NearestAway", 48),
    ("log10", "NearestEven", 76),
    ("log10", "TowardNegative", 44),
    ("log10", "TowardPositive", 44),
    ("log10", "TowardZero", 44),
    ("log2", "NearestEven", 76),
    ("logp1", "NearestAway", 55),
    ("logp1", "NearestEven", 83),
    ("logp1", "TowardNegative", 55),
    ("logp1", "TowardPositive", 55),
    ("logp1", "TowardZero", 55),
    ("pow", "NearestAway", 36),
    ("pow", "NearestEven", 36),
    ("pow", "TowardNegative", 35),
    ("pow", "TowardPositive", 35),
    ("pow", "TowardZero", 35),
    ("sin", "NearestAway", 37),
    ("sin", "NearestEven", 65),
    ("sin", "TowardNegative", 37),
    ("sin", "TowardPositive", 37),
    ("sin", "TowardZero", 37),
    ("sinh", "NearestEven", 18),
    ("tan", "NearestEven", 65),
    ("tanh", "NearestEven", 52),
];

/// Assert the loaded corpus has exactly the expected per-`(func,
/// mode)` bucket counts, panicking with a precise diff on any
/// missing, extra, or miscounted bucket. Replaces the aggregate
/// `len() > 500` floor, which admitted silent compensating drift
/// between buckets (fd-aqs.10).
pub fn assert_bucket_counts(vectors: &[FrozenVec], expected: &[(&str, &str, usize)]) {
    use std::collections::BTreeMap;
    let mut actual: BTreeMap<(&str, &str), usize> = BTreeMap::new();
    for v in vectors {
        *actual
            .entry((v.func.as_str(), v.mode.as_str()))
            .or_insert(0) += 1;
    }
    let mut want: BTreeMap<(&str, &str), usize> = BTreeMap::new();
    for &(f, m, n) in expected {
        want.insert((f, m), n);
    }
    let mut problems = Vec::new();
    for (&(f, m), &got) in &actual {
        match want.get(&(f, m)) {
            None => problems.push(format!(
                "EXTRA    {f}/{m}: {got} (bucket not in expected table)"
            )),
            Some(&exp) if exp != got => {
                problems.push(format!("COUNT    {f}/{m}: expected {exp}, got {got}"));
            }
            Some(_) => {}
        }
    }
    for &(f, m, exp) in expected {
        if !actual.contains_key(&(f, m)) {
            problems.push(format!("MISSING  {f}/{m}: expected {exp}, got 0"));
        }
    }
    assert!(
        problems.is_empty(),
        "frozen corpus per-(func,mode) bucket pin mismatch ({} problem(s)):\n  {}\n\
         If the corpus was regenerated deliberately, update the matching \
         EXPECTED_BUCKETS_* table in ferrodec-test-support/src/frozen.rs and \
         the SHA256SUMS manifest (fd-aqs.10).",
        problems.len(),
        problems.join("\n  ")
    );
}

/// Every near-anchor band vector (fd-aqs.6) for `prec` significant
/// digits, from the `tests/vectors/transcend/anchor_bands/`
/// subdirectory. Same line format as the sampled corpus (`pow`
/// lines are binary). The band corpus pins the hazard decades
/// around the additive anchors 0 and 1, where the 2026-06-09 review
/// found the kernel's relative error model collapsing to absolute
/// (mis-rounding `ln`/`log10`/`log2` just below 1, `atanh`/`asinh`
/// small arguments, `asin`/`acos` near ±1, and `pow` through
/// `y · ln x`). See `tools/gen_anchor_band_vectors.py` for the
/// oracle and acceptance rule; directed-mode lines appear only
/// where a 50-digit-correct kernel can decide them (the rest are
/// the fd-aqs.7 enclosure contract). Panics if the subdirectory is
/// missing, matching the `load` semantics.
#[must_use]
pub fn load_anchor_bands(prec: u32) -> Vec<FrozenVec> {
    load_from(&corpus_dir().join("anchor_bands"), prec)
}

/// Shared directory walk for [`load`] and [`load_anchor_bands`]:
/// every `*.txt` in `dir`, filtered to `prec`, unary or binary line
/// shape by [`BINARY_FUNCS`] stem.
fn load_from(dir: &Path, prec: u32) -> Vec<FrozenVec> {
    let mut out = Vec::new();
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("frozen corpus directory {}: {e}", dir.display()))
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("txt"))
        .collect();
    files.sort();
    let want = prec.to_string();
    for path in files {
        let func = path
            .file_stem()
            .and_then(|s| s.to_str())
            .expect("corpus file stem")
            .to_string();
        let binary = BINARY_FUNCS.contains(&func.as_str());
        let text = fs::read_to_string(&path).expect("read corpus file");
        for line in text.lines() {
            if line.starts_with('#') || line.is_empty() {
                continue;
            }
            // Unary: `<prec> <mode> <input> <output>`
            // Binary: `<prec> <mode> <input1> <input2> <output>`
            let mut it = line.split_whitespace();
            let Some(p) = it.next() else { continue };
            if p != want {
                continue;
            }
            let rest: Vec<&str> = it.collect();
            let (mode, input, input2, output) = match (binary, rest.as_slice()) {
                (false, [mode, input, output]) => (
                    (*mode).to_string(),
                    (*input).to_string(),
                    None,
                    (*output).to_string(),
                ),
                (true, [mode, input, input2, output]) => (
                    (*mode).to_string(),
                    (*input).to_string(),
                    Some((*input2).to_string()),
                    (*output).to_string(),
                ),
                _ => panic!("malformed frozen line in {}: {line:?}", path.display()),
            };
            out.push(FrozenVec {
                func: func.clone(),
                mode,
                input,
                input2,
                output,
            });
        }
    }
    out.sort_by(|a, b| {
        (&a.func, &a.mode, &a.input, &a.input2).cmp(&(&b.func, &b.mode, &b.input, &b.input2))
    });
    out
}

/// Every exhaustive-sweep worst-case vector (ADR-0033 Plan C4, fd-ykr.4)
/// for `prec` significant digits. Loads from the
/// `tests/vectors/transcend/exhaustive/` subdirectory; each `<fn>.txt`
/// there carries one line: the input that achieved the smallest
/// half-ULP margin across the function's full canonical Decimal32
/// input set, paired with the proven correctly-rounded output. The
/// file format and line format match the sampled corpus's
/// `<fn>.txt`; the subdirectory placement keeps the existing `load`
/// loader unaware (so the default frozen test continues to assert
/// against the sampled corpus only).
///
/// Panics if the exhaustive subdirectory is missing, matching the
/// `load` semantics: the absence of checked-in data is a real
/// breakage, not a skip.
#[must_use]
pub fn load_exhaustive(prec: u32) -> Vec<FrozenVec> {
    let dir = corpus_dir().join("exhaustive");
    let mut out = Vec::new();
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("exhaustive corpus directory {}: {e}", dir.display()))
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("txt"))
        .collect();
    files.sort();
    let want = prec.to_string();
    for path in files {
        let func = path
            .file_stem()
            .and_then(|s| s.to_str())
            .expect("exhaustive corpus file stem")
            .to_string();
        let text = fs::read_to_string(&path).expect("read exhaustive corpus file");
        for line in text.lines() {
            if line.starts_with('#') || line.is_empty() {
                continue;
            }
            // Same `<prec> <mode> <input> <output>` unary format as
            // the sampled corpus (binary surface is out of scope for
            // ADR-0033 Plan C4).
            let mut it = line.split_whitespace();
            let Some(p) = it.next() else { continue };
            if p != want {
                continue;
            }
            let rest: Vec<&str> = it.collect();
            let [mode, input, output] = rest.as_slice() else {
                panic!("malformed exhaustive line in {}: {line:?}", path.display());
            };
            out.push(FrozenVec {
                func: func.clone(),
                mode: (*mode).to_string(),
                input: (*input).to_string(),
                input2: None,
                output: (*output).to_string(),
            });
        }
    }
    out.sort_by(|a, b| (&a.func, &a.input).cmp(&(&b.func, &b.input)));
    out
}
