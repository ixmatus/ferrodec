---
slug: mpmath
category: oracle
citation: "Johansson, F. et al. mpmath: a Python library for arbitrary-precision floating-point arithmetic, version 1.x. mpmath.org."
canonical: "https://mpmath.org/"
doi: none
archived: "https://web.archive.org/web/20260609032917/https://mpmath.org/"
archive-date: "2026-06-09"
retrieved: "2026-06-11"
sha256: n/a
license: "BSD 3-Clause."
vendor-status: pointer-only
rot-risk: community-run
provenance: secondary
consumers:
  - tools/diff_oracle.py
  - tools/gen_anchor_band_vectors.py
  - tests/differential.rs
  - docs/decisions/0026-independent-transcendental-oracles.md
  - docs/decisions/0050-anchor-band-reformulations.md
  - docs/testing.md
verification:
  - tests/vectors/transcend/anchor_bands
notes: "Covers the special function surface libmpdec lacks (exp2, log2, cbrt, the trig, inverse trig, and hyperbolic families, atan2) in the differential harness, and generated the anchor band corpus at 160 dps with a libmpdec cross-check (ADR-0050). Adaptive precision, not certified: a missing interpreter or an undecidable case is a diagnostic skip, never a gate; certified enclosures are arb-flint's job."
---

# mpmath

mpmath is the breadth oracle: an arbitrary precision Python library
with the whole §9.2 function surface, reached through the same
`tools/diff_oracle.py` subprocess as libmpdec and used offline by
`tools/gen_anchor_band_vectors.py` to produce the committed anchor
band corpus (867 vectors, ADR-0050). Its results are adaptive rather
than certified, which is why it ranks below Arb in the oracle stack
(docs/testing.md) and why the ADR-0051 note about sub 10^-100
relative corrections names a higher precision offline pass as the
closing step. Maintained by the same author as Arb, but as a
community project with multiple contributors, hence the rot class.
