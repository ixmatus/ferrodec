---
slug: failure-anchor-band-cancellation
category: failure
citation: "ferrodec failure museum: anchor absorption near 0 and 1 collapsed the kernel's relative error model to absolute, falsifying ADR-0032 in bands (found 2026-06-09, fixed by ADR-0050 / fd-aqs.6)."
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
  - docs/decisions/0050-anchor-band-reformulations.md
  - docs/archive/REPORT-rigorous-review-2026-06-09.md
verification:
  - tests/transcend_anchor_bands.rs
notes: "Closed; the canonical detail lives in ADR-0050. This entry is the museum's why-the-guards-missed-it record."
---

# Failure: anchor absorption broke the relative error model in bands

**What shipped.** Formulas that hand `1 ± tiny` (or `x` whose square
falls below resolution) to a 50 significant digit representation turn
a `1e-49` relative error budget into `1e-49` absolute: the tiny part
is absorbed by the additive anchor before the function ever sees it.
`atanh`, `asinh`, `ln` near 1, `asin`/`acos` near plus and minus 1,
and `pow` with near-1 bases returned values wrong far beyond the
proof envelope in those bands, falsifying ADR-0032's contract there.

**Why every guard missed it.** The Arb worst case search samples; the
hazard bands around the anchors are measure-tiny and the sampler
never drew them. decTest has no transcendental vectors at all on the
fixed formats. The astro-float faithful suites used magnitude guards
that skirted the same decades. The proof envelope argument was sound
for the formula as written on paper and silently unsound for the
formula as evaluated, because the error model (relative) stopped
matching the computation (absolute after absorption).

**The fix.** Cancellation-free reformulations (log1p forms, factored
radicands; Goldberg's catalog, the Handbook's chapter 11 patterns)
restore the relative model, and a committed 867 vector anchor band
corpus with arbitrary messy coefficients pins the class (ADR-0050,
with the directed-mode seam completed by ADR-0051).

**The lesson.** A proof about a formula is a proof about its
evaluation order; reformulation is a correctness tool, not a
performance one, near additive anchors. Sampled corpora need
hazard-band stratification, not just more samples.
