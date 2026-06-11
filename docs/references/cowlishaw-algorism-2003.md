---
slug: cowlishaw-algorism-2003
category: history
citation: "Cowlishaw, M. F. Decimal Floating-Point: Algorism for Computers. Proceedings of the 16th IEEE Symposium on Computer Arithmetic (ARITH-16), 2003."
canonical: "https://speleotrove.com/decimal/IEEE-cowlishaw-arith16.pdf"
doi: "10.1109/ARITH.2003.1207666"
archived: "https://web.archive.org/web/20260310005038/https://speleotrove.com/decimal/IEEE-cowlishaw-arith16.pdf"
archive-date: "2026-03-10"
retrieved: "2026-06-11"
sha256: n/a
license: "Copyright IEEE; the speleotrove copy is the author's posted version. Pointer and archive; no vendored copy."
vendor-status: legally-cannot
rot-risk: academic-personal
provenance: primary
consumers:
  - docs/references/decimal-thread.md
verification:
  - tests/conformance.rs
notes: "The case-for-decimal paper: why binary floating point misrepresents human-facing quantities, what the commercial data actually contains (the survey of decimal data in databases), and the design rationale (cohorts, unnormalized coefficients, the 854 inheritance) that became IEEE 754-2008's decimal half. The intellectual mission statement behind the format ferrodec implements."
---

# Decimal Floating-Point: Algorism for Computers (Cowlishaw 2003)

The ARITH-16 paper is where the modern decimal floating point design
is argued from first principles: most numeric data humans produce and
consume is decimal; binary representation of such data is a silent
source of error; and a decimal format with cohorts and preferred
exponents can carry the scale information commercial arithmetic
needs. The paper's design carried into the GDA specification,
decNumber, IBM z hardware DFP, and then IEEE 754-2008. ferrodec's
choice to implement the decimal formats, and to treat cohort
semantics as conformance-bearing rather than cosmetic, descends from
exactly this argument; the decimal-thread entry carries the longer
lineage.
