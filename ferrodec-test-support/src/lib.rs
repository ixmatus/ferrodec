//! Shared test scaffolding for the ferrodec family.
//!
//! Workspace-internal crate (`publish = false`). Provides the IBM
//! decTest parser, the directive-aware [`conformance::Context`], the
//! per-file expectation guard from ADR-0010, and a generic
//! [`conformance::run_suite`] driver. Each sibling's
//! `tests/conformance.rs` is reduced to its
//! type-specific dispatch closure plus a one-line invocation of the
//! driver.
//!
//! Adding a new precision-specific harness: implement a
//! `fn run_case(case: &TestCase, ctx: &Context) -> Outcome` closure
//! that dispatches on `case.op` and routes through your decimal type's
//! methods, then call [`conformance::run_suite`] from a `#[test]`.

pub mod conformance;

/// Content-hash integrity check for vendored decTest fixtures (ADR-0042).
/// Re-hashes each directory's committed `*.decTest` files against its
/// `SHA256SUMS` manifest so a silent byte drift, or an unpinned new file,
/// fails the build. Pure std plus `sha2`; default-on, the byte-level
/// companion to the ADR-0010 pass-count pins.
pub mod vendored;

/// Schema guard for the `docs/references/` registry (ADR-0052). Parses
/// each entry's constrained frontmatter and exposes the checks asserted
/// by the default-on `tests/references_integrity.rs`: per-category field
/// requirements, INDEX.md synchronization in both directions, consumer
/// path existence, and vendored-copy hash integrity. Pure std plus the
/// existing `sha2` path; no network.
pub mod references;

/// Loader for the Arb/FLINT frozen hard-to-round vector corpus
/// (Phase 2 of fd-cb6, ADR-0026). Pure std, no oracle and no C-FFI in
/// the path: the corpus is committed data, so the per-crate
/// frozen-vector tests are default-on and run in standard CI.
pub mod frozen;

/// Exact decimal significant-digit rounding shared by the Arb frozen
/// corpus consumers (fd-cb6, ADR-0026). Pure std, no `rug`/MPFR and no
/// C-FFI in the path: hoisted out of the `mpfr-gate` test so the
/// default-on `round_dec` meta-test can exercise the proof-tier
/// rounding keystone in lockstep with the Python generator.
pub mod round_dec;

pub mod oracle;

/// Process-and-protocol harness for the Python/libmpdec differential
/// (Track 3, plan 2026-05-17). Always compiles (std-only, no extra
/// dependency); the Python subprocess is reached only from the
/// `differential`-feature test binaries, so a default `cargo test`
/// never spawns it.
pub mod differential;

/// Generic faithful-rounding oracle harness for the transcendental
/// property suites (ADR-0021). Gated behind the `transcend-oracle`
/// feature so `astro-float` is compiled only for the dev-dependents
/// that assert the faithful contract, never for the conformance-only
/// path.
#[cfg(feature = "transcend-oracle")]
pub mod transcend_oracle;
