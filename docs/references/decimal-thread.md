---
slug: decimal-thread
category: history
citation: "The decimal arithmetic thread: REXX through GDA, decNumber, IBM z DFP hardware, and IEEE 754-2008, narrated against the speleotrove FAQ and the primary entries it links."
canonical: "https://speleotrove.com/decimal/decifaq.html"
doi: none
archived: "https://web.archive.org/web/20260226142706/https://speleotrove.com/decimal/decifaq.html"
archive-date: "2026-02-26"
retrieved: "2026-06-11"
sha256: n/a
license: "The FAQ pages carry the same IBM-permission copyright as the rest of speleotrove.com; this narrative entry is repo-original prose."
vendor-status: pointer-only
rot-risk: academic-personal
provenance: secondary
consumers:
  - docs/references/ieee-854-1987.md
  - docs/references/cowlishaw-algorism-2003.md
verification:
  - tests/conformance.rs
notes: "The lineage in one place so future documentation does not reconstruct it from scattered ADRs: why decimal exists as a hardware format, where GDA's semantics come from, and why a 754 storage implementation sometimes cannot express what the GDA model can. RPN and calculator ancestry are deliberately out of scope here (that history belongs to the downstream calculator project, not to this crate family)."
---

# The decimal arithmetic thread

Why decimal floating point exists, in one paragraph of lineage:
human-facing arithmetic (money, measurements, regulation) is decimal,
and scaled-integer workarounds in binary systems kept failing at the
edges. Cowlishaw's REXX language (1979 onward) made exact decimal
arithmetic a language feature; its semantics grew into the General
Decimal Arithmetic specification, whose context model descends from
IEEE 854-1987 (radix independent floating point) rather than the
binary 754-1985. decNumber implemented GDA as the reference; the
ARITH-16 paper (cowlishaw-algorism-2003) made the public case; IBM
shipped hardware DFP in POWER6 and System z; and IEEE 754-2008
standardized the formats with two encodings, DPD (the hardware
lineage) and BID (the software lineage Intel championed). 754-2019
is the current revision and ferrodec's authority.

The load bearing consequence for ferrodec: GDA semantics are an 854
inheritance carried in a wide working context, while ferrodec's
storage formats are 754 interchange types. Most of the time the two
agree exactly; where they cannot (operands pre-clamped at parse, the
ADR-0048 CLAMPED residual), the difference is the 854-versus-754
ancestry showing through, not a defect in either model. That is why
the conformance harness reasons about decNumber's working exponent
when it skips, and why the registry keeps the lineage entries
(ieee-854-1987, ieee-754-2008) alongside the live standard.
