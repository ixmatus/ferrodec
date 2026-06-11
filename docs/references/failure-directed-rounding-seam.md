---
slug: failure-directed-rounding-seam
category: failure
citation: "ferrodec failure museum: directed modes mis-rounded grid-stuck small arguments because the rounding seam discarded the residual's sign (found 2026-06-09, fixed by ADR-0051 / fd-aqs.7)."
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
  - docs/decisions/0051-residual-across-rounding-seam.md
  - docs/archive/REPORT-rigorous-review-2026-06-09.md
verification:
  - tests/transcend_anchor_bands.rs
notes: "Closed; ADR-0051 holds the mechanism. Museum record of the blind spot pattern: directed modes fail in places nearest modes provably cannot."
---

# Failure: grid-stuck directed rounding lost the residual's sign

**What shipped.** For small arguments where `f(x)` rounds to `x`
itself on the format grid (`sin(1E-40)` at Decimal32 and family),
the directed modes returned the grid value unmoved: `TowardNegative`
should step down to the next representable when the true result sits
below `x`, and the kernel could not know, because the seam between
the Extended kernel and the format rounding discarded whether the
infinitely precise result sat above or below the returned midpoint.

**Why every guard missed it.** Nearest-mode results were provably
correct in exactly these bands, so the sampled corpus (mostly
nearest-heavy) and the identities never tripped; the directed-mode
exact-output filter in the corpus generator had deliberately dropped
such candidates as undecidable (the ADR-0033 Slice A lesson), which
removed precisely the cases that would have caught it. The blind
spot was structural: every oracle that could see the band could not
see the mode, and the filter hid the rest.

**The fix.** The kernel carries a signed residual across the
rounding seam, so the format rounding knows which side of the grid
value the true result occupies; band corpus rows in all four
directed modes pin it (ADR-0051, completing ADR-0050's program).

**The lesson.** Directed modes are not a cheap parameterization of
nearest: they need their own witnesses in every band, and a filter
that drops undecidable cases must be audited for what it makes
invisible.
