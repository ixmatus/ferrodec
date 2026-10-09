---
slug: intel-dfp-math-library
category: oracle
citation: "Intel Corporation. Intel Decimal Floating-Point Math Library (libbid), the BID reference implementation of IEEE 754-2008 decimal arithmetic. Version recalled as 2.0 Update 2; verify before any reliance."
canonical: "https://www.netlib.org/misc/intel/"
doi: none
archived: "https://web.archive.org/web/20260302081943/https://www.netlib.org/misc/intel/"
archive-date: "2026-03-02"
retrieved: n/a
sha256: n/a
license: "Recalled as BSD 3-Clause (the eula.txt in the distribution); quote it verbatim at the first reliance. Pointer only; oracle for outputs, never a template."
vendor-status: pointer-only
rot-risk: single-maintainer
provenance: secondary
consumers:
  - src/bid.rs
  - ferrodec-decimal64/src/bid.rs
  - ferrodec-decimal32/src/bid.rs
  - src/decimal.rs
  - ferrodec-decimal64/src/decimal.rs
  - ferrodec-decimal32/src/decimal.rs
verification:
  - src/decimal.rs
notes: "The three bid.rs files cite 'Intel reference' mask constants for the specials, and three tests are named infinity_bit_patterns_match_intel_reference; only those four special patterns are pinned, and both the IEEE 754-2019 §3.5.2 layout and the shipped one share them (2026-10 review finding B1-1, bead fd-joi3: the finite layout is not §3.5.2 BID). The library is the natural behavioural oracle for finite BID bit patterns (1 is 0x3040…01 at decimal128, 0x31C0000000000001 at decimal64, 0x32800001 at decimal32); no build of it existed on the review machine. The 2026-10-09 save attempt timed out; the 2026-03-02 capture answers 200 but was not opened, so retrieved stays n/a and provenance secondary until the licence file is read."
---

# Intel Decimal Floating-Point Math Library

The reference BID implementation, by the authors of the encoding. Its
role here is an oracle for bit patterns and for values when a BID
interchange question arises, as the 2026-10 review's finding B1-1 did:
the ferrodec `to_bits` layout agrees with it only on the four special
patterns the tests pin. The remediation of fd-joi3 should pin finite
patterns against this library's output (or against §3.5.2 literals
derived by hand), never adopt its code: it is C with Intel's
identifiers, and ferrodec's provenance rule treats such a source as an
oracle for behaviour, not a guide to shape.
