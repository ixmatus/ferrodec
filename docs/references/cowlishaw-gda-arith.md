---
slug: cowlishaw-gda-arith
category: spec
citation: "Cowlishaw, M. F. General Decimal Arithmetic Specification, version 1.70, IBM, 2009."
canonical: "https://speleotrove.com/decimal/decarith.html"
doi: none
archived: "https://web.archive.org/web/20260606044509/https://speleotrove.com/decimal/decarith.html"
archive-date: "2026-06-06"
retrieved: "2026-06-11"
sha256: n/a
license: "Reproduced with permission from IBM, copyright 1997, 2009 by International Business Machines Corporation. The permission is Cowlishaw's, not a redistribution grant."
vendor-status: legally-cannot
rot-risk: academic-personal
provenance: primary
consumers:
  - ferrodec-decimal/src/lib.rs
  - docs/decisions/0005-half-down-05up-wontfix.md
  - docs/decisions/0014-display-notation-divergence.md
  - docs/decisions/0038-arbitrary-precision-decimal.md
  - docs/decisions/0039-general-dectest-conformance.md
  - docs/decisions/0041-gda-miscellaneous-operations.md
verification:
  - tests/vectors
  - ferrodec-decimal/tests/vectors
notes: "The GDA specification defines the context model, the extended operation set beyond IEEE 754, the toSci/toEng string forms, and the cohort/ideal-exponent semantics ferrodec-decimal implements. IEEE 754-2019 is the storage and operation authority for the fixed formats; GDA is the authority for everything the decTest suite exercises beyond it. No alternative spec exists for this surface."
---

# General Decimal Arithmetic Specification (Cowlishaw)

The GDA specification is the parent document of the decTest conformance
suite and the semantic authority for the `ferrodec-decimal` arbitrary
precision type: context (precision, Emax, Emin, rounding, clamp), the
miscellaneous and logical operation set (ADR-0031, ADR-0041), the
`toSci` display form the whole family standardized on (ADR-0014), and
the two GDA-only rounding directives ferrodec deliberately rejects
(ADR-0005). The fixed formats take their storage and required
operations from IEEE 754-2019 and use GDA only where the standard is
silent.

The document lives on speleotrove.com, Mike Cowlishaw's personal site;
the hosted PDF (`decarith.pdf`) is archived at
`https://web.archive.org/web/20260305042322/https://speleotrove.com/decimal/decarith.pdf`.
The page carries IBM copyright reproduced with permission, so the
registry points and archives but never vendors a copy. A predecessor
lineage (IEEE 854, ANSI X3.274) is described in the document itself.
