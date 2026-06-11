---
slug: num-bigint
category: oracle
citation: "The rust-num project. num-bigint: big integer types for Rust, version 0.4. crates.io."
canonical: "https://crates.io/crates/num-bigint"
doi: none
archived: "none (crates.io pages are an SPA that does not capture; the registry guarantees immutable availability of published versions)"
archive-date: n/a
retrieved: "2026-06-11"
sha256: n/a
license: "MIT OR Apache-2.0."
vendor-status: pointer-only
rot-risk: community-run
provenance: secondary
consumers:
  - ferrodec-test-support/src/oracle.rs
  - tests/oracle_soundness.rs
  - docs/decisions/0021-exact-oracle-supersedes-ulp-envelope.md
  - docs/testing.md
verification:
  - tests/oracle_soundness.rs
notes: "The exact integer arithmetic under the ADR-0021 oracle: add, subtract, multiply, fma, divide, and squareRoot recomputed with unbounded integers and rounded per direction, asserting the single correct value bit for bit, no tolerance. Chosen as the boring, widely vetted pure Rust bignum; the oracle's own soundness has its own test. Dev-only; the shipped crates remain dependency-free."
---

# num-bigint (rust-num)

num-bigint carries the exact oracle tier: for the closed arithmetic
operations the correctly rounded answer is computable directly from
unbounded integer arithmetic, so the oracle asserts equality, not
closeness (ADR-0021 retired the ULP-tolerance envelope on exactly
this argument). `ferrodec-test-support/src/oracle.rs` implements the
recomputation and `tests/oracle_soundness.rs` guards the oracle
itself. The differential and conformance tiers exist because not
every operation is closed-form exact; where one is, this tier is the
strongest possible check at property-test cost.
