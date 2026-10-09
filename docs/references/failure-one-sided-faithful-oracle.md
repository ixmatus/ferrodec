---
slug: failure-one-sided-faithful-oracle
category: failure
citation: "ferrodec failure museum: the faithful transcendental oracle accepted any result below the true value (2026-10 review finding C1-1; fix tracked in the remediation epic)."
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
  - ferrodec-test-support/src/transcend_oracle.rs
  - docs/archive/review-2026-10/wave1-verification.md
verification:
  - ferrodec-test-support/src/transcend_oracle.rs
notes: "Open at write time (2026-10-09); gains the fix commit and the negative control test path when the fix lands. Pairs with failure-comparator-masking: the instrument, not the code, carried the defect."
---

# Failure: the faithful oracle was one sided

**What shipped.** `cmp_approx` in `transcend_oracle.rs` called
astro-float's `abs_cmp`, which astro-float-num 0.3.6 documents as an
absolute value comparison and implements as a signed `cmp`. Every
consumer of `within_faithful_ulp`, `faithful_side_ok` and
`assert_faithful`, which is every `property_*` transcendental suite on
all three formats, therefore accepted any result below the true value:
the review's reproducer shows `exp(1)` accepting 0, 1.5 and −7 as
faithful, and a TowardPositive one ulp low mutant of `sinh` passing
every property, frozen vector and directed suite (A5-8).

**Why every guard missed it.** The oracle was never given a result it
had to reject. There was no negative control, and the dependency's
doc comment was taken as its contract.

**The lesson.** Every oracle carries a must fail test. A dependency's
documentation is a map; the value it returns is the territory, and the
two are checked against each other at the first use.

**Status.** Open at write time. The closure (compare absolute values
or the signed difference against ±tol, scale the floor to the format's
smallest subnormal, add negative controls, report the defect upstream)
is a child of the 2026-10 remediation epic; this entry gains the fix
commit when it lands.
