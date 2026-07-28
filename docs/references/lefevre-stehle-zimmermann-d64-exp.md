---
slug: lefevre-stehle-zimmermann-d64-exp
category: conformance
citation: "Lefèvre, V., Stehlé, D., Zimmermann, P. Worst Cases for the Exponential Function in the IEEE 754r decimal64 Format. In: Reliable Implementation of Real Number Algorithms: Theory and Practice, LNCS 5045, Springer, 2008, pp. 114-126."
canonical: "https://inria.hal.science/inria-00068731"
doi: "10.1007/978-3-540-85521-7_7"
archived: "https://web.archive.org/web/20251119101245/https://inria.hal.science/inria-00068731"
archive-date: "2025-11-19"
retrieved: "2026-07-27"
sha256: n/a
license: "Springer LNCS text is not redistributable; the HAL deposit is the open access author copy. The worst case values themselves are mathematical facts. Pointer and archive; the future corpus is our own Arb rederivation, not a vendored copy."
vendor-status: pointer-only
rot-risk: stable-publisher
provenance: primary
consumers:
  - docs/decisions/0059-correctly-rounded-decimal128-lane.md
  - docs/decisions/plans/2026-07-25-correctly-rounded-d128-transcendentals-lane.md
verification:
  - tests/vectors/transcend
notes: "The only published worst case table for any decimal format: every decimal64 exp bad case within 10^-15 ulp of a rounding breakpoint (all modes), for |x| >= 3e-11, computed with the SLZ lattice method. The lane's external anchor: recertifying these rows through our own Arb pipeline grounds the Decimal64 exp claim in someone else's theorem, and comparing our sampled minimum against their true worst case is the second sampling calibration datum beside ADR-0034's Decimal32 one. License check before any vendoring is the first task of its bead."
---

# Lefèvre, Stehlé, Zimmermann 2008 (decimal64 exp worst cases)

The lone externally certified hardest to round table in decimal
floating point. The lane recertifies each row with Arb and commits
the result as `tests/vectors/transcend/external/`, making the
committed corpus our derivation doubly grounded rather than a copied
table. The stated worst case: `exp(9.407822313572878e-2)` whose
value carries fifteen consecutive zeros after the rounding digit.

## Coverage gaps

One function (`exp`), one format (decimal64), domain `|x| >= 3e-11`,
and only cases within 10^-15 ulp of a breakpoint. It says nothing
about decimal128, nothing about the other nineteen functions, and
nothing below its domain cut. The gaps are exactly why the lane's own
campaign corpora exist; the entry anchors, it does not cover.
