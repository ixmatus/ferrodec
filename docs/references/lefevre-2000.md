---
slug: lefevre-2000
category: algorithm
citation: "Lefèvre, V. Moyens arithmétiques pour un calcul fiable. PhD thesis, École normale supérieure de Lyon, 2000 (theses.fr 2000ENSL0142)."
canonical: "https://www.vinc17.net/research/papers/these.pdf"
doi: none
archived: "https://web.archive.org/web/20250814192734/https://www.vinc17.net/research/papers/these.pdf"
archive-date: "2025-08-14"
retrieved: "2026-06-11"
sha256: n/a
license: "Author hosted thesis; French theses are publicly defensible documents. Pointer and archive; not vendored."
vendor-status: pointer-only
rot-risk: academic-personal
provenance: secondary
consumers:
  - docs/decisions/0032-correctly-rounded-transcendentals.md
  - docs/decisions/0033-worst-case-margin-completeness.md
verification:
  - tests/vectors/transcend
notes: "The hardest-to-round search program: Lefèvre's thesis and the Lefèvre-Muller worst case campaigns established that correctly rounded elementary functions can rest on an empirically bounded worst case hardness plus a fixed working precision margin. ADR-0032 names the mechanism after Lefèvre and Muller; ferrodec's Arb search is the same idea executed with certified ball arithmetic. The theses.fr record (archived 2025-09-12, web.archive.org/web/20250912163924) is the bibliographic anchor; the author hosted PDF is the full text."
---

# Lefèvre 2000 (hardest-to-round search)

The thesis behind the proof shape of ADR-0032: instead of Ziv's
unbounded adaptive loop, search the input space for the hardest
rounding cases, then run a fixed working precision whose margin
dominates the worst case found. ferrodec's frozen corpus, exhaustive
Decimal32 sweep, and the 30 plus orders of magnitude kernel headroom
are that program, with Arb's certified enclosures standing in for
Lefèvre's specialized search algorithms. The author hosts the full
text on a personal page, hence the rot class and the archive; the
theses.fr registry entry is the durable bibliographic pointer.
