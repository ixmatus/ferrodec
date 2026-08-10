---
slug: mpfr
category: oracle
citation: "Fousse, L., Hanrot, G., Lefèvre, V., Pélissier, P., Zimmermann, P. MPFR: A multiple-precision binary floating-point library with correct rounding. ACM TOMS 33(2), 2007."
canonical: "https://www.mpfr.org/"
doi: "10.1145/1236463.1236468"
archived: "https://web.archive.org/web/20260530152157/https://www.mpfr.org/"
archive-date: "2026-05-30"
retrieved: "2026-06-11"
sha256: n/a
license: "GNU LGPL v3 or later. Dev-only and feature gated; never linked into a shipped artifact."
vendor-status: pointer-only
rot-risk: community-run
provenance: secondary
consumers:
  - ferrodec-test-support/tests/mpfr_gate.rs
  - ferrodec-test-support/Cargo.toml
  - ferrodec-transcend/src/exact_pi.rs
  - ferrodec-transcend/src/sincospi.rs
  - ferrodec-transcend/src/inverse_trig_pi.rs
  - docs/decisions/0026-independent-transcendental-oracles.md
  - docs/decisions/0061-pi-scaled-family-design.md
  - docs/testing.md
verification:
  - ferrodec-test-support/tests/mpfr_gate.rs
notes: "Since ADR-0061 (D4) the 4.2.2 manual additionally serves as a documentation proxy: the mpfr_sinu/mpfr_asinu family entries (u = 2), which the manual states follow IEEE 754-2019 sinPi..atan2Pi, are one of the two independent sources (with C23 Annex F, see c23-draft) for transcribing the pi-scaled special-value rows; the vendored library is also the cross-validation leg for those seven operations through the native functions. Manual pinned at GNU MPFR 4.2.2 (https://www.mpfr.org/mpfr-4.2.2/mpfr.html; archived https://web.archive.org/web/20251007021713/https://www.mpfr.org/mpfr-4.2.2/mpfr.html). The industrial gold standard for correctly rounded arbitrary precision. Reached through the rug FFI binding behind the off-by-default mpfr-gate feature (ADR-0026 phase 3): the gate recomputes the whole Arb frozen corpus and asserts agreement (0 disagreements at last full run). Kept out of CI and out of default builds because pure Rust dev dependencies are preferred and LGPL FFI never belongs in the embedded artifact; its value is the independent re-derivation, not continuous presence."
---

# MPFR (via rug, mpfr-gate)

MPFR is the second, fully independent certification leg under the
correctly rounded transcendental claim: Arb produced the frozen
corpus, MPFR re-derives every value through a different codebase and
algorithm family, and the `mpfr-gate` test asserts the two agree.
The accept rule (ADR-0026) is conjunctive: Arb enclosure decisive AND
MPFR agrees. The C dependency is the reason for the feature gate; the
default test run and CI prove the contract from the committed corpus
alone, and the gate exists for refresh time and audits.
