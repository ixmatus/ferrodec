//! Default-on guard for the `docs/references/` registry (ADR-0052).
//!
//! Asserts the registry's machine-checkable invariants on every test run:
//! schema completeness per category, INDEX.md synchronization in both
//! directions, consumer path existence, and vendored-copy hash integrity.
//! Registry document pin checks (the generator-rendered enum blocks) live
//! beside these once the registry documents land.

use std::fmt::Write as _;

use ferrodec_ieee::{IeeeClass, RoundingMode, Status};
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

/// The rounding-mode registry document byte-matches a block rendered from
/// `RoundingMode` itself. The exhaustive match (no wildcard arm) makes a
/// new variant a compile error here before it can be a stale document.
#[test]
fn registry_rounding_modes_pinned() {
    const ALL: [RoundingMode; 5] = [
        RoundingMode::NearestEven,
        RoundingMode::NearestAway,
        RoundingMode::TowardZero,
        RoundingMode::TowardPositive,
        RoundingMode::TowardNegative,
    ];
    let mut want = String::new();
    for mode in ALL {
        let (name, ieee, gloss) = match mode {
            RoundingMode::NearestEven => (
                "NearestEven",
                "roundTiesToEven",
                "to nearest with ties to even (the default)",
            ),
            RoundingMode::NearestAway => (
                "NearestAway",
                "roundTiesToAway",
                "to nearest with ties away from zero",
            ),
            RoundingMode::TowardZero => ("TowardZero", "roundTowardZero", "truncation"),
            RoundingMode::TowardPositive => ("TowardPositive", "roundTowardPositive", "ceiling"),
            RoundingMode::TowardNegative => ("TowardNegative", "roundTowardNegative", "floor"),
        };
        let _ = writeln!(want, "- `{name}`: IEEE 754-2019 {ieee}, {gloss}.");
    }
    assert_eq!(
        references::generated_block("registry-rounding-modes", "rounding-modes"),
        want,
        "registry-rounding-modes.md generated block is stale; \
         paste the expected block from this assertion's left side"
    );
}

/// The status-flag registry document byte-matches a block rendered from
/// `Status`'s public flag constants and predicates, and the flag universe
/// still has exactly the documented population.
#[test]
fn registry_status_flags_pinned() {
    let flags: [(Status, &str, &str, &str); 6] = [
        (
            Status::INVALID,
            "INVALID",
            "invalid()",
            "the operation has no useful definition",
        ),
        (
            Status::DIV_BY_ZERO,
            "DIV_BY_ZERO",
            "div_by_zero()",
            "finite non-zero numerator divided by zero",
        ),
        (
            Status::OVERFLOW,
            "OVERFLOW",
            "overflow()",
            "the rounded result exceeds the largest finite magnitude",
        ),
        (
            Status::UNDERFLOW,
            "UNDERFLOW",
            "underflow()",
            "the result is tiny, below the smallest normal magnitude",
        ),
        (
            Status::INEXACT,
            "INEXACT",
            "inexact()",
            "the rounded result differs from the infinitely precise result",
        ),
        (
            Status::CLAMPED,
            "CLAMPED",
            "clamped()",
            "the preferred quantum was clamped (informational, IEEE 754-2019 §7.4)",
        ),
    ];
    // A seventh flag bit upstream forces this count, and the doc, to move.
    assert_eq!(
        Status::from_bits_truncate(0xFF).bits().count_ones(),
        6,
        "Status flag universe changed; update registry-status-flags.md and this table"
    );
    let mut want = String::new();
    for (flag, name, predicate, gloss) in flags {
        assert_eq!(flag.bits().count_ones(), 1, "{name} is not a single bit");
        let bit = flag.bits().trailing_zeros();
        let _ = writeln!(
            want,
            "- `{name}` (bit {bit}, predicate `{predicate}`): {gloss}."
        );
    }
    assert_eq!(
        references::generated_block("registry-status-flags", "status-flags"),
        want,
        "registry-status-flags.md generated block is stale; \
         paste the expected block from this assertion's left side"
    );
}

/// The class registry document byte-matches a block rendered from
/// `IeeeClass`; the exhaustive match makes an eleventh class a compile
/// error before it can be a stale document.
#[test]
fn registry_ieee_classes_pinned() {
    const ALL: [IeeeClass; 10] = [
        IeeeClass::SignalingNaN,
        IeeeClass::QuietNaN,
        IeeeClass::NegativeInfinity,
        IeeeClass::NegativeNormal,
        IeeeClass::NegativeSubnormal,
        IeeeClass::NegativeZero,
        IeeeClass::PositiveZero,
        IeeeClass::PositiveSubnormal,
        IeeeClass::PositiveNormal,
        IeeeClass::PositiveInfinity,
    ];
    let mut want = String::new();
    for class in ALL {
        let (name, gloss) = match class {
            IeeeClass::SignalingNaN => (
                "SignalingNaN",
                "signaling NaN; most operations consume it and raise INVALID",
            ),
            IeeeClass::QuietNaN => ("QuietNaN", "quiet NaN; propagates without INVALID"),
            IeeeClass::NegativeInfinity => ("NegativeInfinity", "negative infinity"),
            IeeeClass::NegativeNormal => (
                "NegativeNormal",
                "negative finite at or above the minimum normal magnitude",
            ),
            IeeeClass::NegativeSubnormal => (
                "NegativeSubnormal",
                "negative finite strictly between zero and the minimum normal magnitude",
            ),
            IeeeClass::NegativeZero => ("NegativeZero", "negative zero"),
            IeeeClass::PositiveZero => ("PositiveZero", "positive zero"),
            IeeeClass::PositiveSubnormal => (
                "PositiveSubnormal",
                "positive finite strictly between zero and the minimum normal magnitude",
            ),
            IeeeClass::PositiveNormal => (
                "PositiveNormal",
                "positive finite at or above the minimum normal magnitude",
            ),
            IeeeClass::PositiveInfinity => ("PositiveInfinity", "positive infinity"),
        };
        let _ = writeln!(want, "- `{name}`: {gloss}.");
    }
    assert_eq!(
        references::generated_block("registry-ieee-classes", "ieee-classes"),
        want,
        "registry-ieee-classes.md generated block is stale; \
         paste the expected block from this assertion's left side"
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
