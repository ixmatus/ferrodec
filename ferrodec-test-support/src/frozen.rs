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

/// One frozen vector: `func(input)` correctly rounds to `output` at
/// the format precision the caller filtered on.
#[derive(Debug, Clone)]
pub struct FrozenVec {
    /// Function name (the corpus file stem, e.g. `exp`, `sin`).
    pub func: String,
    /// Exact decimal input (`coef e exp`, parseable by `parse_str`).
    pub input: String,
    /// Proven correctly-rounded (`NearestEven`) value at `prec`
    /// significant digits.
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
    let pfx = format!("{prec} ");
    for path in files {
        let func = path
            .file_stem()
            .and_then(|s| s.to_str())
            .expect("corpus file stem")
            .to_string();
        let text = fs::read_to_string(&path).expect("read corpus file");
        for line in text.lines() {
            if line.starts_with('#') || line.is_empty() {
                continue;
            }
            // `<prec> <input> <output>`
            if let Some(rest) = line.strip_prefix(&pfx) {
                let mut it = rest.split_whitespace();
                let (Some(input), Some(output), None) = (it.next(), it.next(), it.next()) else {
                    panic!("malformed frozen line in {}: {line:?}", path.display());
                };
                out.push(FrozenVec {
                    func: func.clone(),
                    input: input.to_string(),
                    output: output.to_string(),
                });
            }
        }
    }
    out.sort_by(|a, b| (&a.func, &a.input).cmp(&(&b.func, &b.input)));
    out
}
