---
slug: brisebarre-muller-2007
category: algorithm
citation: "Brisebarre, N., Muller, J.-M. Correct rounding of algebraic functions. RAIRO Theoretical Informatics and Applications 41(1), 2007, pp. 71-83."
canonical: "https://www.rairo-ita.org/articles/ita/abs/2007/01/ita06004/ita06004.html"
doi: "10.1051/ita:2007002"
archived: "https://web.archive.org/web/20260417092410/https://www.rairo-ita.org/articles/ita/abs/2007/01/ita06004/ita06004.html"
archive-date: "2026-04-17"
retrieved: "2026-08-05"
sha256: n/a
license: "EDP Sciences; open access record on EuDML (eudml.org/doc/250075). Pointer and archive."
vendor-status: pointer-only
rot-risk: stable-publisher
provenance: primary
consumers:
  - docs/decisions/0060-liouville-floors-algebraic-group.md
verification:
  - tools/liouville_probe.py
notes: "The journal generalization of the exclusion zone program: diophantine approximation bounds on how far intermediate computations must be carried to correctly round algebraic functions. ADR-0060's Engine B is the same mathematical family specialized to power of ten denominators and the IEEE decimal boundary form (35 digit numerators); this entry grounds the claim that the approach is the standard one for algebraic functions rather than an invention of the lane, and the paper's sharper per degree bounds are the first place to look if the decimal floors ever need tightening. Citation metadata verified against the EuDML record; the publisher abstract page already carried a 2026-04-17 Wayback capture, recorded above; the EuDML record (archived 2024-04-15) is the metadata fallback."
---

# Brisebarre and Muller 2007 (correct rounding of algebraic functions)

The diophantine approximation treatment of correctly rounding
algebraic functions; ADR-0060's Engine B belongs to this family.
Cited as method precedent and as the tightening reserve.
