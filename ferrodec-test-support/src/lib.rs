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
