---
slug: mpdecimal
category: oracle
citation: "Krah, S. mpdecimal (libmpdec), the arbitrary precision decimal library underlying CPython's decimal module. bytereef.org, 2008 to 2025."
canonical: "https://www.bytereef.org/mpdecimal/index.html"
doi: none
archived: "https://web.archive.org/web/20260517130620/https://www.bytereef.org/mpdecimal/index.html"
archive-date: "2026-05-17"
retrieved: "2026-06-11"
sha256: n/a
license: "libmpdec is BSD 2-Clause (license text ships with the source distribution); the documentation site is copyright Stefan Krah 2008 to 2025."
vendor-status: pointer-only
rot-risk: single-maintainer
provenance: secondary
consumers:
  - tools/diff_oracle.py
  - tests/differential.rs
  - ferrodec-decimal/tests/differential.rs
  - ferrodec-decimal64/tests/differential.rs
  - ferrodec-decimal32/tests/differential.rs
  - docs/decisions/0026-independent-transcendental-oracles.md
  - docs/testing.md
verification:
  - tests/differential.rs
  - ferrodec-decimal/tests/differential.rs
notes: "Chosen as the differential oracle because it is an independently developed, correctly rounded GDA implementation that ships inside every CPython (no extra install), giving a zero-setup cross-check for arithmetic and the exp/ln/log10/power family. The mpmath entry covers the functions libmpdec lacks. Oracle for behavior, never a template for code."
---

# mpdecimal / libmpdec (Krah)

libmpdec is the C library behind CPython's `decimal` module and the
most battle tested independent GDA implementation. ferrodec reaches it
through a Python subprocess (`tools/diff_oracle.py`) behind the
`differential` feature: off by default, never in CI, used for local
sweeps that draw operand decades the fixed oracles skip. The
`ferrodec-decimal` 1-ulp power band versus libmpdec (ADR-0040) and two
nextToward flag bugs decTest missed are among its catches.

bytereef.org is a single maintainer's personal site, the canonical
documentation home, and the rot reason this entry is in the first
archive tier. The library documentation is archived at
`https://web.archive.org/web/20251113061800/https://www.bytereef.org/mpdecimal/doc/libmpdec/index.html`.
The library itself needs no vendoring: it arrives via CPython, and the
differential harness treats a missing interpreter as a diagnostic
skip, never a gate.
