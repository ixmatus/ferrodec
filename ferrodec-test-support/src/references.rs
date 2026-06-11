//! Schema guard for the `docs/references/` registry (ADR-0052).
//!
//! Each registry entry is one markdown file with a frontmatter block in a
//! deliberately constrained YAML subset: `---` fences, scalar `key: value`
//! lines, and one level of `- item` lists. This module parses that subset
//! (rejecting anything fancier, by design) and exposes the checks the
//! default-on `tests/references_integrity.rs` asserts: required keys per
//! category, enumerated field values, slug/filename agreement, INDEX.md
//! synchronization in both directions, consumer path existence, and
//! vendored-copy hash integrity. No network anywhere: liveness of the
//! recorded URLs is a manual concern; the `archived` field is the hedge.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

/// External source categories: every frontmatter field is required.
pub const EXTERNAL_CATEGORIES: [&str; 5] =
    ["spec", "conformance", "oracle", "algorithm", "history"];

/// Internal document categories: external fields are pinned to `n/a`.
pub const INTERNAL_CATEGORIES: [&str; 4] = ["registry", "glossary", "verification", "failure"];

const ROT_RISKS: [&str; 6] = [
    "died-once",
    "single-maintainer",
    "community-run",
    "academic-personal",
    "stable-publisher",
    "ephemeral",
];

const SCALAR_KEYS: [&str; 14] = [
    "slug",
    "category",
    "citation",
    "canonical",
    "doi",
    "archived",
    "archive-date",
    "retrieved",
    "sha256",
    "license",
    "vendor-status",
    "rot-risk",
    "provenance",
    "notes",
];

const LIST_KEYS: [&str; 2] = ["consumers", "verification"];

/// One parsed registry entry.
#[derive(Debug)]
pub struct Entry {
    /// Filename stem, asserted equal to the `slug` field.
    pub stem: String,
    /// Scalar frontmatter fields by key.
    pub scalars: BTreeMap<String, String>,
    /// List frontmatter fields by key.
    pub lists: BTreeMap<String, Vec<String>>,
    /// Markdown body after the closing fence.
    pub body: String,
    /// Path the entry was read from.
    pub path: PathBuf,
}

impl Entry {
    /// Scalar field by key; panics if absent (call after schema validation).
    pub fn scalar(&self, key: &str) -> &str {
        self.scalars
            .get(key)
            .unwrap_or_else(|| panic!("{}: missing field {key}", self.path.display()))
    }
}

/// The registry directory, `<workspace root>/docs/references`, located
/// relative to this crate's manifest so the guard runs from any test cwd.
pub fn registry_dir() -> PathBuf {
    workspace_root().join("docs/references")
}

/// The workspace root (parent of this crate's manifest directory).
pub fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("ferrodec-test-support sits one level under the workspace root")
        .to_path_buf()
}

/// Parse every entry file in the registry directory.
///
/// `SCHEMA.md` and `INDEX.md` are the only non-entry files allowed; any
/// other `*.md` must parse as an entry, and nothing but `vendor/` may be a
/// subdirectory. Panics with a problem list on any structural violation.
pub fn parse_all() -> Vec<Entry> {
    let dir = registry_dir();
    let mut entries = Vec::new();
    let mut problems = Vec::new();

    for dirent in fs::read_dir(&dir).unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display())) {
        let path = dirent.expect("dir entry").path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if path.is_dir() {
            if name != "vendor" {
                problems.push(format!("unexpected subdirectory {name}/"));
            }
            continue;
        }
        if name == "SCHEMA.md" || name == "INDEX.md" {
            continue;
        }
        match name.strip_suffix(".md") {
            None => problems.push(format!("non-markdown file {name}")),
            Some(stem) => match parse_entry(&path, stem) {
                Ok(entry) => entries.push(entry),
                Err(e) => problems.push(format!("{name}: {e}")),
            },
        }
    }

    assert!(
        problems.is_empty(),
        "registry structure/parse failed for {} ({} problem(s)):\n  {}",
        dir.display(),
        problems.len(),
        problems.join("\n  "),
    );
    entries.sort_by(|a, b| a.stem.cmp(&b.stem));
    entries
}

/// Validate one entry's schema: key set, per-category requirements, and
/// enumerated field formats. Returns the problems found (empty when clean).
pub fn schema_problems(entry: &Entry) -> Vec<String> {
    let mut problems = Vec::new();
    let p = |msg: String| format!("{}: {msg}", entry.path.display());

    for key in SCALAR_KEYS {
        match entry.scalars.get(key) {
            None => problems.push(p(format!("missing field {key}"))),
            Some(v) if v.is_empty() => problems.push(p(format!("empty field {key}"))),
            Some(_) => {}
        }
    }
    for key in LIST_KEYS {
        match entry.lists.get(key) {
            None => problems.push(p(format!("missing list {key}"))),
            Some(v) if v.is_empty() => problems.push(p(format!("empty list {key}"))),
            Some(_) => {}
        }
    }
    for key in entry.scalars.keys() {
        if !SCALAR_KEYS.contains(&key.as_str()) {
            problems.push(p(format!("unknown field {key}")));
        }
    }
    for key in entry.lists.keys() {
        if !LIST_KEYS.contains(&key.as_str()) {
            problems.push(p(format!("unknown list {key}")));
        }
    }
    if !problems.is_empty() {
        return problems; // field presence failed; format checks would cascade
    }

    let slug = entry.scalar("slug");
    if slug != entry.stem {
        problems.push(p(format!(
            "slug {slug:?} does not match filename stem {:?}",
            entry.stem
        )));
    }

    let category = entry.scalar("category");
    let external = EXTERNAL_CATEGORIES.contains(&category);
    let internal = INTERNAL_CATEGORIES.contains(&category);
    if !external && !internal {
        problems.push(p(format!("unknown category {category:?}")));
        return problems;
    }

    let check_date = |key: &str, problems: &mut Vec<String>| {
        let v = entry.scalar(key);
        if v != "n/a" && !is_iso_date(v) {
            problems.push(p(format!("{key} {v:?} is neither YYYY-MM-DD nor n/a")));
        }
    };

    if external {
        check_date("archive-date", &mut problems);
        check_date("retrieved", &mut problems);

        let doi = entry.scalar("doi");
        if doi != "none" && !doi.starts_with("10.") {
            problems.push(p(format!("doi {doi:?} is neither a 10.* DOI nor none")));
        }
        let archived = entry.scalar("archived");
        let archived_ok = archived.starts_with("https://web.archive.org/web/")
            || (archived.starts_with("none (") && archived.ends_with(')'));
        if !archived_ok {
            problems.push(p(format!(
                "archived {archived:?} is neither a Wayback URL nor none (reason)"
            )));
        }
        let sha = entry.scalar("sha256");
        if sha != "n/a" && !(sha.len() == 64 && sha.bytes().all(|b| b.is_ascii_hexdigit())) {
            problems.push(p(format!(
                "sha256 {sha:?} is neither 64 hex digits nor n/a"
            )));
        }
        let rot = entry.scalar("rot-risk");
        if !ROT_RISKS.contains(&rot) {
            problems.push(p(format!("rot-risk {rot:?} not in {ROT_RISKS:?}")));
        }
        let vendor = entry.scalar("vendor-status");
        let vendor_ok = vendor.starts_with("vendored-at-path ")
            || vendor == "pointer-only"
            || vendor == "legally-cannot"
            || vendor == "paper-copy-owned";
        if !vendor_ok {
            problems.push(p(format!("vendor-status {vendor:?} malformed")));
        }
        let prov = entry.scalar("provenance");
        if prov != "primary" && prov != "secondary" {
            problems.push(p(format!("provenance {prov:?} not primary|secondary")));
        }
    } else {
        for key in [
            "canonical",
            "doi",
            "archived",
            "archive-date",
            "retrieved",
            "sha256",
            "rot-risk",
            "vendor-status",
        ] {
            let v = entry.scalar(key);
            if v != "n/a" {
                problems.push(p(format!(
                    "internal category requires {key}: n/a, got {v:?}"
                )));
            }
        }
        if entry.scalar("license") != "repo (MIT OR Apache-2.0)" {
            problems.push(p(
                "internal category requires license: repo (MIT OR Apache-2.0)".to_string(),
            ));
        }
        if entry.scalar("provenance") != "primary" {
            problems.push(p(
                "internal category requires provenance: primary".to_string()
            ));
        }
    }

    if category == "conformance" && !entry.body.contains("\n## Coverage gaps") {
        problems.push(p(
            "conformance entry lacks a ## Coverage gaps section".to_string()
        ));
    }

    problems
}

/// Check INDEX.md against the entry set, both directions, including the
/// category token on each line. Returns the problems found.
pub fn index_problems(entries: &[Entry]) -> Vec<String> {
    let path = registry_dir().join("INDEX.md");
    let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let mut problems = Vec::new();
    let mut indexed: BTreeMap<String, String> = BTreeMap::new();

    for (i, line) in text.lines().enumerate() {
        let Some(rest) = line.strip_prefix("- [") else {
            continue;
        };
        let parse = || -> Option<(String, String)> {
            let (slug, rest) = rest.split_once("](")?;
            let rest = rest.strip_prefix(&format!("{slug}.md)"))?;
            let rest = rest.strip_prefix(" — ")?;
            let (category, _title) = rest.split_once(" — ")?;
            Some((slug.to_string(), category.to_string()))
        };
        match parse() {
            Some((slug, category)) => {
                if indexed.insert(slug.clone(), category).is_some() {
                    problems.push(format!("INDEX.md:{}: duplicate slug {slug}", i + 1));
                }
            }
            None => problems.push(format!(
                "INDEX.md:{}: malformed entry line (want `- [slug](slug.md) — category — title`)",
                i + 1
            )),
        }
    }

    let known: BTreeMap<&str, &str> = entries
        .iter()
        .map(|e| (e.stem.as_str(), e.scalar("category")))
        .collect();
    for (slug, category) in &indexed {
        match known.get(slug.as_str()) {
            None => problems.push(format!("INDEX.md lists {slug} but no entry file exists")),
            Some(want) if *want != category => problems.push(format!(
                "INDEX.md says {slug} is {category} but the entry says {want}"
            )),
            Some(_) => {}
        }
    }
    for slug in known.keys() {
        if !indexed.contains_key(*slug) {
            problems.push(format!("entry {slug} is missing from INDEX.md"));
        }
    }
    problems
}

/// Check that every `consumers` and `verification` path exists in the
/// workspace. List items equal to `n/a` are rejected: an internal entry
/// always has real consumers, that is the point of writing it.
pub fn path_problems(entries: &[Entry]) -> Vec<String> {
    let root = workspace_root();
    let mut problems = Vec::new();
    for entry in entries {
        for key in LIST_KEYS {
            for item in &entry.lists[key] {
                if !root.join(item).exists() {
                    problems.push(format!(
                        "{}: {key} path {item} does not exist in the workspace",
                        entry.path.display()
                    ));
                }
            }
        }
    }
    problems
}

/// Check the `vendor/` directory against the entries: every subdirectory
/// belongs to an entry whose `vendor-status` names it, every such entry has
/// its directory, and every vendored directory verifies against its
/// `SHA256SUMS` (every file pinned, via [`crate::vendored::verify_all`]).
pub fn vendor_problems(entries: &[Entry]) -> Vec<String> {
    let vendor_dir = registry_dir().join("vendor");
    let mut problems = Vec::new();

    let mut on_disk = BTreeSet::new();
    if vendor_dir.is_dir() {
        for dirent in fs::read_dir(&vendor_dir).expect("read_dir vendor/") {
            let path = dirent.expect("dir entry").path();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            if path.is_dir() {
                on_disk.insert(name);
            } else {
                problems.push(format!(
                    "vendor/{name} is a stray file, not a <slug>/ directory"
                ));
            }
        }
    }

    let mut claimed = BTreeSet::new();
    for entry in entries {
        let vendor = entry.scalar("vendor-status");
        let Some(path) = vendor.strip_prefix("vendored-at-path ") else {
            continue;
        };
        let Some(rest) = path.strip_prefix("docs/references/vendor/") else {
            continue; // vendored elsewhere (e.g. tests/vectors/, pinned by ADR-0042)
        };
        let slug_dir = rest.trim_end_matches('/');
        claimed.insert(slug_dir.to_string());
        if on_disk.contains(slug_dir) {
            crate::vendored::verify_all(vendor_dir.join(slug_dir));
        } else {
            problems.push(format!(
                "{}: vendor-status names vendor/{slug_dir}/ but it does not exist",
                entry.path.display()
            ));
        }
    }
    for name in on_disk.difference(&claimed) {
        problems.push(format!("vendor/{name}/ exists but no entry claims it"));
    }
    problems
}

/// Extract the text between `<!-- BEGIN GENERATED: name -->` and
/// `<!-- END GENERATED: name -->` in a registry document, for the
/// generator-pinned registry entries (category `registry`): the pin test
/// renders the canonical block from the `ferrodec-ieee` types and asserts
/// byte equality with the committed block.
pub fn generated_block(entry_stem: &str, name: &str) -> String {
    let path = registry_dir().join(format!("{entry_stem}.md"));
    let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let begin = format!("<!-- BEGIN GENERATED: {name} -->\n");
    let end = format!("<!-- END GENERATED: {name} -->");
    let start = text
        .find(&begin)
        .unwrap_or_else(|| panic!("{}: missing {begin:?}", path.display()))
        + begin.len();
    let stop = text[start..]
        .find(&end)
        .unwrap_or_else(|| panic!("{}: missing {end:?}", path.display()));
    text[start..start + stop].to_string()
}

fn is_iso_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b.iter().enumerate().all(|(i, c)| {
            if i == 4 || i == 7 {
                *c == b'-'
            } else {
                c.is_ascii_digit()
            }
        })
}

fn parse_entry(path: &Path, stem: &str) -> Result<Entry, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("read failed: {e}"))?;
    let rest = text
        .strip_prefix("---\n")
        .ok_or("entry must start with a --- frontmatter fence")?;
    let (front, body) = rest
        .split_once("\n---\n")
        .ok_or("frontmatter fence never closes")?;

    let mut scalars = BTreeMap::new();
    let mut lists: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut open_list: Option<String> = None;

    for (i, line) in front.lines().enumerate() {
        let err = |msg: String| format!("frontmatter line {}: {msg}", i + 2);
        if let Some(item) = line.strip_prefix("  - ") {
            let key = open_list
                .clone()
                .ok_or_else(|| err("list item outside a list".to_string()))?;
            lists.get_mut(&key).unwrap().push(unquote(item).to_string());
            continue;
        }
        let (key, value) = line
            .split_once(':')
            .ok_or_else(|| err(format!("expected key: value, got {line:?}")))?;
        let key = key.trim().to_string();
        let value = value.trim();
        if value.is_empty() {
            if lists.insert(key.clone(), Vec::new()).is_some() {
                return Err(err(format!("duplicate list {key}")));
            }
            open_list = Some(key);
        } else {
            open_list = None;
            if scalars
                .insert(key.clone(), unquote(value).to_string())
                .is_some()
            {
                return Err(err(format!("duplicate field {key}")));
            }
        }
    }

    Ok(Entry {
        stem: stem.to_string(),
        scalars,
        lists,
        body: body.to_string(),
        path: path.to_path_buf(),
    })
}

/// Strip one symmetric pair of double quotes, if present.
fn unquote(s: &str) -> &str {
    s.strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(s)
}
