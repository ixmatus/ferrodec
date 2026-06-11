---
slug: failure-self-referential-metamorphic
category: failure
citation: "ferrodec failure museum: metamorphic identities whose both sides flowed through the same kernel verified nothing (caught at design review, recorded in ADR-0025)."
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
  - docs/decisions/0025-metamorphic-identity-tests.md
verification:
  - tests/property_metamorphic.rs
notes: "Closed; ADR-0025 names the kept and the dropped identities. Museum record because the pattern (a check that compares a computation with itself) recurs in every oracle design."
---

# Failure: self-referential identities that verified nothing

**What nearly shipped.** The metamorphic suite's first draft included
identities that are true by construction: `log10_kernel` is literally
`ln(x) * const`, so `log_b(x) * ln(b) = ln(x)` tests `ln` against
itself; `tanh_kernel` is `sinh/cosh`, so `tanh = sinh/cosh` is the
definition restated; `exp2` and `pow(2, x)` shared one
`exp_from_extended(x * ln 2)` path, so their agreement compared a
computation with itself. Green forever, evidence never.

**Why it nearly passed review.** Each identity is mathematically
true and looks like an independent cross-check until the kernel call
graph is drawn. The failure mode is not a wrong test but a vacuous
one, which no failing case can ever expose; only reading the
implementation against the test catalogs it.

**The fix.** ADR-0025 keeps only identities whose two sides take
genuinely distinct computational paths, with condition-number-derived
tolerances, and documents per identity why the paths are independent
(the `acosh` near 1 case is the instructive keeper: its `log1p` path
makes the naive reconstruction independent exactly there).

**The lesson.** An oracle's value is its independence, and
independence is a property of the call graph, not of the
mathematics. Every cross-check added to any suite carries the burden
of stating what the two sides do not share. The same discipline named
the verification blind spots in the 2026-06-09 review prompt (each
checker judged by what it cannot see).
