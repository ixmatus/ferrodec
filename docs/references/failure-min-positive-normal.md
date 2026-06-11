---
slug: failure-min-positive-normal
category: failure
citation: "ferrodec failure museum: MIN_POSITIVE_NORMAL encoded the wrong value on all three fixed formats (found 2026-06-09; fix tracked as fd-aqs.1)."
canonical: n/a
doi: n/a
archived: n/a
archive-date: n/a
retrieved: n/a
sha256: n/a
license: "repo (MIT OR Apache-2.0)"
vendor-status: n/a
rot-risk: n/a
provenance: primary
consumers:
  - docs/archive/REPORT-rigorous-review-2026-06-09.md
verification:
  - tests/conformance.rs
notes: "Open at write time: the constant is still wrong pending fd-aqs.1; this entry records the post mortem now and gains the fix pointer when the fix lands (the accretion ritual)."
---

# Failure: MIN_POSITIVE_NORMAL wrong on every fixed format

**What shipped.** The named constant encoded `1E-33` (Decimal128),
`1E-15` (Decimal64), and `1E-6` (Decimal32) instead of the documented
`1E-6143`, `1E-383`, and `1E-95`. The construction placed
`BIAS - PRECISION + 1` in the biased exponent field, which evaluates
to `-E_MIN` rather than `E_MIN + BIAS`; the correct biased value is
`PRECISION - 1`. Two independent reviewers found it in the 2026-06-09
review.

**Why every guard missed it.** No oracle ever reads a named Rust
constant: decTest exercises operations, the exact oracle recomputes
operations, Kani proves operation properties. The only assertions
touching the constant checked `is_normal()`, which the wrong value
also satisfies. A constant is a claim with no operation attached, so
the whole operation-shaped verification stack was blind to it.

**The lesson.** Named constants need value pins (assert the decoded
cohort, not a predicate), and the registry's constants discipline
exists because a constant that nothing recomputes is documentation
wearing a type signature.

**Status.** Open at write time; fd-aqs.1 carries the fix and the
value pins. This entry gains the fix commit pointer when it lands.
