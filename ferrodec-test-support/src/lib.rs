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
