---
slug: lang-muller-2001
category: algorithm
citation: "Lang, T., Muller, J.-M. Bounds on Runs of Zeros and Ones for Algebraic Functions. Proc. 15th IEEE Symposium on Computer Arithmetic (ARITH-15), 2001, pp. 13-20."
canonical: "http://www.acsel-lab.com/arithmetic/arith15/papers/ARITH15_Lang.pdf"
doi: "10.1109/ARITH.2001.930099"
archived: "https://web.archive.org/web/20230803014739/http://www.acsel-lab.com/arithmetic/arith15/papers/ARITH15_Lang.pdf"
archive-date: "2023-08-03"
retrieved: "2026-08-05"
sha256: "d1122bca808cccac79f433c603eb9ad6a8b6ae0110658acad78bab773c8f0bf2"
license: "IEEE copyright (0-7695-1150-3/01); the acsel-lab proceedings mirror is the accessible copy. Pointer and archive; not vendored."
vendor-status: pointer-only
rot-risk: academic-personal
provenance: primary
consumers:
  - docs/decisions/0060-liouville-floors-algebraic-group.md
  - tools/liouville_probe.py
verification:
  - tools/liouville_probe.py
notes: "The binary format exclusion zone bounds for algebraic functions (reciprocal, division, square root, reciprocal square root, q-th roots, 2D norms, normalization), derived via digit recurrence residual bounds: runs after the rounding bit reach at most n+1 (sqrt, norm), 2n+1 (rsqrt), about (n+1)(q-1) (q-th root). ADR-0060's decimal floors are rederived from scratch by a different route (conjugate identity plus denominator integrality); this paper is the independent cross check that every scaling law matches (rsqrt ~3n, q-th root ~qn, sqrt and norm ~2n), the source of the attained case families (sqrt(1.00..01), sqrt(0.11..1): the S = k^2+1 and k^2+k shapes the probe confirmed near sharp in decimal), and the honesty precedent that the rsqrt bound is unattained through double precision with sharpness open. The sha256 pins the PDF as fetched from the canonical URL on the retrieval date. Derivation over analogy: nothing is transcribed; the agreement is the evidence. DOI verified against IEEE Xplore document 930099."
---

# Lang and Muller 2001 (runs of zeros and ones for algebraic functions)

The published binary analog of ADR-0060's Liouville floors, by an
independent method (digit recurrence residuals rather than the
conjugate and denominator argument). Grounds the lemma's external
cross check leg: same exponent laws, same near attaining families,
same open sharpness for reciprocal square root.
