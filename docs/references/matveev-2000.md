---
slug: matveev-2000
category: algorithm
citation: "Matveev, E. M. An explicit lower bound for a homogeneous rational linear form in logarithms of algebraic numbers, II (2000). Izvestiya: Mathematics, exact volume/pages RECALLED, NOT VERIFIED."
canonical: "pending verification"
doi: "pending verification"
archived: "pending (no URL pinned until the citation is verified)"
archive-date: n/a
retrieved: n/a
sha256: n/a
license: "publisher copyright; pointer only."
vendor-status: pointer-only
rot-risk: standards-body
provenance: recalled
consumers:
  - docs/decisions/plans/2026-08-10-s5-transcendence-measure-memo.md
verification:
  - "S5 spike only; NOT specialist verified. Only the ORDER of the constants was used (10^10-10^11 for two logarithms), and the memo's conclusion (floors near 10^(-10^16): termination theorem, no practical budget) is robust to several orders of slop."
---

# Matveev 2000: explicit linear forms in logarithms

The explicit Baker-theory constants behind the S5 memo's section 2:
boundary floors for exp/ln/pow/atan2 exist effectively, but at a
depth (~10^16 digits) that closes the termination question and
nothing else. Registered so the negative result is citable without
re-deriving it.
