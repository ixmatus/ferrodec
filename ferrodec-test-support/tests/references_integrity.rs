//! Default-on guard for the `docs/references/` registry (ADR-0052).
//!
//! Asserts the registry's machine-checkable invariants on every test run:
//! schema completeness per category, INDEX.md synchronization in both
//! directions, consumer path existence, and vendored-copy hash integrity.
//! Registry document pin checks (the generator-rendered enum blocks) live
//! beside these once the registry documents land.

use ferrodec_test_support::references;

/// Every entry parses and satisfies its category's field requirements.
#[test]
fn schema() {
    let entries = references::parse_all();
    let problems: Vec<String> = entries
        .iter()
        .flat_map(references::schema_problems)
        .collect();
    assert!(
        problems.is_empty(),
        "registry schema check failed ({} problem(s)):\n  {}\n\
         The normative schema is docs/references/SCHEMA.md (ADR-0052).",
        problems.len(),
        problems.join("\n  "),
    );
}

/// INDEX.md and the entry set agree in both directions, category included.
#[test]
fn index_sync() {
    let entries = references::parse_all();
    let problems = references::index_problems(&entries);
    assert!(
        problems.is_empty(),
        "INDEX.md sync check failed ({} problem(s)):\n  {}",
        problems.len(),
        problems.join("\n  "),
    );
}

/// Every `consumers` and `verification` path exists in the workspace, so a
/// rename of a cited file is caught here instead of rotting silently.
#[test]
fn consumer_paths_exist() {
    let entries = references::parse_all();
    let problems = references::path_problems(&entries);
    assert!(
        problems.is_empty(),
        "registry path check failed ({} problem(s)):\n  {}",
        problems.len(),
        problems.join("\n  "),
    );
}

/// `vendor/` and the entries agree: every vendored directory is claimed by
/// an entry, every claiming entry has its directory, and every directory
/// verifies against its SHA256SUMS.
#[test]
fn vendor_integrity() {
    let entries = references::parse_all();
    let problems = references::vendor_problems(&entries);
    assert!(
        problems.is_empty(),
        "registry vendor check failed ({} problem(s)):\n  {}",
        problems.len(),
        problems.join("\n  "),
    );
}
