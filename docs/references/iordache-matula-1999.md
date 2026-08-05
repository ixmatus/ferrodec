---
slug: iordache-matula-1999
category: algorithm
citation: "Iordache, C. S., Matula, D. W. On Infinitely Precise Rounding for Division, Square Root, Reciprocal and Square Root Reciprocal. Proc. 14th IEEE Symposium on Computer Arithmetic (ARITH-14), 1999, pp. 233-240."
canonical: "http://acsel-lab.com/arithmetic/arith14/papers/ARITH14_Iordache.pdf"
doi: "10.1109/ARITH.1999.762849"
archived: "https://web.archive.org/web/20240423032626/http://acsel-lab.com/arithmetic/arith14/papers/ARITH14_Iordache.pdf"
archive-date: "2024-04-23"
retrieved: "2026-08-05"
sha256: "40cbed72c7189d0ac47a00f1773d1542a1e58fffdb6a81c7fb7af2eccebe95fc"
license: "IEEE copyright; the acsel-lab proceedings mirror is the accessible copy. Pointer and archive; not vendored."
vendor-status: pointer-only
rot-risk: academic-personal
provenance: secondary
consumers:
  - docs/decisions/0060-liouville-floors-algebraic-group.md
verification:
  - tools/liouville_probe.py
notes: "The ARITH-14 precursor of the exclusion zone bounds for division, sqrt, and rsqrt that lang-muller-2001 simplifies and generalizes; ADR-0060 cites it as lineage and second witness. Provenance is marked secondary because the results were consulted through Lang and Muller's Table 4 comparison, not by a full read of this paper; the PDF was fetched and hash pinned so a future full read has the exact artifact. Anything load bearing beyond the comparison table must be checked against the paper itself before use."
---

# Iordache and Matula 1999 (infinitely precise rounding)

Lineage entry: the ARITH-14 precursor of the algebraic function
exclusion zone bounds. ADR-0060 cites it as history and second
witness, via lang-muller-2001's comparison table; the fetched PDF is
hash pinned for the eventual full read.
