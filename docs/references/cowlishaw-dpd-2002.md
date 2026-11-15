---
slug: cowlishaw-dpd-2002
category: algorithm
citation: "Cowlishaw, M. F. Densely Packed Decimal Encoding. IEE Proceedings, Computers and Digital Techniques 149(3), May 2002, pp. 102-104. Summary page: A Summary of Densely Packed Decimal encoding, speleotrove.com."
canonical: "https://speleotrove.com/decimal/DPDecimal.html"
doi: none
archived: "https://web.archive.org/web/20261009081946/https://speleotrove.com/decimal/DPDecimal.html"
archive-date: "2026-10-09"
retrieved: "2026-10-09"
sha256: n/a
license: "Copyright IBM Corporation and Mike Cowlishaw, 2000, 2007. All rights reserved (summary page footer). The journal paper is IET copyright. Pointer only; the equations are transcribed, the page is never copied."
vendor-status: pointer-only
rot-risk: academic-personal
provenance: primary
consumers:
  - src/dpd.rs
  - ferrodec-decimal64/src/dpd.rs
  - ferrodec-decimal32/src/dpd.rs
  - docs/decisions/0009-dpd-interchange.md
verification:
  - src/verify/dpd.rs
  - ferrodec-decimal32/src/verify/dpd.rs
  - tests/conformance.rs
  - ferrodec-decimal64/tests/conformance.rs
notes: "The boolean declet equations the three DPD codecs transcribe; the module headers say 'transcribed verbatim' so the code audits line by line against the page. IEEE 754-2019 §3.5.2 defines DPD by tables; the equations are Cowlishaw's. The IEE DOI is recalled as 10.1049/ip-cdt:20020407 and stays 'none' until checked against the publisher page. dqEncode, ddEncode and dsEncode (cowlishaw-dectest) are the behavioural verification under the dpd feature. Added by the 2026-10 deep review (reader D4, finding D4-11); an earlier capture from 2026-06-30 (web.archive.org/web/20260630144141/...) also exists."
---

# Cowlishaw, Densely Packed Decimal (2002)

The summary page carries the paper's abstract, the Chen-Ho lineage, and
the encode and decode equations for the ten bit declet that packs three
decimal digits. The three `dpd.rs` files state that their equations are
transcribed from this page so a reader can audit the codec line by line;
their module comment "Cowlishaw §3.5.2" conflates the page with the IEEE
clause that defines the same mapping by tables (the clause is the
normative source, the page is the algorithmic one). The provenance of the
transcription from an "All rights reserved" page is a question the
2026-10 review recorded for Parnell (reader D4, provenance spot check);
the equations themselves are the published algorithm, which the review
treats as general knowledge, and the code's identifiers and structure
are its own.
