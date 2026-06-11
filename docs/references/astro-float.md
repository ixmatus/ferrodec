---
slug: astro-float
category: oracle
citation: "stencillogic. astro-float: arbitrary precision floating point numbers implemented purely in Rust, version 0.9. crates.io."
canonical: "https://crates.io/crates/astro-float"
doi: none
archived: "none (crates.io pages are an SPA that does not capture; the registry guarantees immutable availability of published versions)"
archive-date: n/a
retrieved: "2026-06-11"
sha256: n/a
license: "MIT."
vendor-status: pointer-only
rot-risk: single-maintainer
provenance: secondary
consumers:
  - ferrodec-test-support/src/transcend_oracle.rs
  - ferrodec-test-support/Cargo.toml
  - docs/testing.md
verification:
  - tests/property_exp.rs
  - tests/property_hyperbolic.rs
notes: "The faithful (within 1 ulp per direction) oracle for the transcendental property suites, behind the transcend-oracle feature. Chosen over an MPFR binding for the pure Rust dev-dependency posture: clean CI on every platform beats the last percentage of battle testing, and faithful is the right strength for a hard-defect catcher under the exact tiers above it. Single maintainer on crates.io, hence the rot class; the crate is pinned by Cargo.lock and cached by the registry, so rot risk is to future maintenance, not to reproducibility."
---

# astro-float

astro-float is the always-available, pure Rust faithful oracle: the
property suites drive millions of random inputs through it asserting
the within 1 ulp contract per rounding direction, catching hard
defects cheaply where the certified tiers (Arb corpus, exact integer
oracle) are either frozen or arithmetic-only. A faithful check stayed
meaningful after the contract tightened to correctly rounded
(ADR-0032) because faithful is the strictly weaker bound: any
violation is still a hard defect. Wrapped by
`ferrodec-test-support/src/transcend_oracle.rs` with magnitude guards
at the decade breaks where its own rounding becomes the suspect.
