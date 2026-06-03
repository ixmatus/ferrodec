//! Content-hash integrity check for vendored decTest fixtures (ADR-0042).
//!
//! Each vectors directory carries a committed `SHA256SUMS` manifest (the
//! standard `shasum -a 256` format, `<hex>  <name>`) pinning the SHA-256 of
//! every `*.decTest` file it vendors from the upstream archive. A default-on
//! test in each crate re-hashes the committed files and asserts they match,
//! so a silent byte drift, or a new vendored file that was never attested,
//! fails the build.
//!
//! This is the content-hash companion to the per-file pass-count pins
//! (ADR-0010): the pass-count pins guard *behavior* (a fixture change that
//! moves a result), this guards the *bytes* (the fixture is what we vetted
//! against the documented upstream archive SHA-256, see each dir's README).
//! Neither is adversarial; both detect accidental drift. The manifest is
//! regenerable with `cd <dir> && shasum -a 256 *.decTest > SHA256SUMS`.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use sha2::{Digest, Sha256};

/// Verify every `*.decTest` file in `dir` against `dir/SHA256SUMS`.
///
/// Panics with a precise diff when a file's SHA-256 differs from its pinned
/// value, when a pinned file is absent, or when a `*.decTest` file on disk is
/// not pinned (so a newly vendored fixture cannot slip in unattested). The
/// `SHA256SUMS` manifest itself carries no extension match, so it is neither
/// pinned nor scanned.
pub fn verify(dir: impl AsRef<Path>) {
    let dir = dir.as_ref();
    let manifest_path = dir.join("SHA256SUMS");
    let manifest = fs::read_to_string(&manifest_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", manifest_path.display()));

    // Pinned name -> hex from the manifest.
    let mut pinned: BTreeMap<String, String> = BTreeMap::new();
    for (i, line) in manifest.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (hex, name) = line.split_once(char::is_whitespace).unwrap_or_else(|| {
            panic!(
                "{}:{}: malformed manifest line {line:?}",
                manifest_path.display(),
                i + 1
            )
        });
        // `shasum` writes two spaces (text mode) or ` *` (binary mode) before
        // the name; tolerate either.
        let name = name.trim_start_matches([' ', '*']).to_string();
        pinned.insert(name, hex.to_string());
    }

    // Computed name -> hex for every `*.decTest` actually on disk.
    let mut on_disk: BTreeMap<String, String> = BTreeMap::new();
    for entry in fs::read_dir(dir).unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display())) {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|s| s.to_str()) != Some("decTest") {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let bytes = fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        on_disk.insert(name, hex(hasher.finalize().as_slice()));
    }

    let mut problems = Vec::new();
    for (name, computed) in &on_disk {
        match pinned.get(name) {
            None => problems.push(format!("UNPINNED  {name}  (on disk but not in SHA256SUMS)")),
            Some(want) if want != computed => {
                problems.push(format!("CHANGED   {name}  pinned {want}  got {computed}"));
            }
            Some(_) => {}
        }
    }
    for name in pinned.keys() {
        if !on_disk.contains_key(name) {
            problems.push(format!("MISSING   {name}  (in SHA256SUMS but not on disk)"));
        }
    }

    assert!(
        problems.is_empty(),
        "vendored fixture integrity failed for {} ({} mismatch(es)):\n  {}\n\
         If the change is intended, regenerate the manifest:\n  \
         (cd {} && shasum -a 256 *.decTest > SHA256SUMS)\n\
         and confirm the upstream archive SHA-256 in the directory README still \
         holds (ADR-0042).",
        dir.display(),
        problems.len(),
        problems.join("\n  "),
        dir.display(),
    );
}

/// Lowercase hex encoding of a byte slice.
fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}
