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
//! oracles. Each sibling asserts its faithful kernel (≤1 ULP,
//! ADR-0021) lands within one representable step of the proven
//! correctly-rounded value.

use std::fs;
use std::path::PathBuf;

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
    let dir = corpus_dir();
    let mut out = Vec::new();
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
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
