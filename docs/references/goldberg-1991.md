---
slug: goldberg-1991
category: algorithm
citation: "Goldberg, D. What Every Computer Scientist Should Know About Floating-Point Arithmetic. ACM Computing Surveys 23(1), 1991."
canonical: "https://docs.oracle.com/cd/E19957-01/806-3568/ncg_goldberg.html"
doi: "10.1145/103162.103163"
archived: "https://web.archive.org/web/20260608130707/https://docs.oracle.com/cd/E19957-01/806-3568/ncg_goldberg.html"
archive-date: "2026-06-08"
retrieved: "2026-06-11"
sha256: n/a
license: "Copyright ACM 1991; the Oracle hosted reprint is published with permission. Pointer and archive only."
vendor-status: legally-cannot
rot-risk: stable-publisher
provenance: secondary
consumers:
  - docs/decisions/0050-anchor-band-reformulations.md
verification:
  - tests/vectors/transcend/anchor_bands
notes: "The canonical free introduction to floating point error analysis, cited for the cancellation patterns and relative error reasoning behind the anchor band reformulations (log1p and factored radicand forms, ADR-0050), and named in the IEEE spec entries as proxy literature for readers without the paywalled standard."
---

# Goldberg 1991

The survey is the working reference for cancellation analysis: the
ADR-0050 fix derives its log1p and factored radicand reformulations
from exactly the catastrophic cancellation patterns this paper
catalogs. The Oracle hosted reprint (an appendix of the Numerical
Computation Guide) is the canonical free copy and the archived one;
the ACM DOI is the formal citation.
