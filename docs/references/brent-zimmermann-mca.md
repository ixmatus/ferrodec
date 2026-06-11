---
slug: brent-zimmermann-mca
category: algorithm
citation: "Brent, R. P., Zimmermann, P. Modern Computer Arithmetic, version 0.5.9. Cambridge University Press, 2010 (author hosted electronic version)."
canonical: "https://members.loria.fr/PZimmermann/mca/pub226.html"
doi: "10.1017/CBO9780511921698"
archived: "https://web.archive.org/web/20260213054438/https://members.loria.fr/PZimmermann/mca/mca-cup-0.5.9.pdf"
archive-date: "2026-02-13"
retrieved: "2026-06-11"
sha256: n/a
license: "Free electronic version: copying allowed for non-commercial use only (per the authors' page). The NC restriction is incompatible with this repository's MIT OR Apache-2.0 terms, so pointer and archive, no vendored copy."
vendor-status: pointer-only
rot-risk: academic-personal
provenance: secondary
consumers:
  - ferrodec-decimal/src/transc/series.rs
  - ferrodec-decimal/src/transc/consts.rs
  - docs/decisions/0044-decbig-perf-pass-results.md
  - docs/decisions/0046-decimal-perf-followups.md
verification:
  - ferrodec-decimal/tests/conformance.rs
notes: "The working reference for the ferrodec-decimal performance algorithms: binary splitting for rational series (§4.9 territory, with haible-papanikolaou-1998 as the primary), rectangular series evaluation, and the division and reciprocal analysis behind the rejected Newton division candidate (ADR-0046 records the bench verdict). Author hosted on a personal LORIA page, hence the rot class and the archived PDF."
---

# Modern Computer Arithmetic (Brent and Zimmermann)

MCA is the algorithms handbook for the DecBig performance work: the
ADR-0044 pass (Karatsuba threshold, rectangular ln series) and the
ADR-0046 follow-ups (binary split constants, exp argument halving,
the Newton division candidate that measured neutral and was reverted
per the stop-loss rule) all cite it as the consulted treatment. The
free PDF lives on Zimmermann's personal page under a non-commercial
copying grant, which the license gate reads as pointer plus archive,
never a vendored copy.
