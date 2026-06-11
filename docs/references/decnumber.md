---
slug: decnumber
category: oracle
citation: "Cowlishaw, M. F. The decNumber C library, version 3.68, IBM, 2010."
canonical: "https://speleotrove.com/decimal/decnumber.html"
doi: none
archived: "https://web.archive.org/web/20260307015721/https://speleotrove.com/decimal/decnumber.html"
archive-date: "2026-03-07"
retrieved: "2026-06-11"
sha256: n/a
license: "The decNumber package is distributed under the ICU License (free open source, vendorable in principle). The documentation pages carry IBM copyright reproduced with Cowlishaw's permission."
vendor-status: pointer-only
rot-risk: single-maintainer
provenance: secondary
consumers:
  - KNOWN_ISSUES.md
  - docs/decisions/0031-gda-decnumber-extensions.md
  - docs/decisions/0048-clamped-fidelity-and-bid-residual.md
  - docs/decisions/0049-gda-extension-residue-closure.md
verification:
  - tests/conformance.rs
notes: "The GDA reference implementation: decTest expected values were produced against it, so its behavioral model (notably the wide working exponent that BID encodings cannot reproduce, ADR-0048) explains every structural residual in the conformance run. Behavioral reference only; ferrodec never reads its source as a template (code provenance discipline), and no FFI binding exists or is wanted."
---

# decNumber (IBM / Cowlishaw)

decNumber is the reference implementation of the General Decimal
Arithmetic specification and the implementation the decTest expected
values trace to. ferrodec consults it as a documented behavioral
model, never as code: the GDA extension operation semantics
(ADR-0031, ADR-0049) and the CLAMPED analysis (ADR-0048) reason about
its wide working exponent model to explain why a BID storage format
cannot raise certain flags the suite expects.

The canonical home is the same personal site as the specification
(speleotrove.com), hence the single-maintainer rot class and the
archive. The package itself is ICU licensed and survives in several
downstream mirrors (ICU, GCC's libdecnumber); if the page dies, the
code will not, but the documentation pages are the part worth the
archive.
